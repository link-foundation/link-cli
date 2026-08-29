//! Crash-recovery coverage for the transactions layer over a
//! **file-backed** doublets store (issue #98, ask 3).
//!
//! Every test here composes `GenericTransactionsDecorator` over a
//! memory-mapped `doublets::unit::Store` addressed by `usize` — the exact
//! shape an embedding application (e.g. `link-assistant/router`) uses —
//! and a durable append-only `FileTransitionLog`. "Crash" is simulated by
//! dropping the decorator without calling `save()`, which is the closest
//! in-process analogue of a process dying: nothing gets an orderly
//! shutdown, and only what was already written to the log and to the
//! mapping survives.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::Result;
use link_cli::transactions::{
    CommitMode, FileTransitionLog, GenericTransactionsDecorator, LogRetentionPolicy,
};
use link_cli::{DoubletsStorage, FileMappedUnitStore, LinkError, LinksStorage};
use tempfile::TempDir;

type FileBackedTransactions = GenericTransactionsDecorator<
    usize,
    DoubletsStorage<usize, FileMappedUnitStore<usize>>,
    FileTransitionLog,
>;

fn open(db: &Path, log: &Path) -> Result<FileBackedTransactions, LinkError> {
    let storage = DoubletsStorage::<usize, _>::open_exclusive(db)?;
    let log = FileTransitionLog::open(log)?;
    GenericTransactionsDecorator::new(
        storage,
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
}

struct Paths {
    _dir: TempDir,
    db: PathBuf,
    log: PathBuf,
}

fn paths() -> Result<Paths> {
    let dir = TempDir::new()?;
    let db = dir.path().join("store.doublets");
    let log = dir.path().join("store.transitions.log");
    Ok(Paths { _dir: dir, db, log })
}

#[test]
fn committed_writes_survive_a_crash_without_save() -> Result<()> {
    let p = paths()?;

    let (a, b) = {
        let mut tx = open(&p.db, &p.log)?;
        tx.begin_transaction()?;
        let a = tx.create(0, 0)?;
        let b = tx.create(a, a)?;
        tx.commit()?;
        // No save(): the process "dies" right after commit returned.
        (a, b)
    };

    let tx = open(&p.db, &p.log)?;
    assert!(tx.exists(a), "committed link {a} must survive the crash");
    assert!(tx.exists(b), "committed link {b} must survive the crash");
    assert_eq!(
        tx.inner()
            .get_link(b)
            .map(|link| (link.source, link.target)),
        Some((a, a))
    );
    Ok(())
}

#[test]
fn uncommitted_writes_are_rolled_back_after_a_crash() -> Result<()> {
    let p = paths()?;

    let committed = {
        let mut tx = open(&p.db, &p.log)?;
        tx.begin_transaction()?;
        let committed = tx.create(0, 0)?;
        tx.commit()?;

        // A second transaction that never reaches commit.
        tx.begin_transaction()?;
        let doomed = tx.create(committed, committed)?;
        assert!(tx.exists(doomed));
        tx.save()?;
        // Crash: the decorator is dropped with the transaction open.
        committed
    };

    let mut tx = open(&p.db, &p.log)?;
    assert!(tx.exists(committed), "committed link must be kept");
    assert_eq!(
        tx.inner().links_count(),
        1,
        "the uncommitted link must be rolled back by recovery (R10)"
    );

    // The rollback is itself recorded, so a second recovery is a no-op.
    tx.reload()?;
    assert_eq!(tx.inner().links_count(), 1);
    Ok(())
}

#[test]
fn committed_but_unapplied_transitions_are_reapplied() -> Result<()> {
    let p = paths()?;
    {
        let mut tx = open(&p.db, &p.log)?;
        tx.begin_transaction()?;
        tx.create(0, 0)?;
        tx.commit()?;
        tx.save()?;
    }

    // Simulate a crash between "commit marker durably written" and "data
    // store side-effect durably written": hand-write a committed
    // transition for a link the store does not contain, with no applied
    // marker for it.
    let mut log = OpenOptions::new().append(true).open(&p.log)?;
    writeln!(
        log,
        "__transactions:transition:v1|{:032x}|2|0|0|0,0,0|2,1,1",
        7u128
    )?;
    writeln!(log, "__transactions:commit:{:032x}", 7u128)?;
    log.flush()?;
    drop(log);

    let tx = open(&p.db, &p.log)?;
    assert!(
        tx.exists(2),
        "a committed-but-unapplied transition must be re-applied by recovery"
    );
    assert_eq!(
        tx.inner()
            .get_link(2)
            .map(|link| (link.source, link.target)),
        Some((1, 1))
    );
    Ok(())
}

#[test]
fn a_torn_final_log_entry_is_ignored_during_recovery() -> Result<()> {
    let p = paths()?;
    let survivor = {
        let mut tx = open(&p.db, &p.log)?;
        tx.begin_transaction()?;
        let survivor = tx.create(0, 0)?;
        tx.commit()?;
        tx.save()?;
        survivor
    };
    let complete_len = std::fs::metadata(&p.log)?.len();

    // A crash in the middle of appending the next entry leaves a line
    // with no terminating newline.
    let mut log = OpenOptions::new().append(true).open(&p.log)?;
    write!(log, "__transactions:transition:v1|000000000000000000000")?;
    log.flush()?;
    drop(log);
    assert!(std::fs::metadata(&p.log)?.len() > complete_len);

    let mut tx = open(&p.db, &p.log)?;
    assert!(tx.exists(survivor), "the complete entries must still load");
    assert_eq!(tx.log().len(), 1, "the torn entry must not be replayed");

    // Recovery keeps writing to the log; the torn tail must not corrupt
    // subsequent entries.
    tx.begin_transaction()?;
    let next = tx.create(0, 0)?;
    tx.commit()?;
    tx.save()?;
    drop(tx);

    let tx = open(&p.db, &p.log)?;
    assert!(tx.exists(survivor));
    assert!(tx.exists(next));
    assert_eq!(tx.log().len(), 2);
    Ok(())
}

#[test]
fn a_log_written_by_a_wider_address_type_is_rejected_not_dropped() -> Result<()> {
    let p = paths()?;
    {
        let mut tx = open(&p.db, &p.log)?;
        tx.begin_transaction()?;
        tx.create(0, 0)?;
        tx.commit()?;
        tx.save()?;
    }

    let mut log = OpenOptions::new().append(true).open(&p.log)?;
    let too_wide = u128::from(u64::MAX) + 1;
    writeln!(
        log,
        "__transactions:transition:v1|{:032x}|2|0|0|0,0,0|{too_wide},0,0",
        9u128
    )?;
    log.flush()?;
    drop(log);

    let storage = DoubletsStorage::<u32, _>::open_exclusive(&p.db)?;
    let narrow = FileTransitionLog::open(&p.log)?;
    let opened = GenericTransactionsDecorator::new(
        storage,
        narrow,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    );
    assert!(
        matches!(opened, Err(LinkError::AddressOutOfRange(_))),
        "recovery must refuse a log it cannot represent, got {opened:?}",
        opened = opened.map(|_| "Ok(..)")
    );
    Ok(())
}

#[test]
fn the_data_store_is_mutated_in_place_across_transactions() -> Result<()> {
    let p = paths()?;
    let mut tx = open(&p.db, &p.log)?;
    tx.begin_transaction()?;
    tx.create(0, 0)?;
    tx.commit()?;
    tx.save()?;

    #[cfg(unix)]
    let inode = {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(&p.db)?.ino()
    };

    for _ in 0..8 {
        tx.begin_transaction()?;
        tx.create(1, 1)?;
        tx.commit()?;
        tx.save()?;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        assert_eq!(
            std::fs::metadata(&p.db)?.ino(),
            inode,
            "the database file must be mutated in place, never replaced"
        );
    }
    assert_eq!(tx.inner().links_count(), 9);
    Ok(())
}
