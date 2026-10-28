//! A frozen clock and fresh process must not reuse an old journal decision.
#![cfg(target_os = "linux")]
use anyhow::{bail, Result};
use link_cli::{
    CommitMode, FileTransitionLog, GenericTransactionsDecorator, LogRetentionPolicy,
    NamedTypeLinks, NamedTypesDecorator,
};
use std::{
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn bounded(command: &mut Command) -> Result<()> {
    let mut child = command
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if child.try_wait()?.is_some() {
            break;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            bail!("Finite transaction identity child did not finish.");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn restart(prior_abort: bool, rollback: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let clock = directory.path().join("frozen-realtime.so");
    bounded(
        Command::new("cc")
            .args(["-shared", "-fPIC", "-O2"])
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/common/frozen_realtime.c"
            ))
            .args(["-o"])
            .arg(&clock)
            .arg("-ldl"),
    )?;
    for mode in [
        "seed",
        if rollback { "rollback" } else { "exit" },
        "verify",
        "verify",
        "following",
        "verify-following",
    ] {
        bounded(
            Command::new("bash")
                .args([
                    "-c",
                    "ulimit -v 524288\nulimit -s 8192\nexec \"$@\"",
                    "identity-child",
                ])
                .arg(std::env::current_exe()?)
                .args(["--exact", "identity_child", "--ignored", "--nocapture"])
                .env("LD_PRELOAD", &clock)
                .env("LINK_IDENTITY_DIRECTORY", directory.path())
                .env("LINK_IDENTITY_MODE", mode)
                .env("LINK_IDENTITY_PRIOR_ABORT", prior_abort.to_string()),
        )?;
    }
    let old = std::fs::read_to_string(directory.path().join("old-id"))?;
    let new = std::fs::read_to_string(directory.path().join("new-id"))?;
    assert_ne!(old, new, "A recovered decision was reused for a new owner.");
    Ok(())
}

#[test]
fn committed_id_does_not_commit_a_new_pending_write() -> Result<()> {
    restart(false, false)
}
#[test]
fn committed_id_does_not_resurrect_a_new_aborted_write() -> Result<()> {
    restart(false, true)
}
#[test]
fn aborted_id_does_not_hide_a_new_pending_write() -> Result<()> {
    restart(true, false)
}
#[test]
fn aborted_id_does_not_identify_a_new_aborted_write() -> Result<()> {
    restart(true, true)
}

#[test]
#[ignore = "Executed only by bounded frozen-clock restart parents"]
fn identity_child() -> Result<()> {
    let directory = std::env::var("LINK_IDENTITY_DIRECTORY")?;
    let directory = Path::new(&directory);
    let mode = std::env::var("LINK_IDENTITY_MODE")?;
    let mut data = NamedTypesDecorator::new(directory.join("data.links"), false)?;
    // The baseline is outside the transaction whose old outcome is exercised.
    if mode == "seed" {
        let id = data.try_create(11, 12)?;
        data.save()?;
        std::fs::write(directory.join("row"), id.to_string())?;
    }
    let row = std::fs::read_to_string(directory.join("row"))?.parse()?;
    let log = FileTransitionLog::open(directory.join("journal"))?;
    let mut tx = GenericTransactionsDecorator::<u32, _, _>::new(
        data,
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        true,
    )?;
    let prior_abort = std::env::var("LINK_IDENTITY_PRIOR_ABORT")? == "true";
    let baseline = if prior_abort { 11 } else { 13 };
    if mode.starts_with("verify") {
        assert_eq!(
            tx.get(row).unwrap().source,
            if mode == "verify-following" {
                31
            } else {
                baseline
            },
            "A recovered old decision changed a newer transaction's outcome."
        );
        return Ok(());
    }
    let handle = tx.begin_transaction()?;
    eprintln!("{mode}: transaction {:032x}", handle.id);
    if mode == "seed" {
        std::fs::write(directory.join("old-id"), handle.id.to_string())?;
        tx.update(row, 13, 14)?;
        if prior_abort {
            tx.rollback()?;
        } else {
            tx.commit()?;
        }
    } else if mode == "following" {
        tx.update(row, 31, 32)?;
        tx.commit()?;
    } else {
        std::fs::write(directory.join("new-id"), handle.id.to_string())?;
        tx.update(row, 21, 22)?;
        if mode == "exit" {
            // Explicit flush can persist pending rows without a commit decision.
            tx.save()?;
            std::process::exit(0);
        }
        tx.rollback()?;
        assert_eq!(tx.get(row).unwrap().source, baseline);
    }
    tx.save()?;
    Ok(())
}
