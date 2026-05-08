use anyhow::{ensure, Result};
use link_cli::NamedTypesDecorator;
use std::process::Command;
use tempfile::tempdir;

#[test]
fn cli_stores_string_aliases_in_separate_names_database() -> Result<()> {
    let temp_dir = tempdir()?;
    let db_path = temp_dir.path().join("string-ids.links");
    let names_path = NamedTypesDecorator::make_names_database_filename(&db_path);

    let output = Command::new(env!("CARGO_BIN_EXE_clink"))
        .args([
            "--db",
            db_path.to_str().unwrap(),
            "--auto-create-missing-references",
            "() ((child: father mother))",
            "--after",
        ])
        .output()?;

    ensure!(
        output.status.success(),
        "clink failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout)?;
    assert!(stdout.contains("(father: father father)"));
    assert!(stdout.contains("(mother: mother mother)"));
    assert!(stdout.contains("(child: father mother)"));

    let main_database = std::fs::read_to_string(&db_path)?;
    assert!(!main_database.contains("father"));
    assert!(!main_database.contains("mother"));
    assert!(!main_database.contains("child"));

    let names_database = std::fs::read_to_string(&names_path)?;
    assert!(!names_database.trim().is_empty());

    Ok(())
}
