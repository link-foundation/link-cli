//! End-to-end CLI tests for the persistent transformation triggers wired up in
//! main.rs: where a trigger is stored, when it fires, and when it stops firing.
//!
//! The expected outputs are the ones the C# CLI produces for the same
//! invocations; see docs/case-studies/issue-100/evidence/cli-parity.

use anyhow::{ensure, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use tempfile::tempdir;

fn clink() -> Command {
    Command::new(env!("CARGO_BIN_EXE_clink"))
}

fn run(db: &Path, args: &[&str]) -> Result<Output> {
    let mut command = clink();
    command.arg("--db").arg(db);
    for arg in args {
        command.arg(arg);
    }
    Ok(command.output()?)
}

fn run_ok(db: &Path, args: &[&str]) -> Result<String> {
    let output = run(db, args)?;
    ensure!(
        output.status.success(),
        "clink {args:?} failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// The database as `--after` prints it, which is what a scenario compares.
fn dump(db: &Path) -> Result<String> {
    run_ok(db, &["--after"])
}

fn triggers_sidecar_for(db: &Path) -> PathBuf {
    let stem = db.file_stem().unwrap().to_string_lossy().into_owned();
    db.parent().unwrap().join(format!("{stem}.triggers.links"))
}

/// A database holding the two point links the trigger scenarios start from.
fn seeded_database(db: &Path) -> Result<()> {
    run_ok(db, &["--query", "() ((1 1) (2 2))"])?;
    Ok(())
}

/// The trigger every scenario stores: whenever link 1 is the point `(1 1)`,
/// repoint it at link 2.
const TRIGGER: &str = "(((1: 1 1)) ((1: 1 2)))";

#[test]
fn always_trigger_fires_on_a_later_write() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    seeded_database(&db)?;

    let stored = run_ok(&db, &["--always", TRIGGER])?;
    assert!(
        stored.starts_with("Always persistent transformation trigger stored: "),
        "storing a trigger should report the address it was stored at; got:\n{stored}"
    );

    // The trigger is not asked to fire here; the write that follows is what
    // gives it the chance to.
    assert_eq!(dump(&db)?, "(1: 1 1)\n(2: 2 2)\n");

    run_ok(&db, &["--query", "() ((2 1))"])?;
    assert_eq!(dump(&db)?, "(1: 1 2)\n(2: 2 2)\n(3: 2 1)\n");
    Ok(())
}

#[test]
fn always_trigger_keeps_firing() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    seeded_database(&db)?;
    run_ok(&db, &["--always", TRIGGER])?;
    run_ok(&db, &["--query", "() ((2 1))"])?;

    // Undo what the trigger did, then write again: it fires a second time.
    run_ok(&db, &["--query", "((1: 1 2)) ((1: 1 1))"])?;
    run_ok(&db, &["--query", "() ((2 2))"])?;
    assert_eq!(dump(&db)?, "(1: 1 2)\n(2: 2 2)\n(3: 2 1)\n");
    Ok(())
}

#[test]
fn once_trigger_fires_only_once() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    seeded_database(&db)?;

    let stored = run_ok(&db, &["--once", TRIGGER])?;
    assert!(
        stored.starts_with("Once persistent transformation trigger stored: "),
        "storing a trigger should report the address it was stored at; got:\n{stored}"
    );

    run_ok(&db, &["--query", "() ((2 1))"])?;
    assert_eq!(dump(&db)?, "(1: 1 2)\n(2: 2 2)\n(3: 2 1)\n");

    // Same undo and write as in the --always scenario; this time nothing
    // repoints link 1, because firing consumed the trigger.
    run_ok(&db, &["--query", "((1: 1 2)) ((1: 1 1))"])?;
    run_ok(&db, &["--query", "() ((2 2))"])?;
    assert_eq!(dump(&db)?, "(1: 1 1)\n(2: 2 2)\n(3: 2 1)\n");
    Ok(())
}

