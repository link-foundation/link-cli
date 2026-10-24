//! cargo run --example engine_strategies --manifest-path rust/Cargo.toml
use link_cli::engine::replication::{ReplicatedLog, Synchronization};
use link_cli::engine::{Engine, EngineOptions, HistoryPolicy, MemoryLog};
use link_cli::{FileTransitionLog, LinksStorage};

fn main() -> anyhow::Result<()> {
    // Bare stores remain the cheapest disposable cache. This example selects
    // isolated sessions and two volatile copies, with explicit acknowledgement.
    let log = ReplicatedLog::new(
        MemoryLog::default(),
        vec![MemoryLog::default()],
        Synchronization::Synchronous {
            acknowledgements: 2,
        },
    )?;
    let db = Engine::<u64, _>::open(
        log,
        EngineOptions {
            history: HistoryPolicy::All,
            ..Default::default()
        },
    )?;
    let mut tx = db.begin()?;
    let id = tx.create_link(0, 0)?;
    tx.set_name(id, Some("hello 雪".into()))?;
    assert!(db.snapshot()?.links().is_empty());
    tx.commit()?;
    assert!(db.snapshot_at(0)?.links().is_empty());
    println!("committed revision {}, link {id}", db.snapshot()?.revision);

    let mut branch = db.fork(MemoryLog::default(), EngineOptions::default())?;
    let mut edit = branch.engine().begin()?;
    edit.set_name(id, Some("edited independently".into()))?;
    edit.commit()?;
    println!("merged revision {}", branch.merge()?);
    // Persist this with application metadata to resume the same branch later.
    println!("branch merge base {}", branch.merge_base_record());

    // Optional path selects durable storage instead; the exclusive writer
    // lock is held until this engine and every session have been dropped.
    if let Some(path) = std::env::args().nth(1) {
        let persistent = Engine::<u64, _>::open(
            FileTransitionLog::open_exclusive(path)?,
            EngineOptions {
                require_durable_flush: true,
                ..Default::default()
            },
        )?;
        let mut tx = persistent.begin()?;
        let id = tx.create_link(0, 0)?;
        tx.set_name(id, Some("durable".into()))?;
        println!("durable revision {}", tx.commit()?);
    }
    Ok(())
}
