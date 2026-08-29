//! Embedding `link-cli` as a transactional store, the way an external
//! crate would (issue #98).
//!
//! Run it with:
//!
//! ```text
//! cargo run --manifest-path rust/Cargo.toml --example embedded_store
//! ```
//!
//! It walks through the four properties that make the library reusable:
//!
//! 1. the store is a real file-mapped `doublets::unit::Store`, addressed
//!    by whichever type the consumer already uses — `usize` here;
//! 2. writes go through the transactions layer, so an uncommitted
//!    transaction is rolled back;
//! 3. the database file is mutated **in place**, so a mapping another
//!    process is holding never goes stale, and committed writes survive
//!    a crash without an explicit `save`;
//! 4. an exclusive lock keeps a second writer out, and `StorageRevision`
//!    tells a reader whether anyone else has written.

use std::path::Path;

use link_cli::storage::LinksStorage;
use link_cli::transactions::{
    CommitMode, FileTransitionLog, GenericTransactionsDecorator, LogRetentionPolicy,
};
use link_cli::{DoubletsStorage, LinkError, StorageRevision};

/// `DoubletsStorage` over file-mapped memory, addressed by `usize`.
type Store = DoubletsStorage<usize, link_cli::FileMappedUnitStore<usize>>;
type Transactions = GenericTransactionsDecorator<usize, Store, FileTransitionLog>;

fn open(database: &Path, log: &Path) -> Result<Transactions, LinkError> {
    // `open_exclusive` takes the advisory write lock on `<database>.lock`
    // and holds it for the lifetime of the returned storage.
    let store = Store::open_exclusive(database)?;
    let log = FileTransitionLog::open(log)?;
    GenericTransactionsDecorator::new(
        store,
        log,
        LogRetentionPolicy::default(),
        CommitMode::default(),
        false,
    )
}

/// Identity of the file behind `path`. A store that is rebuilt through a
/// temporary file and `rename` changes it; an in-place one never does.
/// Only Unix exposes the inode, so elsewhere the check is skipped.
#[cfg(unix)]
fn file_identity(path: &Path) -> std::io::Result<Option<u64>> {
    use std::os::unix::fs::MetadataExt;
    Ok(Some(std::fs::metadata(path)?.ino()))
}

#[cfg(not(unix))]
fn file_identity(_path: &Path) -> std::io::Result<Option<u64>> {
    Ok(None)
}

fn main() -> Result<(), LinkError> {
    let directory = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/clink-embedded-store".to_string());
    let directory = Path::new(&directory);
    let _ = std::fs::remove_dir_all(directory);
    std::fs::create_dir_all(directory)?;

    let database = directory.join("db.links");
    let log = directory.join("db.transitions.log");

    // -- 1. Commit one write, then abandon another ---------------------
    let committed;
    let abandoned;
    let first_identity;
    {
        let mut transactions = open(&database, &log)?;

        transactions.begin_transaction()?;
        committed = transactions.create(0, 0)?;
        transactions.commit()?;
        println!("committed link {committed}");

        first_identity = file_identity(&database)?;

        // Dropping the handle without `commit` is what a crash looks
        // like to the next process that opens the store.
        transactions.begin_transaction()?;
        abandoned = transactions.create(0, 0)?;
        println!("wrote link {abandoned} inside a transaction that never commits");

        // While this storage lives, nobody else can take the write lock.
        assert!(Store::try_open_exclusive(&database)?.is_none());
        println!("a second writer is locked out while the first one is open");
    }

    // -- 2. Reopen: recovery runs in the constructor --------------------
    let revision;
    {
        let transactions = open(&database, &log)?;
        assert!(
            transactions.exists(committed),
            "committed write must survive"
        );
        assert!(
            !transactions.exists(abandoned),
            "uncommitted write must be rolled back"
        );
        println!("after recovery: {committed} is present, {abandoned} is gone");

        assert_eq!(
            file_identity(&database)?,
            first_identity,
            "the database must be mutated in place, never replaced"
        );
        match first_identity {
            Some(inode) => {
                println!("inode unchanged ({inode}): another process's mapping stays valid")
            }
            None => println!("file identity unchanged: another process's mapping stays valid"),
        }

        revision = StorageRevision::of(&database)?;
    }

    // -- 3. Notice a write by somebody else -----------------------------
    {
        let mut transactions = open(&database, &log)?;
        transactions.begin_transaction()?;
        let extra = transactions.create(committed, committed)?;
        transactions.commit()?;
        transactions.flush()?;
        println!("another holder committed link {extra}");
    }

    let after = StorageRevision::of(&database)?;
    println!("revision changed: {}", after != revision);

    // -- 4. Read the store back through the storage trait ---------------
    let store = Store::open_shared(&database)?;
    println!("links in the store: {}", store.links_count());

    Ok(())
}
