//! cargo run --features etcd --example cluster -- http://localhost:2379 /demo/
use link_cli::engine::{cluster::ClusterEngine, etcd::EtcdJournalStore, EngineOptions};
use link_cli::LinksStorage;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: cluster endpoints-comma-separated prefix/".into());
    }
    let store = EtcdJournalStore::new(
        args[0].split(',').map(String::from).collect(),
        args[1].clone(),
    )?;
    let engine = ClusterEngine::<u32, _>::new(
        store,
        EngineOptions {
            require_durable_flush: true,
            ..Default::default()
        },
    )?;
    let mut transaction = engine.begin(&["a", "b"])?;
    for key in ["a", "b"] {
        let shard = transaction.shard(key)?;
        if shard.get_link(1).is_none() {
            shard.create_link(0, 0)?;
        }
        shard.set_name(1, Some("Rust coordinated 雪".into()))?;
    }
    transaction.commit()?;
    println!("PASS Rust HTTP: both shard journals committed atomically");
    Ok(())
}
