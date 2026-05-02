use anyhow::{ensure, Result};
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

#[test]
fn export_alias_writes_numbered_references() -> Result<()> {
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("numbered.links");
    let output_path = temp_dir.path().join("numbered.lino");

    let output = run_clink(&db_path, "() ((1 1) (2 2))", false, &output_path)?;

    ensure_success(&output)?;
    assert_eq!(
        std::fs::read_to_string(&output_path)?,
        "(1: 1 1)\n(2: 2 2)\n"
    );

    Ok(())
}

#[test]
fn export_alias_writes_named_references() -> Result<()> {
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("named.links");
    let output_path = temp_dir.path().join("named.lino");

    let output = run_clink(&db_path, "() ((child: father mother))", true, &output_path)?;

    ensure_success(&output)?;
    assert_eq!(
        std::fs::read_to_string(&output_path)?,
        "(father: father father)\n(mother: mother mother)\n(child: father mother)\n"
    );

    Ok(())
}

fn run_clink(
    db_path: &Path,
    query: &str,
    auto_create_missing_references: bool,
    output_path: &Path,
) -> Result<Output> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_clink"));
    command.arg("--db").arg(db_path);
    if auto_create_missing_references {
        command.arg("--auto-create-missing-references");
    }

    Ok(command
        .arg(query)
        .arg("--export")
        .arg(output_path)
        .output()?)
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
