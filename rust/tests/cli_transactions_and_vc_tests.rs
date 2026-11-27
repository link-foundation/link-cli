//! End-to-end CLI tests for the transactions and version-control
//! layers wired up in main.rs. Exercises the option-implies-option
//! semantics (R8/R17) and validates the visible side effects of each
//! family of flags.

use anyhow::{ensure, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

fn clink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_clink"))
}

fn ensure_success(output: &Output) -> Result<()> {
    ensure!(
        output.status.success(),
        "clink failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}

fn run_query(db: &Path, query: &str, extra: &[&str]) -> Result<Output> {
    let mut command = clink();
    command.arg("--db").arg(db);
    for arg in extra {
        command.arg(arg);
    }
    Ok(command.arg(query).output()?)
}

fn transitions_sidecar_for(db: &Path) -> PathBuf {
    let stem = db.file_stem().unwrap().to_string_lossy().into_owned();
    db.parent().unwrap().join(format!("{stem}.transitions.log"))
}

fn vc_sidecar_for(db: &Path) -> PathBuf {
    let stem = db.file_stem().unwrap().to_string_lossy().into_owned();
    db.parent()
        .unwrap()
        .join(format!("{stem}.versioncontrol.links"))
}

#[test]
fn transactions_flag_creates_transitions_sidecar() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");

    let out = run_query(&db, "() ((1 1) (2 2))", &["--transactions"])?;
    ensure_success(&out)?;

    let sidecar = transitions_sidecar_for(&db);
    assert!(
        sidecar.exists(),
        "default transitions sidecar should be created next to the db"
    );
    Ok(())
}

#[test]
fn transactions_log_flag_prints_recorded_transitions() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");

    // Apply two link creations.
    ensure_success(&run_query(&db, "() ((1 1) (2 2))", &["--transactions"])?)?;

    // Then print the log.
    let out = run_query(&db, "", &["--transactions", "--log"])?;
    ensure_success(&out)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.lines().count() >= 2,
        "log should contain at least one line per applied transition; got:\n{stdout}"
    );
    assert!(
        stdout.contains("Create") || stdout.contains("Update"),
        "log should mention transition kind; got:\n{stdout}"
    );
    Ok(())
}

#[test]
fn cli_import_and_query_share_one_commit() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    let input = dir.path().join("input.lino");
    let document: String = (1..=10).map(|i| format!("(k{i}: a{i} b{i})\n")).collect();
    std::fs::write(&input, &document)?;
    let flags = ["--transactions", "--trace", "--in", input.to_str().unwrap()];

    let first = run_query(&db, "", &flags)?;
    ensure_success(&first)?;
    let trace = String::from_utf8_lossy(&first.stderr);
    assert_eq!(trace.matches("Committed tx ").count(), 1, "{trace}");
    // The two files are saved in recovery on open and once more at commit.
    assert_eq!(trace.matches("] Saved ").count(), 4, "{trace}");

    let log_path = transitions_sidecar_for(&db);
    let before = std::fs::read(&log_path)?;
    let unchanged = run_query(&db, "", &flags)?;
    ensure_success(&unchanged)?;
    assert_eq!(std::fs::read(&log_path)?, before);

    let document = document.replacen("(k1: a1 b1)", "(k1: b1 a1)", 1);
    std::fs::write(&input, &document)?;
    let query = run_query(&db, "() ((31 31) (32 32))", &flags)?;
    ensure_success(&query)?;
    assert_eq!(
        String::from_utf8_lossy(&query.stderr)
            .matches("Committed tx ")
            .count(),
        1
    );
    let printed = run_query(&db, "", &["--transactions", "--after"])?;
    ensure_success(&printed)?;
    let text = String::from_utf8(printed.stdout)?;
    for line in document.lines().chain(["(31: 31 31)", "(32: 32 32)"]) {
        assert!(
            text.lines().any(|row| row == line),
            "missing {line} in {text}"
        );
    }
    Ok(())
}

#[test]
fn a_query_error_rolls_back_the_import_in_the_same_invocation() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    let input = dir.path().join("input.lino");
    std::fs::write(&input, "(base: base base)\n")?;
    let flags = ["--vc", "--in", input.to_str().unwrap()];
    ensure_success(&run_query(&db, "", &flags)?)?;

    std::fs::write(&input, "(base: fresh base)\n(new: base fresh)\n")?;
    let failed = run_query(&db, "(((", &flags)?;
    assert!(!failed.status.success());
    assert!(String::from_utf8_lossy(&failed.stderr).contains("Parse error"));
    let printed = run_query(&db, "", &["--vc", "--after"])?;
    ensure_success(&printed)?;
    assert_eq!(printed.stdout, b"(base: base base)\n");
    Ok(())
}

