//! A finite valid ancestry must not consume one stack frame per parent.
#![cfg(not(target_arch = "wasm32"))]
use anyhow::{bail, Result};
use std::{
    process::{Command, Stdio},
    time::{Duration, Instant},
};
#[path = "common/flat_version_metadata.rs"]
mod flat_version_metadata;

fn contract(operation: &str) -> Result<()> {
    let depth = 2048;
    let mut version = flat_version_metadata::FlatMetadata::deep_version(depth)?;
    println!(
        "Constructed {} valid branches; {operation}",
        version.list_branches().len()
    );
    match operation {
        "checkout" => version.checkout(0)?,
        "switch" => {
            version.switch_branch("main")?;
            version.switch_branch(&format!("branch-{depth}"))?;
        }
        "fork" => {
            version
                .branch("following", Some(1))
                .expect_err("an empty chain has no sequence 1");
            version.branch("following", Some(0))?;
        }
        _ => bail!("Unknown finite ancestry operation."),
    }
    assert_eq!(version.current_branch(), format!("branch-{depth}"));
    assert_eq!(version.current_sequence(), 0);
    assert_eq!(
        version.list_branches().len(),
        depth + 1 + usize::from(operation == "fork")
    );
    let id = version.create_and_update(11, 12)?;
    let sequence = version.current_sequence();
    assert!(sequence > 0);
    version.checkout(0)?;
    version.checkout(sequence)?;
    assert_eq!(version.transactions().get(id).unwrap().source, 11);
    println!("Finite ancestry contract passed: {operation}");
    Ok(())
}

fn bounded(operation: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    let mut command = {
        let mut command = Command::new("bash");
        command.args([
            "-c",
            "ulimit -c 0\nulimit -v 524288\nexec \"$@\"",
            "ancestry-child",
        ]);
        command.arg(std::env::current_exe()?);
        command
    };
    #[cfg(not(target_os = "linux"))]
    let mut command = Command::new(std::env::current_exe()?);
    let mut child = command
        .args(["--exact", "ancestry_child", "--ignored", "--nocapture"])
        .env("LINK_ANCESTRY_OPERATION", operation)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(15);
    while child.try_wait()?.is_none() {
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            bail!("Finite ancestry child did not finish: {operation}");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let output = child.wait_with_output()?;
    assert!(
        output.status.success(),
        "Finite child {operation}: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Finite ancestry contract passed"));
    Ok(())
}
#[test]
fn deep_checkout_uses_bounded_stack() -> Result<()> {
    bounded("checkout")
}
#[test]
fn deep_switch_uses_bounded_stack() -> Result<()> {
    bounded("switch")
}
#[test]
fn deep_fork_validation_uses_bounded_stack() -> Result<()> {
    bounded("fork")
}
#[test]
#[ignore = "Executed only by finite ancestry parents"]
fn ancestry_child() -> Result<()> {
    let operation = std::env::var("LINK_ANCESTRY_OPERATION")?;
    std::thread::Builder::new()
        .stack_size(256 * 1024)
        .spawn(move || contract(&operation).map_err(|error| error.to_string()))?
        .join()
        .expect("finite ancestry thread panicked")
        .map_err(anyhow::Error::msg)
}
