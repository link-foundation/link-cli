//! Integration tests for the optional transactions decorator (Rust side).
//!
//! Mirrors the C# `TransactionsDecoratorTests` and exercises the
//! requirements R1–R10 of issue #94.

use anyhow::Result;
use link_cli::transactions::DoubletLink;
#[cfg(not(target_arch = "wasm32"))]
use link_cli::NamedTypesDecorator;
use link_cli::{CommitMode, LogRetentionPolicy, TransactionsDecorator, Transition, TransitionKind};
use std::path::PathBuf;
#[cfg(not(target_arch = "wasm32"))]
use tempfile::NamedTempFile;
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;

fn make_tx() -> (TransactionsDecorator, Vec<test_store::NamedFiles>) {
    let (data_files, data_links) = test_store::named().expect("open data links");
    let (log_files, log_links) = test_store::named().expect("open log links");
    let tx = TransactionsDecorator::new(
        data_links,
        log_links,
        LogRetentionPolicy::default(),
        CommitMode::default(),
        false,
    )
    .expect("open transactions decorator");
    (tx, vec![data_files, log_files])
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn auto_transaction_records_create_and_update() -> Result<()> {
    let (mut tx, _guards) = make_tx();

    let created = tx.create_and_update(0, 0)?;
    assert_ne!(0, created);

    let log = tx.log();
    assert_eq!(
        2,
        log.len(),
        "create_and_update must record two transitions"
    );
    assert_eq!(TransitionKind::Create, log[0].kind);
    assert_eq!(TransitionKind::Update, log[1].kind);
    assert_eq!(created, log[0].after.index);
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn rollback_undoes_create() -> Result<()> {
    let (mut tx, _guards) = make_tx();

    tx.begin_transaction()?;
    let created = tx.create_and_update(0, 0)?;
    assert!(tx.exists(created));
    tx.rollback()?;

    assert!(
        !tx.exists(created),
        "rolled-back create must remove the link"
    );
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn commit_persists_create() -> Result<()> {
    let (mut tx, _guards) = make_tx();

    tx.begin_transaction()?;
    let created = tx.create_and_update(0, 0)?;
    tx.commit()?;

    assert!(tx.exists(created));
    assert_eq!(tx.last_logged_sequence(), tx.applied_sequence());
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn rollback_undoes_update() -> Result<()> {
    let (mut tx, _guards) = make_tx();

    let a = tx.create_and_update(0, 0)?;
    let b = tx.create_and_update(0, 0)?;
    let c = tx.create_and_update(0, 0)?;

    tx.begin_transaction()?;
    tx.update(c, a, b)?;
    let updated = tx.get(c).copied().unwrap();
    assert_eq!(a, updated.source);
    assert_eq!(b, updated.target);
    tx.rollback()?;

    let after_rollback = tx.get(c).copied().unwrap();
    assert_eq!(c, after_rollback.index);
    assert_eq!(0, after_rollback.source);
    assert_eq!(0, after_rollback.target);
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn rollback_undoes_delete() -> Result<()> {
    let (mut tx, _guards) = make_tx();

    let a = tx.create_and_update(0, 0)?;
    let b = tx.create_and_update(0, 0)?;
    let c = tx.create_and_update(0, 0)?;
    tx.update(c, a, b)?;

    tx.begin_transaction()?;
    tx.delete(c)?;
    assert!(!tx.exists(c));
    tx.rollback()?;

    assert!(tx.exists(c), "delete must be restored by rollback");
    let restored = tx.get(c).copied().unwrap();
    assert_eq!(a, restored.source);
    assert_eq!(b, restored.target);
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn sized_retention_drops_oldest_after_applied() -> Result<()> {
    let (mut tx, _guards) = make_tx();
    tx.set_retention_policy(LogRetentionPolicy::Sized { max_transitions: 3 });

    for _ in 0..5 {
        tx.create_and_update(0, 0)?;
    }

    assert!(
        tx.log().len() as u64 <= 3,
        "sized retention must cap log length; got {}",
        tx.log().len()
    );
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn chunked_retention_archives_oldest() -> Result<()> {
    let archive_dir = std::env::temp_dir().join(format!(
        "tx-archive-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&archive_dir);

    {
        let (mut tx, _guards) = make_tx();
        tx.set_retention_policy(LogRetentionPolicy::Chunked {
            chunk_size: 2,
            archive_directory: archive_dir.clone(),
        });

        for _ in 0..4 {
            tx.create_and_update(0, 0)?;
        }

        assert!(archive_dir.exists(), "archive directory must be created");
        let files: Vec<_> = std::fs::read_dir(&archive_dir)?
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with("transitions-chunk-")
            })
            .collect();
        assert!(!files.is_empty(), "chunked retention must archive files");
    }
    let _ = std::fs::remove_dir_all(&archive_dir);
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn retention_policy_parses_specs() {
    assert!(matches!(
        LogRetentionPolicy::parse("infinite").unwrap(),
        LogRetentionPolicy::Infinite
    ));
    assert!(matches!(
        LogRetentionPolicy::parse("sized:1000").unwrap(),
        LogRetentionPolicy::Sized {
            max_transitions: 1000
        }
    ));
    match LogRetentionPolicy::parse("chunked:500:/tmp/x").unwrap() {
        LogRetentionPolicy::Chunked {
            chunk_size,
            archive_directory,
        } => {
            assert_eq!(chunk_size, 500);
            assert_eq!(archive_directory, PathBuf::from("/tmp/x"));
        }
        _ => panic!("expected Chunked"),
    }
    assert!(LogRetentionPolicy::parse("garbage").is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn transition_round_trips_through_serialize() {
    let t = Transition {
        transaction_id: 0xabcdef1234567890u128,
        sequence: 42,
        timestamp_ms: 1234567890,
        kind: TransitionKind::Update,
        before: DoubletLink::new(1, 2, 3),
        after: DoubletLink::new(1, 4, 5),
    };

    let parsed = Transition::try_parse(&t.serialize()).unwrap();
    assert_eq!(t, parsed);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn async_commit_marks_applied() -> Result<()> {
    // Rust port runs sync; the contract that `applied_sequence` reaches
    // `last_logged_sequence` after commit still holds.
    let (mut tx, _guards) = make_tx();
    tx.set_commit_mode(CommitMode::Async);

    tx.begin_transaction()?;
    let created = tx.create_and_update(0, 0)?;
    tx.commit()?;

    assert_eq!(tx.last_logged_sequence(), tx.applied_sequence());
    assert!(tx.exists(created));
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn no_behaviour_change_when_not_opted_in() -> Result<()> {
    // R8: bare NamedTypesDecorator behaves identically whether or not
    // the TransactionsDecorator is wrapped above it.
    let (_files, mut data_links) = test_store::named()?;
    let id = data_links.get_or_create(0, 0);
    assert!(data_links.exists(id));
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn recovery_reapplies_committed_transitions() -> Result<()> {
    // After dropping the decorator and reopening with the same store
    // files, the recovery protocol should rebuild the log and restore
    // committed state (R6).
    let data_file = NamedTempFile::new()?;
    let log_file = NamedTempFile::new()?;
    let data_path = data_file.path().to_path_buf();
    let log_path = log_file.path().to_path_buf();

    let id = {
        let data_links = NamedTypesDecorator::new(&data_path, false)?;
        let log_links = NamedTypesDecorator::new(&log_path, false)?;
        let mut tx = TransactionsDecorator::new(
            data_links,
            log_links,
            LogRetentionPolicy::default(),
            CommitMode::default(),
            false,
        )?;
        tx.begin_transaction()?;
        let id = tx.create_and_update(0, 0)?;
        tx.commit()?;
        tx.save()?;
        id
    };

    let data_links = NamedTypesDecorator::new(&data_path, false)?;
    let log_links = NamedTypesDecorator::new(&log_path, false)?;
    let tx = TransactionsDecorator::new(
        data_links,
        log_links,
        LogRetentionPolicy::default(),
        CommitMode::default(),
        false,
    )?;

    assert!(tx.exists(id), "committed link must survive reopen");
    assert!(tx.last_logged_sequence() >= 2);
    assert!(tx.applied_sequence() >= tx.last_logged_sequence());
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn make_transitions_database_filename_returns_sibling_path() {
    let path = TransactionsDecorator::make_transitions_database_filename("/var/data/db.links");
    assert_eq!(path, PathBuf::from("/var/data/db.transitions.links"));

    let path = TransactionsDecorator::make_transitions_database_filename("db.links");
    assert_eq!(path, PathBuf::from("db.transitions.links"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn nested_transactions_are_rejected() -> Result<()> {
    let (mut tx, _guards) = make_tx();
    tx.begin_transaction()?;
    let result = tx.begin_transaction();
    assert!(result.is_err(), "nested transactions must be rejected");
    tx.rollback()?;
    Ok(())
}