#[cfg(unix)]
#[test]
fn killing_an_import_before_its_commit_recovers_the_original_store() -> Result<()> {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::time::Duration;

    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    let input = dir.path().join("input.lino");
    let fifo = dir.path().join("export.fifo");
    ensure_success(&Command::new("mkfifo").arg(&fifo).output()?)?;
    let flags = ["--transactions", "--in", input.to_str().unwrap()];
    std::fs::write(&input, "(base: base base)\n")?;
    ensure_success(&run_query(&db, "", &flags)?)?;
    std::fs::write(&input, "(base: added base)\n(new: base added)\n")?;

    // --after confirms the writes ran. Opening the export FIFO then blocks
    // before the CLI can make its commit decision, with no timing race.
    let mut child = clink()
        .arg("--db")
        .arg(&db)
        .args(flags)
        .arg("--after")
        .arg("--out")
        .arg(fifo)
        .stdout(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().unwrap();
    let (send, receive) = mpsc::sync_channel(1);
    let reader = std::thread::spawn(move || {
        let result = BufReader::new(stdout)
            .lines()
            .any(|line| line.is_ok_and(|line| line == "(new: base added)"));
        let _ = send.send(result);
    });
    let saw_import = receive.recv_timeout(Duration::from_secs(15));
    let killed = child.kill();
    child.wait()?;
    reader.join().unwrap();
    killed?;
    assert!(saw_import?, "the child did not reach the export barrier");

    let printed = run_query(&db, "", &["--transactions", "--after"])?;
    ensure_success(&printed)?;
    assert_eq!(printed.stdout, b"(base: base base)\n");
    Ok(())
}

#[test]
fn explicit_transactions_file_is_honored() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    let log = dir.path().join("custom-log.links");

    ensure_success(&run_query(
        &db,
        "() ((1 1))",
        &["--transactions-file", log.to_str().unwrap()],
    )?)?;

    assert!(log.exists(), "explicit transactions file must be created");
    assert!(
        !transitions_sidecar_for(&db).exists(),
        "default sidecar should NOT be created when --transactions-file is given"
    );
    Ok(())
}

#[test]
fn vc_flag_creates_version_control_sidecar() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");

    ensure_success(&run_query(&db, "() ((1 1))", &["--vc"])?)?;
    assert!(
        vc_sidecar_for(&db).exists(),
        "version-control sidecar should be created next to the db"
    );
    Ok(())
}

#[test]
fn vc_list_branches_shows_default_branch() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");

    ensure_success(&run_query(&db, "() ((1 1))", &["--vc"])?)?;

    let out = run_query(&db, "", &["--vc", "--list-branches"])?;
    ensure_success(&out)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("main"),
        "list-branches should list the default 'main' branch; got:\n{stdout}"
    );
    assert!(
        stdout.contains('*'),
        "list-branches should mark the current branch with '*'; got:\n{stdout}"
    );
    Ok(())
}

#[test]
fn vc_branch_then_switch_back_creates_new_branch() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");

    ensure_success(&run_query(&db, "() ((1 1))", &["--vc"])?)?;
    ensure_success(&run_query(
        &db,
        "() ((2 2))",
        &["--vc", "--branch", "feature"],
    )?)?;

    let out = run_query(&db, "", &["--vc", "--list-branches"])?;
    ensure_success(&out)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("main"));
    assert!(
        stdout.contains("feature"),
        "feature branch should be created; got:\n{stdout}"
    );
    Ok(())
}

#[test]
fn vc_tag_then_list_tags_round_trip() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");

    ensure_success(&run_query(&db, "() ((1 1))", &["--vc"])?)?;
    ensure_success(&run_query(&db, "", &["--vc", "--tag", "v1"])?)?;

    let out = run_query(&db, "", &["--vc", "--list-tags"])?;
    ensure_success(&out)?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("v1"),
        "list-tags should include the new 'v1' tag; got:\n{stdout}"
    );
    Ok(())
}

#[test]
fn invalid_commit_mode_value_is_rejected() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    let out = run_query(&db, "() ((1 1))", &["--commit-mode", "bogus"])?;
    assert!(
        !out.status.success(),
        "invalid commit-mode value should cause the binary to exit non-zero"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--commit-mode"),
        "stderr should mention --commit-mode; got:\n{stderr}"
    );
    Ok(())
}

#[test]
fn invalid_retention_value_is_rejected() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    let out = run_query(&db, "() ((1 1))", &["--retention", "bogus"])?;
    assert!(
        !out.status.success(),
        "invalid retention value should cause the binary to exit non-zero"
    );
    Ok(())
}

#[test]
fn no_flags_does_not_create_transactions_sidecar() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    ensure_success(&run_query(&db, "() ((1 1))", &[])?)?;
    assert!(
        !transitions_sidecar_for(&db).exists(),
        "running without transactions flags must NOT create a transitions sidecar (R8)"
    );
    assert!(
        !vc_sidecar_for(&db).exists(),
        "running without version-control flags must NOT create a vc sidecar (R17)"
    );
    Ok(())
}
