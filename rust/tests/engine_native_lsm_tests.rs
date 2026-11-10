#![cfg(all(feature = "physical-lsm", feature = "native-pages", any(unix, windows)))]
use link_cli::engine::{
    cluster::{AtomicJournalStore, JournalVersion},
    lsm::{FileLsmJournalStore, FileLsmOptions, LsmOptions, LsmStoreOptions},
    maintenance::{JournalMaintenanceStore, JournalScanOptions},
};
use link_cli::LinkError;
use std::{collections::BTreeMap, fs, path::Path};

fn options(mode: usize) -> FileLsmOptions {
    FileLsmOptions {
        store: LsmStoreOptions {
            limits: LsmOptions {
                max_keys: 32,
                max_bytes: 4096,
                buffer_keys: 2,
                buffer_bytes: 128,
                max_runs: 2,
                max_wal_records: 4,
                max_wal_bytes: 8192,
            },
            max_namespace_entries: 16,
        },
        max_pages: 128,
        mirrored_root: mode & 1 != 0,
        mirrored_banks: mode & 2 != 0,
        mirrored_tables: mode & 4 != 0,
    }
}
fn put(store: &mut FileLsmJournalStore, key: &str, value: &str) -> Result<(), LinkError> {
    let keys = vec![key.into()];
    let before = store.read(&keys)?;
    assert!(store.compare_exchange(&keys, &before, &[Some(vec![value.into()])])?);
    Ok(())
}
fn snapshot(path: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().into_string().unwrap(),
                fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}
