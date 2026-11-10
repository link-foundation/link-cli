//! Existing archive entries belong to their original writers.
#![cfg(target_os = "linux")]
use anyhow::{bail, Result};
use link_cli::{
    CommitMode, FileTransitionLog, GenericTransactionsDecorator, LogRetentionPolicy,
    NamedTypeLinks, NamedTypesDecorator,
};
use std::{
    collections::BTreeMap,
    ffi::OsString,
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
            bail!("Finite archive collision child did not finish.");
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

type Snapshot = BTreeMap<OsString, (u8, Vec<u8>)>;
fn snapshot(directory: &Path) -> Result<Snapshot> {
    std::fs::read_dir(directory)?
        .map(|entry| {
            let entry = entry?;
            let kind = entry.file_type()?;
            Ok((
                entry.file_name(),
                (
                    if kind.is_dir() {
                        2
                    } else if kind.is_symlink() {
                        1
                    } else {
                        0
                    },
                    if kind.is_dir() || (kind.is_symlink() && !entry.path().exists()) {
                        vec![]
                    } else {
                        std::fs::read(entry.path())?
                    },
                ),
            ))
        })
        .collect()
}
fn preserved(before: &Snapshot, after: &Snapshot) {
    for (name, value) in before {
        assert_eq!(
            after.get(name),
            Some(value),
            "Archive entry {name:?} was replaced."
        );
    }
    assert_eq!(
        after.len(),
        before.len() + 1,
        "A new archive must have its own entry."
    );
}

fn contract(kind: &str) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    let clock = root.join("clock.so");
    bounded(
        Command::new("cc")
            .args(["-shared", "-fPIC", "-O2"])
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/common/frozen_realtime.c"
            ))
            .arg("-o")
            .arg(&clock)
            .arg("-ldl"),
    )?;
    let child = |label: &str, mode: &str, value: u32| -> Result<()> {
        bounded(
            Command::new("bash")
                .args([
                    "-c",
                    "ulimit -v 524288\nulimit -s 8192\nexec \"$@\"",
                    "archive-child",
                ])
                .arg(std::env::current_exe()?)
                .args(["--exact", "archive_child", "--ignored", "--nocapture"])
                .env("LD_PRELOAD", &clock)
                .env("LINK_ARCHIVE_DIRECTORY", root)
                .env("LINK_ARCHIVE_LABEL", label)
                .env("LINK_ARCHIVE_MODE", mode)
                .env("LINK_ARCHIVE_VALUE", value.to_string()),
        )
    };
    child("first", "seed", 21)?;
    let archive = root.join("archive");
    let original = std::fs::read_dir(&archive)?.next().unwrap()?.path();
    if kind != "file" {
        let protected = root.join("original-archive");
        std::fs::rename(&original, &protected)?;
        if kind.contains("symlink") {
            let target = if kind == "dangling-symlink" {
                root.join("missing-target")
            } else {
                protected
            };
            std::os::unix::fs::symlink(target, &original)?;
        } else {
            std::fs::create_dir(&original)?;
        }
    }
    let mut before = snapshot(&archive)?;
    for (label, value) in [("second", 31), ("third", 41)] {
        child(label, "seed", value)?;
        let after = snapshot(&archive)?;
        preserved(&before, &after);
        before = after;
    }
    child("first", "following", 51)?;
    let after = snapshot(&archive)?;
    preserved(&before, &after);
    for (label, value) in [("first", 51), ("second", 31), ("third", 41)] {
        child(label, "verify", value)?;
        child(label, "verify", value)?;
    }
    assert_eq!(snapshot(&archive)?, after);
    assert!(!root.join("missing-target").exists());
    Ok(())
}
#[test]
fn shared_archive_directory_preserves_older_files_after_clock_restart() -> Result<()> {
    contract("file")
}
#[test]
fn archive_creation_does_not_follow_an_existing_symlink() -> Result<()> {
    contract("symlink")
}
#[test]
fn archive_creation_does_not_create_a_dangling_symlink_target() -> Result<()> {
    contract("dangling-symlink")
}
#[test]
fn archive_creation_preserves_an_existing_directory() -> Result<()> {
    contract("directory")
}
#[test]
#[ignore = "Executed only by finite archive collision parents"]
fn archive_child() -> Result<()> {
    let directory = std::env::var("LINK_ARCHIVE_DIRECTORY")?;
    let root = Path::new(&directory);
    let label = std::env::var("LINK_ARCHIVE_LABEL")?;
    let mode = std::env::var("LINK_ARCHIVE_MODE")?;
    let value = std::env::var("LINK_ARCHIVE_VALUE")?.parse::<u32>()?;
    let mut data = NamedTypesDecorator::new(root.join(format!("{label}.links")), false)?;
    if mode == "seed" {
        assert_eq!(data.try_create(11, 12)?, 1);
    }
    let log = FileTransitionLog::open(root.join(format!("{label}.journal")))?;
    let mut tx = GenericTransactionsDecorator::<u32, _, _>::new(
        data,
        log,
        LogRetentionPolicy::Chunked {
            chunk_size: 1,
            archive_directory: root.join("archive"),
        },
        CommitMode::Sync,
        true,
    )?;
    if mode == "verify" {
        assert_eq!(tx.get(1).unwrap().source, value);
    } else {
        tx.begin_transaction()?;
        tx.update(1, value, 22)?;
        tx.commit()?;
        tx.save()?;
    }
    Ok(())
}
