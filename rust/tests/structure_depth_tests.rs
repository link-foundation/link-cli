//! Keep finite deep-output regressions isolated from the test host's stack.
#![cfg(target_os = "linux")]
use anyhow::{bail, Result};
use link_cli::{LinkStorage, NamedTypeLinks};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn bounded(operation: &str) -> Result<()> {
    let mut child = Command::new("bash")
        .args([
            "-c",
            "ulimit -c 0\nulimit -v 131072\nulimit -t 2\nexec \"$@\"",
            "finite-structure-child",
        ])
        .arg(std::env::current_exe()?)
        .args([
            "--exact",
            "structure_depth_child",
            "--ignored",
            "--nocapture",
        ])
        .env("LINK_STRUCTURE_OPERATION", operation)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            bail!("Finite structure child failed to stop: {operation}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "Finite structure {operation} ({}): {}\n{}",
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Finite structure contract passed"));
    Ok(())
}

#[test]
fn concrete_structure_output_uses_bounded_stack() -> Result<()> {
    bounded("concrete")
}

#[test]
fn trait_structure_output_uses_bounded_stack() -> Result<()> {
    bounded("trait")
}

#[test]
#[ignore = "Executed only by bounded structure parents"]
fn structure_depth_child() -> Result<()> {
    let operation = std::env::var("LINK_STRUCTURE_OPERATION")?;
    std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(move || -> Result<()> {
            let mut links = LinkStorage::in_memory(false);
            let mut ids = Vec::new();
            let mut root = 0;
            for _ in 0..2048 {
                root = links.create(root, 0);
                ids.push(root);
            }
            eprintln!("Prepared finite 2048-node structure: {operation}");
            let output = match operation.as_str() {
                "concrete" => links.format_structure(root)?,
                "trait" => NamedTypeLinks::format_structure(&mut links, root)?,
                _ => unreachable!(),
            };
            let mut expected = String::new();
            for id in ids.iter().rev() {
                use std::fmt::Write;
                write!(expected, "({id}: ")?;
            }
            expected.push('0');
            for _ in &ids {
                expected.push_str(" 0)");
            }
            assert_eq!(output, expected);
            println!("Finite structure contract passed: {operation}");
            Ok(())
        })?
        .join()
        .expect("Finite structure worker failed")
}