#[test]
fn never_removes_a_stored_trigger() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    seeded_database(&db)?;
    run_ok(&db, &["--always", TRIGGER])?;

    let removed = run_ok(&db, &["--never", TRIGGER])?;
    assert_eq!(removed, "Persistent transformation triggers removed: 1\n");

    run_ok(&db, &["--query", "() ((2 1))"])?;
    assert_eq!(dump(&db)?, "(1: 1 1)\n(2: 2 2)\n(3: 2 1)\n");
    Ok(())
}

#[test]
fn never_on_an_empty_trigger_store_removes_nothing() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    seeded_database(&db)?;

    let removed = run_ok(&db, &["--never", TRIGGER])?;
    assert_eq!(removed, "Persistent transformation triggers removed: 0\n");
    Ok(())
}

#[test]
fn triggers_are_stored_in_the_sidecar_next_to_the_database() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    seeded_database(&db)?;
    run_ok(&db, &["--always", TRIGGER])?;

    assert!(
        triggers_sidecar_for(&db).exists(),
        "the default trigger store belongs next to the database"
    );
    // The trigger lives there and not in the database it guards.
    assert_eq!(dump(&db)?, "(1: 1 1)\n(2: 2 2)\n");
    Ok(())
}

#[test]
fn triggers_file_puts_the_store_where_it_is_asked_to() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    let triggers = dir.path().join("elsewhere.links");
    seeded_database(&db)?;

    run_ok(
        &db,
        &[
            "--triggers-file",
            triggers.to_str().unwrap(),
            "--always",
            TRIGGER,
        ],
    )?;
    assert!(triggers.exists(), "--triggers-file should be honoured");
    assert!(
        !triggers_sidecar_for(&db).exists(),
        "the default sidecar should not be created as well"
    );

    // A store somewhere else has to be pointed at again to keep firing.
    run_ok(&db, &["--query", "() ((2 1))"])?;
    assert_eq!(dump(&db)?, "(1: 1 1)\n(2: 2 2)\n(3: 2 1)\n");

    run_ok(
        &db,
        &[
            "--triggers-file",
            triggers.to_str().unwrap(),
            "--query",
            "() ((2 3))",
        ],
    )?;
    assert_eq!(dump(&db)?, "(1: 1 2)\n(2: 2 2)\n(3: 2 1)\n(4: 2 3)\n");
    Ok(())
}

#[test]
fn embedded_triggers_stay_in_the_main_database() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    seeded_database(&db)?;

    run_ok(&db, &["--embed-triggers", "--always", TRIGGER])?;
    assert!(
        !triggers_sidecar_for(&db).exists(),
        "--embed-triggers should not create a sidecar"
    );

    let dumped = dump(&db)?;
    assert!(
        dumped.contains("(Always: Always Always)"),
        "the trigger schema belongs in the database itself; got:\n{dumped}"
    );

    run_ok(&db, &["--embed-triggers", "--query", "() ((2 1))"])?;
    assert!(
        dump(&db)?.contains("(1: 1 2)\n"),
        "an embedded trigger fires like a sidecar one"
    );
    Ok(())
}

#[test]
fn an_existing_trigger_store_keeps_firing_without_repeating_the_flag() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");
    seeded_database(&db)?;
    run_ok(&db, &["--always", TRIGGER])?;

    // No --triggers here: the store exists, which is enough.
    run_ok(&db, &["--query", "() ((2 1))"])?;
    assert_eq!(dump(&db)?, "(1: 1 2)\n(2: 2 2)\n(3: 2 1)\n");
    Ok(())
}

#[test]
fn only_one_trigger_command_at_a_time() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");

    let output = run(&db, &["--always", "--once", TRIGGER])?;
    assert!(!output.status.success(), "two trigger commands is an error");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Only one of --always, --once, or --never"),
        "the error should name the flags that conflict; got:\n{stderr}"
    );
    Ok(())
}

#[test]
fn a_trigger_command_needs_a_query() -> Result<()> {
    let dir = tempdir()?;
    let db = dir.path().join("data.links");

    let output = run(&db, &["--always"])?;
    assert!(
        !output.status.success(),
        "a trigger without a query is an error"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("require a query"),
        "the error should say a query is missing; got:\n{stderr}"
    );
    Ok(())
}
