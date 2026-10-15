//! Issue #110: opt-in indexed imports and fresh-process readback.
use anyhow::{ensure, Result};
use std::process::Command;
use tempfile::tempdir;

#[test]
fn indexed_import_preserves_names_across_processes() -> Result<()> {
    let dir = tempdir()?;
    let input = dir.path().join("input.lino");
    let db = dir.path().join("data.links");
    let output = dir.path().join("output.lino");
    let text = "('雪😀': '雪😀' '雪😀')\n(Type: Type Type)\n('punctuation:()': 'punctuation:()' 'punctuation:()')\n";
    std::fs::write(&input, text)?;
    let imported = Command::new(env!("CARGO_BIN_EXE_clink"))
        .arg("--db")
        .arg(&db)
        .arg("--indexed-lookups")
        .arg("--import")
        .arg(&input)
        .output()?;
    ensure!(
        imported.status.success(),
        "{}",
        String::from_utf8_lossy(&imported.stderr)
    );
    let exported = Command::new(env!("CARGO_BIN_EXE_clink"))
        .arg("--db")
        .arg(&db)
        .arg("--export")
        .arg(&output)
        .output()?;
    ensure!(
        exported.status.success(),
        "{}",
        String::from_utf8_lossy(&exported.stderr)
    );
    let lines = std::fs::read_to_string(&output)?;
    assert_eq!(lines, text.replace("'雪😀'", "雪😀"));
    Ok(())
}

/// Finite child-process deadline also catches a regression to full-map scans.
/// Debug CI builds get 60s; the release benchmark documents the 25s target.
fn run_bounded(command: &mut Command, dir: &std::path::Path) -> Result<()> {
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    let log = dir.join("stderr.log");
    let mut child = command
        .stdout(Stdio::null())
        .stderr(std::fs::File::create(&log)?)
        .spawn()?;
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if start.elapsed() > Duration::from_secs(60) {
            child.kill()?;
            child.wait()?;
            anyhow::bail!(
                "indexed import exceeded 60s: {}",
                std::fs::read_to_string(log)?
            );
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    ensure!(status.success(), "{}", std::fs::read_to_string(log)?);
    Ok(())
}

#[test]
fn indexed_numeric_and_named_scaling_fixtures_round_trip() -> Result<()> {
    for named in [false, true] {
        for count in [100, 1_000, 10_000] {
            let dir = tempdir()?;
            let db = dir.path().join("data.links");
            let input = dir.path().join("input.lino");
            let output = dir.path().join("output.lino");
            let archive = dir.path().join("archive.bin");
            let readback = dir.path().join("readback.lino");
            let text = (1..=count)
                .map(|i| {
                    let id = if named {
                        format!("atom-{i:064}")
                    } else {
                        i.to_string()
                    };
                    format!("({id}: {id} {id})\n")
                })
                .collect::<String>();
            std::fs::write(&input, &text)?;
            let mut import = Command::new(env!("CARGO_BIN_EXE_clink"));
            import
                .arg("--db")
                .arg(&db)
                .arg("--indexed-lookups")
                .arg("--import")
                .arg(&input)
                .arg("--export")
                .arg(&output)
                .arg("--export-binary")
                .arg(&archive);
            run_bounded(&mut import, dir.path())?;
            assert_eq!(std::fs::read_to_string(&output)?, text);
            // A second process imports the same names into an existing store:
            // no duplicates and no cache state inherited from the writer.
            let mut repeat = Command::new(env!("CARGO_BIN_EXE_clink"));
            repeat
                .arg("--db")
                .arg(&db)
                .arg("--indexed-lookups")
                .arg("--import")
                .arg(&input)
                .arg("--export")
                .arg(&readback);
            run_bounded(&mut repeat, dir.path())?;
            assert_eq!(std::fs::read_to_string(&readback)?, text);
            let mut restore = Command::new(env!("CARGO_BIN_EXE_clink"));
            restore
                .arg("--db")
                .arg(dir.path().join("restored.links"))
                .arg("--indexed-lookups")
                .arg("--import-binary")
                .arg(&archive)
                .arg("--export")
                .arg(&readback);
            run_bounded(&mut restore, dir.path())?;
            assert_eq!(std::fs::read_to_string(readback)?, text);
        }
    }
    Ok(())
}
