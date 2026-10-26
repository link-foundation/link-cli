//! Reopen the same native doublets page journal from Rust, C# and Node/Bun/Deno.
//! cargo run --manifest-path rust/Cargo.toml --features native-pages --example engine_native_pages -- <database> [--mirrored] [--delta] [--flat|--sorted-runs] [--snapshot] [--references] [--indexed-constraints] [--compact|--checkpoint-fixture=<json>]
use link_cli::engine::paged::{
    FileCheckpointPagedLog, FilePageDevice, MirroredPageDevice, PageDevice, PagedLog,
};
use link_cli::engine::{
    Constraint, ConstraintValidation, DeltaLog, Engine, EngineOptions, GroupCommitOutcome,
    HistoryPolicy, IsolationLevel, MemoryLog, RevisionEncoding, StateStructure,
};
use link_cli::{LinkError, LinksStorage, TransitionLogStore};

type Compact<L> = fn(&Engine<u64, L>) -> Result<bool, LinkError>;

fn run<L: TransitionLogStore>(
    mut log: L,
    options: EngineOptions,
    checkpoint: Option<&str>,
    compact: Option<Compact<L>>,
) -> anyhow::Result<L> {
    if let Some(seed) = checkpoint {
        if log.read_log_entries()?.is_empty() {
            log.append_log_entry(seed)?;
            log.flush_log()?;
        }
        anyhow::ensure!(log.read_log_entries()?.first().map(String::as_str) == Some(seed));
    }
    let branch_options = EngineOptions {
        require_durable_flush: false,
        ..options.clone()
    };
    let db = Engine::<u64, _>::open(log, options)?;
    let before = db.snapshot()?;
    let branch = compact
        .map(|_| db.fork(MemoryLog::default(), branch_options.clone()))
        .transpose()?;
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
    if compact.is_none() {
        assert_eq!(db.snapshot_at(before.revision)?.rows(), before.rows());
    }
    let after = db.snapshot()?;
    if let Some(compact) = compact {
        assert!(db.snapshot_at(before.revision).is_err());
        let mut private = db.begin()?;
        assert_eq!(private.create_link(0, 0)?, after.revision + 1);
        assert!(compact(&db)?);
        assert!(!compact(&db)?);
        assert_eq!(before.rows().len() + 2, after.rows().len());
        let branch = branch.unwrap();
        let descriptor = branch.merge_base_record();
        let child = branch
            .into_engine()
            .into_log()
            .map_err(|_| anyhow::anyhow!("live branch sessions"))?;
        let resumed = db.resume_branch(child, branch_options, &descriptor)?;
        assert_eq!(resumed.engine().snapshot()?.rows(), before.rows());
        assert_eq!(resumed.merge_base_record(), descriptor);
        drop(resumed);
        private.rollback();
        drop(private);
        println!("Native compacting pages group: {} -> {} revisions; {} -> {} rows; private reservation {}, pinned rows and branch base verified", before.revision, after.revision, before.rows().len(), after.rows().len(), after.revision + 1);
    } else if checkpoint.is_some() {
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
        history: if args.iter().any(|arg| arg == "--compact") {
            HistoryPolicy::Last(2)
        } else {
            HistoryPolicy::All
        },
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
    if args.iter().any(|arg| arg == "--compact") {
        anyhow::ensure!(
            checkpoint.is_none(),
            "compaction handoffs use their own two-revision retention control"
        );
        let log = FileCheckpointPagedLog::open(path, args.iter().any(|arg| arg == "--mirrored"))?;
        let mut log = if delta {
            run(
                DeltaLog::new(log, 128)?,
                options,
                None,
                Some(Engine::compact_journal),
            )?
            .into_inner()
        } else {
            run(log, options, None, Some(Engine::compact_journal))?
        };
        println!("Native checkpoint bank epoch: {}", log.epoch()?);
        log.close()?;
    } else if args.iter().any(|arg| arg == "--mirrored") {
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
        run(DeltaLog::new(log, 128)?, options, checkpoint, None)?.into_inner()
    } else {
        run(log, options, checkpoint, None)?
    };
    Ok(log.into_device())
}
