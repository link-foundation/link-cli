//! cargo run --manifest-path rust/Cargo.toml --example engine_group_commit -- <journal>
//! Add --delta for localized encoding. Keep the group bounded by the caller.
use link_cli::engine::{
    DeltaLog, Engine, EngineOptions, GroupCommitOutcome, HistoryPolicy, IsolationLevel,
};
use link_cli::{FileTransitionLog, LinksStorage, TransitionLogStore};

fn run<L: TransitionLogStore>(log: L) -> anyhow::Result<()> {
    let db = Engine::<u64, _>::open(
        log,
        EngineOptions {
            isolation: IsolationLevel::Snapshot,
            history: HistoryPolicy::All,
            require_durable_flush: true,
            ..Default::default()
        },
    )?;
    let mut first = db.begin()?;
    let a = first.create_link(0, 0)?;
    first.set_name(a, Some("first independent session".into()))?;
    let mut second = db.begin()?;
    let b = second.create_link(a, 0)?;
    second.set_name(b, Some("second independent session".into()))?;
    for outcome in db.commit_group(vec![first, second])? {
        match outcome {
            GroupCommitOutcome::Committed(revision) => println!("acknowledged revision {revision}"),
            GroupCommitOutcome::Aborted(error) => {
                println!("known abort: {error}; retry with a new session")
            }
            GroupCommitOutcome::Indeterminate(error) => {
                anyhow::bail!("{error}; inspect reopened state before retrying")
            }
        }
    }
    Ok(())
}
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = args
        .iter()
        .find(|a| !a.starts_with("--"))
        .ok_or_else(|| anyhow::anyhow!("provide an exclusive journal path"))?;
    let mut log = FileTransitionLog::open_exclusive(path)?;
    // Append writes bytes; the shared flush makes the complete prefix durable.
    log.set_sync_on_append(false);
    if args.iter().any(|a| a == "--delta") {
        run(DeltaLog::new(log, 128)?)
    } else {
        run(log)
    }
}
