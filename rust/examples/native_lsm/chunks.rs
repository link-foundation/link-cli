use super::consumer::Shared;
use link_cli::{
    engine::{
        cluster::{AtomicJournalStore, JournalVersion},
        lsm::{FileLsmJournalStore, FileLsmOptions, LsmOptions, LsmStoreOptions},
        partitioned::{JournalChunkOptions, PartitionedJournalStore},
        reclaiming::{ReclaimingPartitionedJournalStore, ReclamationOptions},
    },
    LinkError,
};
use std::{collections::BTreeMap, path::Path};

fn check<S: AtomicJournalStore + ?Sized>(
    store: &mut S,
    seed: &[String],
    advanced: bool,
    reclaiming: bool,
) -> Result<(), LinkError> {
    let mut entries = seed.to_vec();
    if advanced {
        entries.push("next".into());
    }
    assert_eq!(
        store.read(&["a".into(), "b".into()])?,
        vec![
            JournalVersion {
                generation: if reclaiming {
                    if advanced {
                        4
                    } else {
                        2
                    }
                } else if advanced {
                    2
                } else {
                    1
                },
                entries
            },
            JournalVersion {
                generation: if reclaiming { 2 } else { 1 },
                entries: vec![]
            },
        ]
    );
    Ok(())
}
pub(super) fn run(
    action: &str,
    directory: &str,
    mirrors: &str,
    reclaiming: bool,
) -> Result<(), LinkError> {
    assert!(matches!(
        action,
        "write" | "read" | "mutate" | "read-advanced"
    ));
    assert!(matches!(mirrors, "plain" | "mirrored"));
    if action == "write" {
        assert_eq!(std::fs::read_dir(directory)?.count(), 0);
    }
    let mirrored = mirrors == "mirrored";
    let open = |name| {
        let path = Path::new(directory).join(name);
        if action == "write" {
            std::fs::create_dir(&path)?;
        }
        FileLsmJournalStore::open(
            path,
            FileLsmOptions {
                store: LsmStoreOptions {
                    limits: LsmOptions {
                        max_keys: 256,
                        max_bytes: 1048576,
                        buffer_keys: 64,
                        buffer_bytes: 262144,
                        ..Default::default()
                    },
                    ..Default::default()
                },
                max_pages: 128,
                mirrored_root: mirrored,
                mirrored_banks: mirrored,
                mirrored_tables: mirrored,
            },
        )
        .map(Shared::new)
    };
    let catalog = open("catalog")?;
    let east = open("east")?;
    let parts = BTreeMap::from([("east".into(), east.clone())]);
    let chunks = JournalChunkOptions {
        max_records: if action == "mutate" { 3 } else { 2 },
        max_bytes: 64,
        max_chunks: 32,
    };
    let collector = if reclaiming {
        Some(Shared::new(
            ReclaimingPartitionedJournalStore::open_chunked(
                catalog.clone(),
                parts.clone(),
                |_| "east".into(),
                ReclamationOptions {
                    cache_bytes: 256,
                    ..Default::default()
                },
                chunks,
            )?,
        ))
    } else {
        None
    };
    let mut store: Box<dyn AtomicJournalStore> = if let Some(collector) = &collector {
        Box::new(collector.clone())
    } else {
        Box::new(
            PartitionedJournalStore::new(catalog.clone(), parts, |_| "east".into())?
                .with_object_cache(256)
                .with_chunks(chunks)?,
        )
    };
    let keys = vec!["a".into(), "b".into()];
    let seed = ["aa", "bb", "\u{feff}雪", "😀\0", "tail"]
        .map(String::from)
        .to_vec();
    if action == "write" {
        let base = store.read(&keys)?;
        assert!(store.compare_exchange(&keys, &base, &[Some(seed.clone()), Some(vec![])])?);
    }
    check(&mut *store, &seed, action == "read-advanced", reclaiming)?;
    if action == "mutate" {
        let before = store.read(&keys)?;
        if let Some(collector) = &collector {
            assert_eq!(collector.with(|s| s.collect_garbage())?.unwrap().epoch, 1);
        }
        let mut advanced = seed.clone();
        advanced.push("next".into());
        assert!(store.compare_exchange(&keys, &before, &[Some(advanced), None])?);
        assert_eq!(before[0].entries, seed);
        assert!(!store.compare_exchange(&keys, &before, &[Some(vec!["stale".into()]), None])?);
        check(&mut *store, &seed, true, reclaiming)?;
        if let Some(collector) = &collector {
            assert_eq!(collector.with(|s| s.collect_garbage())?.unwrap().deleted, 3);
        }
    }
    for files in [catalog, east] {
        if action == "write" || action == "mutate" {
            files.with(FileLsmJournalStore::flush)?;
            files.with(FileLsmJournalStore::compact)?;
        }
        files.with(FileLsmJournalStore::close)?;
    }
    println!("PASS Rust native chunks reclaiming={reclaiming} {action} {mirrors}");
    Ok(())
}
