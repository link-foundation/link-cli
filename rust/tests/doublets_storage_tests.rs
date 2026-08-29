//! Coverage for the doublets-backed storage introduced for issue #98.
//!
//! These tests exercise a **file-backed** `doublets::unit::Store` (not an
//! in-memory map), the `usize` address type used by embedding
//! applications, in-place mutation (stable inode), durability and the
//! advisory locking protocol.

use std::path::Path;

use anyhow::Result;
use doublets::unit::{LinkPart, Store as UnitStore};
use link_cli::{DoubletsStorage, FileLock, LinksStorage, LockMode, PersistentFileMapped};
use tempfile::TempDir;

#[cfg(unix)]
fn inode_of(path: &Path) -> Result<u64> {
    use std::os::unix::fs::MetadataExt;
    Ok(std::fs::metadata(path)?.ino())
}

#[test]
fn doublets_storage_creates_updates_and_deletes_links() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("links.doublets");
    let mut storage = DoubletsStorage::<u32, _>::open(&path)?;

    let a = storage.create_link(0, 0)?;
    let b = storage.create_link(a, a)?;

    assert_eq!(storage.links_count(), 2);
    assert_eq!(
        storage.get_link(b).map(|link| (link.source, link.target)),
        Some((a, a))
    );
    assert!(storage.link_exists(a));

    let before = storage.update_link(b, a, b)?;
    assert_eq!((before.source, before.target), (a, a));
    assert_eq!(storage.get_link(b).map(|link| link.target), Some(b));

    assert_eq!(storage.search_link(a, b), Some(b));
    assert_eq!(storage.search_link(b, b), None);
    assert_eq!(storage.get_or_create_link(a, b)?, b);

    let deleted = storage.delete_link(b)?;
    assert_eq!(deleted.index, b);
    assert!(!storage.link_exists(b));
    assert_eq!(storage.links_count(), 1);

    Ok(())
}

#[test]
fn doublets_storage_queries_by_pattern() -> Result<()> {
    let dir = TempDir::new()?;
    let mut storage = DoubletsStorage::<u32, _>::open(dir.path().join("query.doublets"))?;

    let a = storage.create_link(0, 0)?;
    let b = storage.create_link(0, 0)?;
    let ab = storage.create_link(a, b)?;
    let aa = storage.create_link(a, a)?;

    let from_a = storage.query_links(None, Some(a), None);
    let mut indices: Vec<_> = from_a.iter().map(|link| link.index).collect();
    indices.sort_unstable();
    assert_eq!(indices, vec![ab, aa]);

    assert_eq!(storage.query_links(Some(ab), None, None).len(), 1);
    assert_eq!(storage.all_links().len(), 4);

    Ok(())
}

/// Issue #98 asks for the storage to be generic over the doublets
/// address type, because `link-assistant/router` uses `unit::Store<usize, _>`.
#[test]
fn doublets_storage_supports_usize_addresses() -> Result<()> {
    let dir = TempDir::new()?;
    let mut storage = DoubletsStorage::<usize, _>::open(dir.path().join("usize.doublets"))?;

    let a: usize = storage.create_link(0, 0)?;
    let b: usize = storage.create_link(a, a)?;

    assert_eq!(storage.get_link(b).map(|link| link.source), Some(a));
    assert_eq!(storage.links_count(), 2);

    Ok(())
}

#[test]
fn doublets_storage_supports_u64_addresses() -> Result<()> {
    let dir = TempDir::new()?;
    let mut storage = DoubletsStorage::<u64, _>::open(dir.path().join("u64.doublets"))?;

    let a: u64 = storage.create_link(0, 0)?;
    assert_eq!(storage.get_link(a).map(|link| link.index), Some(a));

    Ok(())
}

