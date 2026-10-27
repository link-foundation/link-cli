//! Explicit collection of separately owned namespaces with portable checkpoints.
use link_cli::engine::{
    cluster::{AtomicJournalStore, ClusterEngine},
    etcd::EtcdJournalStore,
    partitioned::JournalChunkOptions,
    reclaiming::{ReclaimingPartitionedJournalStore, ReclamationOptions},
    Engine, EngineOptions, HistoryPolicy, MemoryLog,
};
use link_cli::LinksStorage;
use std::collections::BTreeMap;
type Store = ReclaimingPartitionedJournalStore<EtcdJournalStore, EtcdJournalStore>;
fn open(endpoints: &[String], prefix: &str) -> Result<Store, Box<dyn std::error::Error>> {
    let provider = |name| EtcdJournalStore::new(endpoints.to_vec(), format!("{prefix}{name}/"));
    let catalog = provider("catalog")?;
    let partitions = BTreeMap::from([
        ("east".into(), provider("east")?),
        ("west".into(), provider("west")?),
    ]);
    let route = |key: &str| if key == "a" { "east" } else { "west" }.into();
    let options = ReclamationOptions {
        page_keys: 1,
        cache_bytes: 65536,
        ..Default::default()
    };
    Ok(
        if std::env::var("RECLAMATION_CHUNKS").as_deref() == Ok("1") {
            Store::open_chunked(
                catalog,
                partitions,
                route,
                options,
                JournalChunkOptions {
                    max_records: 2,
                    ..Default::default()
                },
            )?
        } else {
            Store::open(catalog, partitions, route, options)?
        },
    )
}
fn verify(store: &mut Store) -> Result<u64, Box<dyn std::error::Error>> {
    let journals = store.read(&["a".into(), "b".into()])?;
    let mut revisions = Vec::new();
    for journal in journals {
        let engine = Engine::<u32, _>::open(
            MemoryLog {
                entries: journal.entries,
            },
            EngineOptions {
                history: HistoryPolicy::All,
                ..Default::default()
            },
        )?;
        let current = engine.snapshot()?;
        let revision = current.revision;
        if revision > 0 {
            assert_eq!(current.rows().len(), 1);
            assert_eq!(
                current.get(1).unwrap().name.as_deref(),
                Some(format!("revision-{revision} 雪").as_str())
            );
            for old in 1..revision {
                assert_eq!(
                    engine.snapshot_at(old)?.get(1).unwrap().name.as_deref(),
                    Some(format!("revision-{old} 雪").as_str())
                );
            }
            let mut probe = engine.begin()?;
            assert_eq!(probe.create_link(0, 0)?, u32::try_from(revision + 2)?);
            probe.rollback();
        }
        revisions.push(revision);
    }
    assert_eq!(revisions[0], revisions[1]);
    Ok(revisions[0])
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err(
            "usage: journal_reclamation endpoints-comma-separated dedicated-prefix/".into(),
        );
    }
    let endpoints: Vec<_> = args[1].split(',').map(String::from).collect();
    let mut collector = open(&endpoints, &args[2])?;
    let revision = verify(&mut collector)?;
    // Count the exact old reference graph before publication. With chunks,
    // retained history can occupy more than one object per shard.
    let mut catalog = EtcdJournalStore::new(endpoints.clone(), format!("{}catalog/", args[2]))?;
    let retired: usize = catalog
        .read(&["a".into(), "b".into()])?
        .iter()
        .map(|root| {
            if root.generation == 0 {
                0
            } else if root.entries[0] == "chunks2" {
                root.entries[3]
                    .parse::<usize>()
                    .expect("verified chunk count")
            } else {
                assert_eq!(root.entries[0], "blob2");
                1
            }
        })
        .sum();
    // The bounded example is intended for sequential language handoffs.
    assert!(revision < 32);
    let engine = ClusterEngine::<u32, _>::new(
        open(&endpoints, &args[2])?,
        EngineOptions {
            history: HistoryPolicy::All,
            require_durable_flush: true,
            ..Default::default()
        },
    )?
    .with_journal_checkpoint_interval(1)?;
    let mut stale = engine.begin(&["a", "b"])?;
    let mut write = engine.begin(&["a", "b"])?;
    for key in ["a", "b"] {
        let shard = write.shard(key)?;
        if revision == 0 {
            assert_eq!(shard.create_link(0, 0)?, 1);
        }
        let gap = shard.create_link(0, 0)?;
        assert_eq!(gap, u32::try_from(revision + 2)?);
        shard.delete_link(gap)?;
        shard.set_name(1, Some(format!("revision-{} 雪", revision + 1)))?;
    }
    write.commit()?;
    let collected = collector.collect_garbage()?.expect("uncontended fence");
    assert_eq!(collected.deleted, retired);
    if revision > 0 {
        assert_eq!(
            stale.shard("a")?.get_name(1),
            Some(format!("revision-{revision} 雪"))
        );
    }
    assert!(stale.commit().unwrap_err().to_string().contains("conflict"));
    assert_eq!(verify(&mut collector)?, revision + 1);
    println!("PASS Rust reclamation: revision {}, atomic checkpoints, all retained history, deleted allocator gap, owned stale conflict, {} retired objects", revision + 1, collected.deleted);
    Ok(())
}
