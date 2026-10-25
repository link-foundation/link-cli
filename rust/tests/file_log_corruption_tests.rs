//! Tail repair never suppresses a corrupt complete record.
use anyhow::Result;
use link_cli::{FileTransitionLog, TransitionLogStore};
#[test]
fn complete_invalid_utf8_is_reported_and_preserved() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("journal");
    let bytes = b"committed\n\xff\n";
    std::fs::write(&path, bytes)?;
    let mut log = FileTransitionLog::open(&path)?;
    assert!(log.read_log_entries().is_err());
    drop(log);
    assert_eq!(std::fs::read(&path)?, bytes);
    Ok(())
}