/// An externally owned store can be adopted without giving up ownership
/// of the database — the reusability requirement from issue #98.
#[test]
fn doublets_storage_wraps_an_externally_owned_store() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("external.doublets");

    let mem = PersistentFileMapped::<LinkPart<usize>>::from_path(&path)?;
    let store = UnitStore::<usize, _>::new(mem)?;

    let mut storage = DoubletsStorage::wrap_at(store, &path)?;
    let a = storage.create_link(0, 0)?;
    storage.flush()?;

    // Ownership comes back out intact.
    let store = storage.into_store();
    assert!(doublets::Doublets::get_link(&store, a).is_some());

    Ok(())
}

/// Issue #98 requires in-place mutation: a tmp-file + rename would swap
/// the inode and leave other processes mapping a stale file.
#[cfg(unix)]
#[test]
fn doublets_storage_mutates_the_database_file_in_place() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("in-place.doublets");

    let mut storage = DoubletsStorage::<u32, _>::open(&path)?;
    let initial_inode = inode_of(&path)?;

    for _ in 0..64 {
        storage.create_link(0, 0)?;
    }
    storage.flush()?;

    assert_eq!(
        inode_of(&path)?,
        initial_inode,
        "the database file must be mutated in place so existing mappings stay valid"
    );

    Ok(())
}

/// Dropping the storage syncs the mapping, so a clean shutdown is durable
/// without an explicit `flush`, and reopening sees every write.
#[test]
fn doublets_storage_survives_reopen() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("reopen.doublets");

    let created = {
        let mut storage = DoubletsStorage::<u32, _>::open(&path)?;
        let a = storage.create_link(0, 0)?;
        let b = storage.create_link(a, a)?;
        storage.flush()?;
        (a, b)
    };

    let storage = DoubletsStorage::<u32, _>::open(&path)?;
    assert_eq!(storage.links_count(), 2);
    assert_eq!(
        storage.get_link(created.1).map(|link| link.source),
        Some(created.0)
    );

    Ok(())
}

/// `ensure_link_created` reserves a specific address, which the
/// transactions layer needs when replaying a create transition.
#[test]
fn doublets_storage_reserves_specific_addresses() -> Result<()> {
    let dir = TempDir::new()?;
    let mut storage = DoubletsStorage::<u32, _>::open(dir.path().join("reserve.doublets"))?;

    assert_eq!(storage.ensure_link_created(3)?, 3);
    assert!(storage.link_exists(3));
    assert_eq!(storage.links_count(), 3);

    // Already present: idempotent.
    assert_eq!(storage.ensure_link_created(3)?, 3);
    assert_eq!(storage.links_count(), 3);

    Ok(())
}

#[test]
fn doublets_storage_detects_external_writes() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("shared.doublets");

    let mut writer = DoubletsStorage::<u32, _>::open(&path)?;
    let reader = DoubletsStorage::<u32, _>::open(&path)?;
    assert!(!reader.has_external_changes()?);

    writer.create_link(0, 0)?;
    writer.flush()?;

    assert!(
        reader.has_external_changes()?,
        "a flushed write must be visible to the cheap external-change check"
    );

    Ok(())
}

/// Advisory locks are held per open file description, so two `File`
/// handles inside one process contend exactly like two processes do.
#[test]
fn exclusive_lock_excludes_other_holders() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("locked.doublets");
    let lock_path = link_cli::lock_file_path(&path);

    let held = FileLock::acquire(&lock_path, LockMode::Exclusive)?;
    assert_eq!(held.mode(), LockMode::Exclusive);

    assert!(FileLock::try_acquire(&lock_path, LockMode::Exclusive)?.is_none());
    assert!(FileLock::try_acquire(&lock_path, LockMode::Shared)?.is_none());
    assert!(DoubletsStorage::<u32, _>::try_open_exclusive(&path)?.is_none());

    drop(held);

    assert!(FileLock::try_acquire(&lock_path, LockMode::Exclusive)?.is_some());
    assert!(DoubletsStorage::<u32, _>::try_open_exclusive(&path)?.is_some());

    Ok(())
}

