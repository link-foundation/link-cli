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

#[test]
fn invalid_import_preserves_database_after_reopening() -> Result<()> {
    for (option, flags) in [
        ("--import", vec![]),
        ("--in", vec!["--transactions"]),
        ("--lino-input", vec!["--indexed-lookups"]),
    ] {
        for populated in [false, true] {
            for invalid in ["(c: c", "(c: (nested: a b) (bad: a))"] {
                let dir = tempdir()?;
                let db = dir.path().join("data.links");
                let input = dir.path().join("input.lino");
                let before = dir.path().join("before.bin");
                let after = dir.path().join("after.bin");
                if populated {
                    std::fs::write(&input, "(a: a a)\n(b: b b)\n(existing: a b)\n")?;
                    ensure_success(&run_clink(&db, &input, &dir.path().join("seed.lino"))?)?;
                }
                ensure_success(
                    &Command::new(env!("CARGO_BIN_EXE_clink"))
                        .arg("--db")
                        .arg(&db)
                        .arg("--export-binary")
                        .arg(&before)
                        .output()?,
                )?;
                std::fs::write(&input, format!("(a: b a)\n(new: fresh fresh)\n{invalid}\n"))?;

                let result = Command::new(env!("CARGO_BIN_EXE_clink"))
                    .arg("--db")
                    .arg(&db)
                    .args(&flags)
                    .arg(option)
                    .arg(&input)
                    .output()?;

                assert_eq!(result.status.code(), Some(1));
                assert!(!result.stderr.is_empty());
                ensure_success(
                    &Command::new(env!("CARGO_BIN_EXE_clink"))
                        .arg("--db")
                        .arg(&db)
                        .arg("--export-binary")
                        .arg(&after)
                        .output()?,
                )?;
                assert_eq!(
                    std::fs::read(&before)?,
                    std::fs::read(&after)?,
                    "{option} {invalid}"
                );
            }
        }
    }
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