#[test]
fn native_lsm_all_eight_modes_preserve_owned_groups_compaction_and_following_reopen(
) -> Result<(), LinkError> {
    for mode in 0..8 {
        let dir = tempfile::tempdir()?;
        let mut store = FileLsmJournalStore::open(dir.path(), options(mode))?;
        assert_eq!(store.supports_durable_flush(), cfg!(unix));
        assert!(FileLsmJournalStore::open(dir.path(), options(mode)).is_err());
        let keys = vec!["a".into(), "b".into()];
        let missing = store.read(&keys)?;
        assert!(store.compare_exchange(
            &keys,
            &missing,
            &[Some(vec!["\u{feff}雪\0😀\n".into()]), Some(vec![])]
        )?);
        store.flush()?;
        let mut owned = store.read(&keys)?;
        owned[0].entries[0] = "caller".into();
        assert_eq!(store.read(&keys)?[0].entries[0], "\u{feff}雪\0😀\n");
        assert!(!store.compare_exchange(&keys, &missing, &[None, Some(vec!["lost".into()])])?);
        let before = store.read(&keys)?;
        assert_eq!(before[0].generation, before[1].generation);
        assert!(store.compare_delete("a", before[0].generation)?);
        assert!(!store.compare_delete("a", before[0].generation)?);
        assert_eq!(
            store
                .scan_namespace(JournalScanOptions::default())?
                .iter()
                .map(|row| row.key.as_str())
                .collect::<Vec<_>>(),
            ["b"]
        );
        put(&mut store, "a", "following")?;
        store.compact()?;
        let expected = store.read(&keys)?;
        store.close()?;
        store.close()?;
        assert!(store.read(&keys).is_err());
        let image = snapshot(dir.path());
        let mut reopened = FileLsmJournalStore::open(dir.path(), options(mode))?;
        assert_eq!(reopened.read(&keys)?, expected);
        reopened.close()?;
        assert_eq!(snapshot(dir.path()), image);
        assert!(image
            .keys()
            .all(|name| !name.ends_with(".writer") && name != ".lsm-writer"));
    }
    Ok(())
}
#[test]
fn native_lsm_invalid_admission_precedes_directory_io() {
    for (pages, names) in [(0, 16), (65_537, 16), (128, 1), (128, 65_537)] {
        let dir = tempfile::tempdir().unwrap();
        let mut selected = options(0);
        selected.max_pages = pages;
        selected.store.max_namespace_entries = names;
        assert!(FileLsmJournalStore::open(dir.path(), selected).is_err());
        assert!(snapshot(dir.path()).is_empty());
    }
    let dir = tempfile::tempdir().unwrap();
    let mut selected = options(0);
    selected.store.limits.buffer_keys = 33;
    assert!(FileLsmJournalStore::open(dir.path(), selected).is_err());
    assert!(snapshot(dir.path()).is_empty());
}
#[test]
fn native_lsm_foreign_noncanonical_stale_and_uninitialized_tables_preserve_bytes() {
    for name in [
        "foreign",
        "table-0",
        "table-01",
        "table-18446744073709551616",
        "table-1.writer",
        "table-1.mirror",
        "table-1.meta0",
        "wal.writer",
        ".lsm-writer",
    ] {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(name), b"evidence").unwrap();
        let before = snapshot(dir.path());
        assert!(
            FileLsmJournalStore::open(dir.path(), options(0)).is_err(),
            "{name}"
        );
        assert_eq!(snapshot(dir.path()), before, "{name}");
    }
}
fn two_tables(path: &Path) -> Result<Vec<String>, LinkError> {
    let mut store = FileLsmJournalStore::open(path, options(7))?;
    put(&mut store, "a", "old")?;
    put(&mut store, "b", "second")?;
    store.flush()?;
    put(&mut store, "a", "selected")?;
    store.flush()?;
    store.close()?;
    Ok(snapshot(path)
        .keys()
        .filter(|name| {
            name.strip_prefix("table-")
                .is_some_and(|id| id.parse::<u64>().is_ok())
        })
        .cloned()
        .collect())
}
#[test]
fn native_lsm_missing_survivor_waits_for_every_selected_binding_before_creating_or_repairing(
) -> Result<(), LinkError> {
    let dir = tempfile::tempdir()?;
    let tables = two_tables(dir.path())?;
    assert!(tables.len() >= 2);
    for name in [
        tables[0].clone(),
        format!("{}.meta0", tables[0]),
        format!("{}.meta1", tables[0]),
        "wal.meta0".into(),
        "wal.meta1".into(),
    ] {
        fs::remove_file(dir.path().join(name))?;
    }
    let mut originals = Vec::new();
    for name in [&tables[1], &format!("{}.mirror", tables[1])] {
        let path = dir.path().join(name);
        let bytes = fs::read(&path)?;
        let mut damaged = bytes.clone();
        damaged[100] ^= 1;
        fs::write(&path, damaged)?;
        originals.push((path, bytes));
    }
    let before = snapshot(dir.path());
    assert!(FileLsmJournalStore::open(dir.path(), options(7)).is_err());
    assert_eq!(snapshot(dir.path()), before);
    for (path, bytes) in originals {
        fs::write(path, bytes)?;
    }
    let mut recovered = FileLsmJournalStore::open(dir.path(), options(7))?;
    assert_eq!(recovered.read(&["a".into()])?[0].entries, ["selected"]);
    recovered.close()?;
    assert_eq!(
        fs::read(dir.path().join(&tables[0]))?,
        fs::read(dir.path().join(format!("{}.mirror", tables[0])))?
    );
    Ok(())
}
#[test]
fn native_lsm_corrupt_binding_leaves_uncommitted_wal_suffix_and_all_bytes() -> Result<(), LinkError>
{
    let dir = tempfile::tempdir()?;
    let tables = two_tables(dir.path())?;
    for name in ["wal.bank0", "wal.bank1"] {
        use std::io::Write;
        fs::OpenOptions::new()
            .append(true)
            .open(dir.path().join(name))?
            .write_all(b"uncommitted suffix")?;
    }
    for suffix in ["", ".mirror"] {
        let path = dir.path().join(format!("{}{suffix}", tables[0]));
        let mut bytes = fs::read(&path)?;
        bytes[100] ^= 1;
        fs::write(path, bytes)?;
    }
    let before = snapshot(dir.path());
    assert!(FileLsmJournalStore::open(dir.path(), options(7)).is_err());
    assert_eq!(snapshot(dir.path()), before);
    Ok(())
}
#[test]
fn native_lsm_positive_empty_and_tombstone_generation_remain_distinct() -> Result<(), LinkError> {
    let dir = tempfile::tempdir()?;
    let mut store = FileLsmJournalStore::open(dir.path(), options(0))?;
    assert!(store.compare_exchange(
        &["empty".into()],
        &[JournalVersion::default()],
        &[Some(vec![])]
    )?);
    assert!(store.compare_delete("empty", 1)?);
    store.compact()?;
    store.close()?;
    let mut store = FileLsmJournalStore::open(dir.path(), options(0))?;
    assert_eq!(store.read(&["empty".into()])?[0].generation, 2);
    assert!(store
        .scan_namespace(JournalScanOptions::default())?
        .is_empty());
    store.close()?;
    Ok(())
}