#[test]
fn shared_locks_allow_concurrent_readers_but_block_writers() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("shared-lock.doublets");
    let lock_path = link_cli::lock_file_path(&path);

    let first = FileLock::acquire(&lock_path, LockMode::Shared)?;
    let second = FileLock::try_acquire(&lock_path, LockMode::Shared)?;
    assert!(second.is_some(), "shared locks must not exclude each other");

    assert!(FileLock::try_acquire(&lock_path, LockMode::Exclusive)?.is_none());

    drop(second);
    drop(first);

    assert!(FileLock::try_acquire(&lock_path, LockMode::Exclusive)?.is_some());

    Ok(())
}

#[test]
fn opened_storage_holds_its_lock_for_its_lifetime() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("held.doublets");
    let lock_path = link_cli::lock_file_path(&path);

    let storage = DoubletsStorage::<u32, _>::open_exclusive(&path)?;
    assert!(storage.held_lock().is_some());
    assert!(FileLock::try_acquire(&lock_path, LockMode::Shared)?.is_none());

    drop(storage);
    assert!(FileLock::try_acquire(&lock_path, LockMode::Exclusive)?.is_some());

    Ok(())
}

/// Environment variable used to hand the lock path to the child process.
const LOCK_PROBE_PATH: &str = "LINK_CLI_LOCK_PROBE_PATH";

/// Exit code the probe uses to report that it managed to take the lock.
const PROBE_ACQUIRED: i32 = 3;

/// Helper executed as a **separate process** by
/// [`exclusive_lock_is_honoured_across_processes`]. It is ignored during
/// a normal run and does nothing unless the parent handed it a path.
#[test]
#[ignore = "helper process spawned by exclusive_lock_is_honoured_across_processes"]
fn cross_process_lock_probe() {
    let Ok(lock_path) = std::env::var(LOCK_PROBE_PATH) else {
        return;
    };
    let acquired = FileLock::try_acquire(&lock_path, LockMode::Exclusive)
        .expect("try_acquire must not fail")
        .is_some();
    std::process::exit(if acquired { PROBE_ACQUIRED } else { 0 });
}

fn spawn_lock_probe(lock_path: &Path) -> Result<i32> {
    let status = std::process::Command::new(std::env::current_exe()?)
        .args([
            "--exact",
            "cross_process_lock_probe",
            "--ignored",
            "--quiet",
        ])
        .env(LOCK_PROBE_PATH, lock_path)
        .status()?;
    Ok(status.code().unwrap_or(-1))
}

/// The locking protocol has to work between *processes*, which is what
/// issue #98 needs for one open store per process.
#[test]
fn exclusive_lock_is_honoured_across_processes() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("cross-process.doublets");
    let lock_path = link_cli::lock_file_path(&path);

    let held = FileLock::acquire(&lock_path, LockMode::Exclusive)?;
    assert_eq!(
        spawn_lock_probe(&lock_path)?,
        0,
        "a second process must not be able to take a held exclusive lock"
    );

    drop(held);
    assert_eq!(
        spawn_lock_probe(&lock_path)?,
        PROBE_ACQUIRED,
        "dropping the guard must release the lock for other processes"
    );

    Ok(())
}

/// Regression test for the upstream `platform-mem` behaviour described in
/// `link_cli::PersistentFileMapped`: the default `grow_filled` zeroes the
/// whole newly mapped region, wiping an existing database on open.
#[test]
fn persistent_file_mapped_preserves_existing_contents() -> Result<()> {
    use doublets::mem::RawMem;

    let dir = TempDir::new()?;
    let path = dir.path().join("raw.bin");

    {
        let mut mapped = PersistentFileMapped::<u32>::from_path(&path)?;
        let region = mapped.grow_filled(16, 0u32)?;
        region[0] = 0xdead_beef;
        region[1] = 42;
    }

    let mut mapped = PersistentFileMapped::<u32>::from_path(&path)?;
    let region = mapped.grow_filled(16, 0u32)?;
    assert_eq!(region[0], 0xdead_beef);
    assert_eq!(region[1], 42);

    Ok(())
}
