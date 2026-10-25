//! Reopen the same SQLite database from Rust, C# and Node/Bun/Deno.
//! cargo run --manifest-path rust/Cargo.toml --features sqlite --example engine_sqlite -- <database> [--delta] [--flat] [--snapshot]
use link_cli::engine::sqlite::{CheckpointMode, SqliteLog, SqliteOptions};
use link_cli::engine::{
    DeltaLog, Engine, EngineOptions, GroupCommitOutcome, HistoryPolicy, IsolationLevel,
    RevisionEncoding, StateStructure,
};
use link_cli::{LinksStorage, TransitionLogStore};

fn run<L: TransitionLogStore>(log: L, options: EngineOptions) -> anyhow::Result<L> {
    let db = Engine::<u64, _>::open(log, options)?;
    let before = db.snapshot()?;
    assert_eq!(before.revision, before.rows().len() as u64);
    for (&id, row) in before.rows() {
        assert_eq!(row.link.source, if id % 2 == 1 { 0 } else { id - 1 });
        assert_eq!(row.link.target, 0);
        assert_eq!(
            row.name.as_deref(),
            Some(if id % 2 == 1 {
                "\u{feff}first 雪\0😀"
            } else {
                "second"
            })
        );
    }
    let mut a = db.begin()?;
    let first = a.create_link(0, 0)?;
    a.set_name(first, Some("\u{feff}first 雪\0😀".into()))?;
    let mut b = db.begin()?;
    let second = b.create_link(first, 0)?;
    b.set_name(second, Some("second".into()))?;
    assert_eq!(first, before.revision + 1);
    assert_eq!(second, before.revision + 2);
    assert_eq!(
        db.commit_group(vec![a, b])?,
        [
            GroupCommitOutcome::Committed(before.revision + 1),
            GroupCommitOutcome::Committed(before.revision + 2)
        ]
    );
    assert_eq!(before.rows().len() + 2, db.snapshot()?.rows().len());
    assert_eq!(db.snapshot_at(before.revision)?.rows(), before.rows());
    println!(
        "SQLite group: {} -> {} revisions/rows; pinned predecessor and exact prior names verified",
        before.revision,
        db.snapshot()?.revision
    );
    db.into_log()
        .map_err(|_| anyhow::anyhow!("live engine sessions"))
}
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .ok_or_else(|| anyhow::anyhow!("provide a dedicated SQLite database path"))?;
    let options = EngineOptions {
        isolation: IsolationLevel::Snapshot,
        history: HistoryPolicy::All,
        require_durable_flush: true,
        state_structure: if args.iter().any(|arg| arg == "--flat") {
            StateStructure::Flat
        } else {
            StateStructure::Shared
        },
        revision_encoding: if args.iter().any(|arg| arg == "--snapshot") {
            RevisionEncoding::Snapshot
        } else {
            RevisionEncoding::Changes
        },
        ..Default::default()
    };
    let log = SqliteLog::open(
        path,
        SqliteOptions {
            auto_checkpoint_pages: 0,
            ..Default::default()
        },
    )?;
    let mut log = if args.iter().any(|arg| arg == "--delta") {
        run(DeltaLog::new(log, 128)?, options)?.into_inner()
    } else {
        run(log, options)?
    };
    let result = log.checkpoint(CheckpointMode::Passive)?;
    assert_eq!(result.busy, 0);
    println!("checkpoint {result:?}");
    Ok(())
}
