//! A query clink cannot run is reported on stderr as one error, the way the
//! C# clink reports it.

use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn run_clink(db_path: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_clink"))
        .arg("--db")
        .arg(db_path)
        .args(arguments)
        .output()
        .expect("clink runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_missing_reference_is_a_query_error() {
    let directory = tempdir().unwrap();
    let output = run_clink(&directory.path().join("db.links"), &["() ((2 2))"]);

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        stderr(&output),
        "Error: Query error: Invalid reference to non-existent link '2' in substitution pattern. \
         Link '2' does not exist and will not be created by this operation. \
         Use --auto-create-missing-references to create missing references as point links.\n"
    );
    assert_eq!(stdout(&output), "");
}

#[test]
fn a_malformed_query_is_a_parse_error_pointing_at_the_problem() {
    let directory = tempdir().unwrap();
    let output = run_clink(&directory.path().join("db.links"), &["((("]);

    assert_eq!(output.status.code(), Some(1));
    let stderr = stderr(&output);
    assert!(
        stderr.starts_with("Error: Parse error: Syntax error at line 1, column 4"),
        "{stderr}"
    );
    assert!(stderr.contains("1 | (((\n  |    ^"), "{stderr}");
}

#[test]
fn a_failed_query_leaves_the_store_unchanged() {
    let directory = tempdir().unwrap();
    let db_path = directory.path().join("db.links");
    assert!(run_clink(&db_path, &["() ((1 1))"]).status.success());

    assert_eq!(run_clink(&db_path, &["() ((1 3))"]).status.code(), Some(1));

    let after = run_clink(&db_path, &["--after"]);
    assert!(after.status.success(), "{}", stderr(&after));
    assert_eq!(stdout(&after), "(1: 1 1)\n");
}
