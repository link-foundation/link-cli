//! Reopen the same native doublets page journal from Rust, C# and Node/Bun/Deno.
//! cargo run --manifest-path rust/Cargo.toml --features native-pages --example engine_native_pages -- <database> [--mirrored] [--delta] [--flat|--sorted-runs] [--snapshot] [--references] [--indexed-constraints] [--checkpoint-fixture=<json>]
use link_cli::engine::paged::{FilePageDevice, MirroredPageDevice, PageDevice, PagedLog};
use link_cli::engine::{
    Constraint, ConstraintValidation, DeltaLog, Engine, EngineOptions, GroupCommitOutcome,
    HistoryPolicy, IsolationLevel, RevisionEncoding, StateStructure,
};
use link_cli::{LinksStorage, TransitionLogStore};

fn run<L: TransitionLogStore>(
    mut log: L,
    options: EngineOptions,
    checkpoint: Option<&str>,
) -> anyhow::Result<L> {
    if let Some(seed) = checkpoint {
        if log.read_log_entries()?.is_empty() {
            log.append_log_entry(seed)?;
            log.flush_log()?;
        }
        anyhow::ensure!(log.read_log_entries()?.first().map(String::as_str) == Some(seed));
    }
    let db = Engine::<u64, _>::open(log, options)?;
    let before = db.snapshot()?;
    if let Some(seed) = checkpoint {
        assert!(before.revision >= 40);
        assert_eq!(before.revision - 8, before.rows().len() as u64);
        assert_eq!(db.snapshot_at(40)?.rows().len(), 32);
        assert!(db.snapshot_at(39).is_err());
        assert!((33..=40).all(|id| before.get(id).is_none()));
        assert_eq!(
            db.export_checkpoint()?.first().map(String::as_str),
            Some(seed)
        );
    } else {
        assert_eq!(before.revision, before.rows().len() as u64);
    }
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
    let after = db.snapshot()?;
    if checkpoint.is_some() {
        println!(
            "Native checkpoint pages group: {} -> {} revisions; {} -> {} rows; pinned predecessor and exact prior names verified",
            before.revision, after.revision, before.rows().len(), after.rows().len()
        );
    } else {
        println!(
            "Native pages group: {} -> {} revisions/rows; pinned predecessor and exact prior names verified",
            before.revision, after.revision
        );
    }
    db.into_log()
        .map_err(|_| anyhow::anyhow!("live engine sessions"))
}
fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let path = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .ok_or_else(|| anyhow::anyhow!("provide a dedicated native page database path"))?;
    let options = EngineOptions {
        constraint_validation: if args.iter().any(|arg| arg == "--indexed-constraints") {
            ConstraintValidation::Indexed
        } else {
            ConstraintValidation::Scan
        },
        // Repeated providers deliberately reuse pairs/names; only references
        // are constrained in this interoperability example.
        constraints: if args
            .iter()
            .any(|arg| arg == "--references" || arg == "--indexed-constraints")
        {
            vec![Constraint::References]
        } else {
            vec![]
        },
        isolation: IsolationLevel::Snapshot,
        history: HistoryPolicy::All,
        require_durable_flush: true,
        state_structure: if args.iter().any(|arg| arg == "--flat") {
            StateStructure::Flat
        } else if args.iter().any(|arg| arg == "--sorted-runs") {
            StateStructure::SortedRuns
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
    let delta = args.iter().any(|arg| arg == "--delta");
    let checkpoint = if let Some(path) = args
        .iter()
        .find_map(|arg| arg.strip_prefix("--checkpoint-fixture="))
    {
        let fixture: serde_json::Value = serde_json::from_reader(std::fs::File::open(path)?)?;
        Some(
            fixture["nativePageSeed"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("fixture must contain nativePageSeed"))?
                .to_owned(),
        )
    } else {
        None
    };
    if args.iter().any(|arg| arg == "--mirrored") {
        run_device(
            MirroredPageDevice::open(path)?,
            options,
            delta,
            checkpoint.as_deref(),
        )?
        .close()?;
    } else {
        run_device(
            FilePageDevice::open(path)?,
            options,
            delta,
            checkpoint.as_deref(),
        )?
        .close()?;
    }
    Ok(())
}
fn run_device<D: PageDevice>(
    device: D,
    options: EngineOptions,
    delta: bool,
    checkpoint: Option<&str>,
) -> anyhow::Result<D> {
    let log = PagedLog::open(device)?;
    let log = if delta {
        run(DeltaLog::new(log, 128)?, options, checkpoint)?.into_inner()
    } else {
        run(log, options, checkpoint)?
    };
    Ok(log.into_device())
}
