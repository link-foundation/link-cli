use anyhow::{ensure, Result};
use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

#[test]
fn import_option_reads_numbered_lino_file() -> Result<()> {
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("imported.links");
    let input_path = temp_dir.path().join("input.lino");
    let output_path = temp_dir.path().join("output.lino");
    std::fs::write(&input_path, "(1: 1 1)\n(2: 1 2)\n(3: 2 1)\n")?;

    let output = run_clink(&db_path, &input_path, &output_path)?;

    ensure_success(&output)?;
    assert_eq!(
        std::fs::read_to_string(&output_path)?,
        "(1: 1 1)\n(2: 1 2)\n(3: 2 1)\n"
    );

    Ok(())
}

fn run_clink(db_path: &Path, input_path: &Path, output_path: &Path) -> Result<Output> {
    Ok(Command::new(env!("CARGO_BIN_EXE_clink"))
        .arg("--db")
        .arg(db_path)
        .arg("--import")
        .arg(input_path)
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
