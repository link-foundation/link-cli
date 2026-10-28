//! Malformed finite graphs must fail without exhausting the test host.
#![cfg(target_os = "linux")]
use anyhow::{bail, Result};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[path = "common/unicode_graph.rs"]
mod unicode_graph;

fn bounded(operation: &str) -> Result<()> {
    let mut command = {
        let mut command = Command::new("bash");
        command.args([
            "-c",
            "ulimit -c 0\nulimit -v 131072\nulimit -t 2\nexec \"$@\"",
            "unicode-cycle-child",
        ]);
        command.arg(std::env::current_exe()?);
        command
    };
    let mut child = command
        .args(["--exact", "unicode_cycle_child", "--ignored", "--nocapture"])
        .env("LINK_UNICODE_OPERATION", operation)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            bail!("Finite Unicode graph child failed to stop: {operation}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "Finite Unicode graph {operation} ({}): {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Unicode cycles rejected"));
    Ok(())
}

#[test]
fn cyclic_strings_fail_without_exhausting_resources() -> Result<()> {
    bounded("string")
}
#[test]
fn cyclic_code_units_fail_without_exhausting_resources() -> Result<()> {
    bounded("code-units")
}
#[test]
fn persisted_cycles_fail_after_repeated_reopening() -> Result<()> {
    bounded("persistent")
}

fn persistent() -> Result<()> {
    use link_cli::{external_reference, LinkStorage, UnicodeStringStorage};
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("data.links");
    let mut links = LinkStorage::new(&path, false)?;
    let (value, row) = {
        let mut storage = UnicodeStringStorage::new(&mut links)?;
        let root = storage.links_mut().create(0, 0);
        storage.links_mut().update(root, root, root)?;
        let sequence_type = storage.unicode_sequence_type();
        let string_type = storage.string_type();
        let name_type = storage.name_type();
        let sequence = storage.links_mut().get_or_create(root, sequence_type);
        let value = storage.links_mut().get_or_create(string_type, sequence);
        let row = storage.links_mut().create(0, 0);
        let name = storage.links_mut().get_or_create(name_type, value);
        storage.links_mut().get_or_create(row, name);
        storage
            .links_mut()
            .get_or_create(external_reference(900), name);
        (value, row)
    };
    links.save()?;
    drop(links);
    let preserved = std::fs::read(&path)?;
    for _ in 0..2 {
        let mut links = LinkStorage::new(&path, false)?;
        let mut storage = UnicodeStringStorage::new(&mut links)?;
        assert!(storage.get_string(value).is_err());
        assert!(storage.unicode_sequence_code_units(value).is_err());
        assert!(storage.get_name(row).is_err());
        assert!(storage.external_names().is_err());
        assert_eq!(std::fs::read(&path)?, preserved);
        let following = storage.create_string("following 雪")?;
        assert_eq!(storage.get_string(following)?, "following 雪");
    }
    Ok(())
}

#[test]
#[ignore = "Executed only by bounded Unicode graph parents"]
fn unicode_cycle_child() -> Result<()> {
    let operation = std::env::var("LINK_UNICODE_OPERATION")?;
    if operation == "persistent" {
        persistent()?;
    } else {
        unicode_graph::cyclic_decoding(&operation)?;
    }
    println!("Unicode cycles rejected");
    Ok(())
}
