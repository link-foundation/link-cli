use link_cli::{
    engine::{
        cluster::{AtomicJournalStore, ClusterEngine, JournalVersion},
        lsm::{FileLsmJournalStore, FileLsmOptions, LsmOptions, LsmStoreOptions},
        maintenance::{JournalMaintenanceStore, JournalNamespaceEntry, JournalScanOptions},
        partitioned::{JournalChunkOptions, PartitionedJournalStore},
        reclaiming::{ReclaimingPartitionedJournalStore, ReclamationOptions},
        EngineOptions,
    },
    LinkError, LinksStorage,
};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, Mutex},
};
pub(super) struct Shared<S>(Arc<Mutex<S>>);
impl<S> Clone for Shared<S> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}
impl<S> Shared<S> {
    pub(super) fn new(value: S) -> Self {
        Self(Arc::new(Mutex::new(value)))
    }
    pub(super) fn with<T>(
        &self,
        action: impl FnOnce(&mut S) -> Result<T, LinkError>,
    ) -> Result<T, LinkError> {
        action(
            &mut *self
                .0
                .lock()
                .map_err(|_| std::io::Error::other("example owner mutex poisoned"))?,
        )
    }
}
impl<S: AtomicJournalStore> AtomicJournalStore for Shared<S> {
    fn supports_durable_flush(&self) -> bool {
        self.0
            .lock()
            .map(|s| s.supports_durable_flush())
            .unwrap_or(false)
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        self.with(|s| s.read(keys))
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        self.with(|s| s.compare_exchange(keys, expected, updates))
    }
}
impl<S: JournalMaintenanceStore> JournalMaintenanceStore for Shared<S> {
    fn scan_namespace(
        &mut self,
        options: JournalScanOptions,
    ) -> Result<Vec<JournalNamespaceEntry>, LinkError> {
        self.with(|s| s.scan_namespace(options))
    }
    fn compare_delete(&mut self, key: &str, expected: u64) -> Result<bool, LinkError> {
        self.with(|s| s.compare_delete(key, expected))
    }
}
fn exercise<S: AtomicJournalStore>(store: S, write: bool) -> Result<(), LinkError> {
    let engine = ClusterEngine::<u32, _>::new(
        store,
        EngineOptions {
            require_durable_flush: cfg!(unix),
            ..Default::default()
        },
    )?
    .with_journal_checkpoint_interval(1)?;
    if write {
        for revision in 0..3 {
            let mut transaction = engine.begin(&["a", "b"])?;
            for key in ["a", "b"] {
                let shard = transaction.shard(key)?;
                if shard.get_link(1).is_none() {
                    shard.create_link(0, 0)?;
                }
                shard.set_name(1, Some(format!("revision-{revision} 雪😀")))?;
            }
            transaction.commit()?;
        }
        let mut stale = engine.begin(&["a", "b"])?;
        let mut winner = engine.begin(&["a"])?;
        winner.shard("a")?.set_name(1, Some("winner".into()))?;
        winner.commit()?;
        stale.shard("b")?.set_name(1, Some("lost".into()))?;
        assert!(stale.commit().unwrap_err().to_string().contains("conflict"));
    } else {
        let mut reader = engine.begin(&["a", "b"])?;
        assert_eq!(reader.shard("a")?.get_name(1).as_deref(), Some("winner"));
        assert_eq!(
            reader.shard("b")?.get_name(1).as_deref(),
            Some("revision-2 雪😀")
        );
        for key in ["a", "b"] {
            assert!(reader.shard(key)?.get_link(1).is_some());
        }
    }
    Ok(())
}
fn route(key: &str) -> String {
    if key == "a" { "east" } else { "west" }.into()
}
fn open(directory: &Path, name: &str) -> Result<Shared<FileLsmJournalStore>, LinkError> {
    let path = directory.join(name);
    std::fs::create_dir_all(&path)?;
    Ok(Shared::new(FileLsmJournalStore::open(
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
            ..Default::default()
        },
    )?))
}
pub(super) fn run(mode: &str) -> Result<(), LinkError> {
    assert!(matches!(
        mode,
        "direct" | "partitioned" | "chunks" | "reclaiming"
    ));
    let directory = tempfile::tempdir()?;
    for write in [true, false] {
        let catalog = open(directory.path(), "catalog")?;
        let mut physical = vec![catalog.clone()];
        if mode == "direct" {
            exercise(catalog, write)?;
        } else {
            let east = open(directory.path(), "east")?;
            let west = open(directory.path(), "west")?;
            physical.extend([east.clone(), west.clone()]);
            let parts = BTreeMap::from([("east".into(), east), ("west".into(), west)]);
            if mode == "partitioned" || mode == "chunks" {
                let store = PartitionedJournalStore::new(catalog, parts, route)?;
                let store = if mode == "chunks" {
                    store.with_chunks(JournalChunkOptions {
                        max_records: 2,
                        ..Default::default()
                    })?
                } else {
                    store
                };
                exercise(store, write)?;
            } else {
                let store = Shared::new(ReclaimingPartitionedJournalStore::open(
                    catalog,
                    parts,
                    route,
                    ReclamationOptions::default(),
                )?);
                exercise(store.clone(), write)?;
                if write {
                    assert!(store.with(|s| s.collect_garbage())?.unwrap().deleted > 0);
                }
            }
        }
        for store in physical {
            store.with(FileLsmJournalStore::compact)?;
            store.with(FileLsmJournalStore::close)?;
        }
    }
    println!("PASS Rust native LSM {mode}: actual links, checkpoint journals, stale group rejection and file reopen");
    Ok(())
}