#[test]
fn native_lsm_one_surviving_plain_metadata_slot_is_sufficient() -> Result<(), LinkError> {
    let directory = tempfile::tempdir()?;
    let mut store = FileLsmJournalStore::open(directory.path(), options(0))?;
    put(&mut store, "a", "selected")?;
    store.flush()?;
    store.close()?;
    let table = snapshot(directory.path())
        .keys()
        .find(|name| name.starts_with("table-") && !name.contains('.'))
        .unwrap()
        .clone();
    // Slot zero may still describe the initial empty table. Preserve the
    // committed nonempty decision, as in the shared JavaScript control.
    let first = fs::read(directory.path().join(format!("{table}.meta0")))?;
    let pages = u64::from_le_bytes(first[40..48].try_into().unwrap());
    let obsolete = if pages == 0 { 0 } else { 1 };
    fs::remove_file(directory.path().join(format!("{table}.meta{obsolete}")))?;
    let mut reopened = FileLsmJournalStore::open(directory.path(), options(0))?;
    assert_eq!(reopened.read(&["a".into()])?[0].entries, ["selected"]);
    reopened.close()?;
    Ok(())
}

#[test]
fn native_lsm_namespace_bound_precedes_opening_wal_and_preserves_orphans() -> Result<(), LinkError>
{
    let directory = tempfile::tempdir()?;
    for id in 1..=3 {
        fs::write(
            directory.path().join(format!("table-{id}.meta0")),
            b"unselected",
        )?;
    }
    let before = snapshot(directory.path());
    let mut selected = options(0);
    selected.store.max_namespace_entries = 2;
    let error = FileLsmJournalStore::open(directory.path(), selected)
        .err()
        .unwrap();
    assert!(error.to_string().contains("namespace bound"));
    assert_eq!(snapshot(directory.path()), before);
    Ok(())
}

#[test]
fn native_lsm_directory_entry_is_rejected_before_any_child_open() -> Result<(), LinkError> {
    let directory = tempfile::tempdir()?;
    fs::create_dir(directory.path().join("wal"))?;
    assert!(FileLsmJournalStore::open(directory.path(), options(0)).is_err());
    assert!(!directory.path().join(".lsm-writer").exists());
    assert_eq!(fs::read_dir(directory.path())?.count(), 1);
    assert_eq!(fs::read_dir(directory.path().join("wal"))?.count(), 0);
    Ok(())
}

#[cfg(unix)]
#[test]
fn native_lsm_symbolic_roots_children_and_fifo_are_rejected_without_following(
) -> Result<(), LinkError> {
    use std::{
        ffi::CString,
        os::unix::{ffi::OsStrExt, fs::symlink},
    };
    let root = tempfile::tempdir()?;
    let target = root.path().join("target");
    fs::create_dir(&target)?;
    let alias = root.path().join("alias");
    symlink(&target, &alias)?;
    assert!(FileLsmJournalStore::open(alias, options(0)).is_err());
    assert!(snapshot(&target).is_empty());
    for fifo in [false, true] {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("wal");
        if fifo {
            let name = CString::new(path.as_os_str().as_bytes()).unwrap();
            // SAFETY: the bounded native pathname is NUL-terminated; creation
            // does not open the FIFO or wait for another process.
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        } else {
            symlink(&target, &path)?;
        }
        assert!(FileLsmJournalStore::open(directory.path(), options(0)).is_err());
        assert!(!directory.path().join(".lsm-writer").exists());
        assert_eq!(fs::read_dir(directory.path())?.count(), 1);
        assert!(snapshot(&target).is_empty());
    }
    Ok(())
}
