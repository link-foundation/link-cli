//! Non-collision archive errors must keep committed cleanup retryable.
#![cfg(not(target_arch = "wasm32"))]
use anyhow::Result;
use link_cli::{
    CommitMode, FileTransitionLog, GenericTransactionsDecorator, LogRetentionPolicy,
    NamedTypeLinks, NamedTypesDecorator, Transition,
};

#[test]
fn archive_failure_preserves_decision_and_history_until_cleanup_retry() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    let archive = root.join("archive");
    std::fs::write(&archive, "existing entry")?;
    let row;
    {
        let mut data = NamedTypesDecorator::new(root.join("data"), false)?;
        row = data.try_create(11, 12)?;
        data.save()?;
        let log = FileTransitionLog::open(root.join("journal"))?;
        let mut tx = GenericTransactionsDecorator::<u32, _, _>::new(
            data,
            log,
            LogRetentionPolicy::Chunked {
                chunk_size: 1,
                archive_directory: archive.clone(),
            },
            CommitMode::Sync,
            false,
        )?;
        tx.begin_transaction()?;
        tx.update(row, 31, 32)?;
        assert!(tx.commit().is_err());
        assert!(tx.begin_transaction().is_err());
        assert_eq!(tx.log().len(), 1);
        assert_eq!(tx.get(row).unwrap().source, 31);
        assert_eq!(std::fs::read_to_string(&archive)?, "existing entry");
        assert!(tx.update(row, 41, 42).is_err());
        // The decision is already durable: rollback retries checkpoint/retention.
        assert!(tx.rollback().is_err());
        assert_eq!(tx.get(row).unwrap().source, 31);
        std::fs::remove_file(&archive)?;
        tx.rollback()?;
        assert!(tx.log().is_empty());
        let original = std::fs::read_dir(&archive)?.next().unwrap()?.path();
        let contents = std::fs::read_to_string(&original)?;
        let retained = Transition::parse(contents.trim_end())?;
        assert_eq!(retained.after.source, 31);
        tx.update(row, 51, 52)?;
        assert_eq!(std::fs::read_to_string(&original)?, contents);
        assert_eq!(std::fs::read_dir(&archive)?.count(), 2);
        tx.save()?;
    }
    for _ in 0..2 {
        let data = NamedTypesDecorator::new(root.join("data"), false)?;
        let log = FileTransitionLog::open(root.join("journal"))?;
        let tx = GenericTransactionsDecorator::<u32, _, _>::new(
            data,
            log,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )?;
        assert_eq!(tx.get(row).unwrap().source, 51);
        assert_eq!(tx.last_logged_sequence(), tx.applied_sequence());
        assert_eq!(std::fs::read_dir(&archive)?.count(), 2);
    }
    Ok(())
}
