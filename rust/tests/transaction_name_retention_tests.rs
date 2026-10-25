//! Real retained journals and archives keep complete logical name records.
#![cfg(not(target_arch = "wasm32"))]
use anyhow::Result;
use link_cli::{
    transactions::JournalRecord, CommitMode, FileTransitionLog, GenericTransactionsDecorator,
    LogRetentionPolicy, NamedTypes, NamedTypesDecorator,
};

fn retained(chunked: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    let archive = root.join("archive");
    let policy = if chunked {
        LogRetentionPolicy::Chunked {
            chunk_size: 1,
            archive_directory: archive.clone(),
        }
    } else {
        LogRetentionPolicy::Sized { max_transitions: 1 }
    };
    let open = |policy| -> Result<_> {
        Ok(GenericTransactionsDecorator::<u32, _, _>::new(
            NamedTypesDecorator::new(root.join("data"), false)?,
            FileTransitionLog::open(root.join("journal"))?,
            policy,
            CommitMode::Sync,
            false,
        )?)
    };
    let id;
    {
        let mut tx = open(policy)?;
        id = tx.create(11, 12)?;
        tx.set_name(id, "original")?;
        tx.set_name(id, "changed 雪|\0\n")?;
        tx.begin_transaction()?;
        tx.set_name(77, "changed 雪|\0\n")?;
        tx.rollback()?;
        assert_eq!(
            NamedTypes::get_name(tx.inner_mut(), id)?.as_deref(),
            Some("changed 雪|\0\n")
        );
        tx.delete(id)?;
        tx.set_name(77, "following")?;
        tx.save()?;
    }
    for _ in 0..2 {
        let mut tx = open(LogRetentionPolicy::Infinite)?;
        assert!(!tx.exists(id));
        assert_eq!(NamedTypes::get_name(tx.inner_mut(), id)?, None);
        assert_eq!(
            NamedTypes::get_name(tx.inner_mut(), 77)?.as_deref(),
            Some("following")
        );
        tx.set_name(88, "next")?;
        tx.save()?;
    }
    if chunked {
        let mut records = Vec::new();
        for file in std::fs::read_dir(&archive)? {
            for line in std::fs::read_to_string(file?.path())?.lines() {
                records.push(JournalRecord::<u32>::parse(line)?);
            }
        }
        records.sort_by_key(|record| record.sequence);
        assert!(records.iter().any(|record| record.name_only));
        assert!(records
            .iter()
            .any(|record| record.kind == link_cli::TransitionKind::Delete
                && !record.names.is_empty()));
        let mut restored = GenericTransactionsDecorator::<u32, _, _>::new(
            NamedTypesDecorator::new(root.join("restored"), false)?,
            FileTransitionLog::open(root.join("restored-journal"))?,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )?;
        for record in records {
            restored.apply_record(&record)?;
        }
        assert!(!restored.exists(id));
        assert_eq!(
            NamedTypes::get_name(restored.inner_mut(), 77)?.as_deref(),
            Some("following")
        );
    }
    Ok(())
}
#[test]
fn sized_names_survive_retention_abort_and_reopening() -> Result<()> {
    retained(false)
}
#[test]
fn chunk_archives_replay_complete_committed_names() -> Result<()> {
    retained(true)
}
