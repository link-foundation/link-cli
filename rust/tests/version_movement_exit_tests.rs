//! Real bounded child exits leave durable movement ownership for reopening.
#![cfg(not(target_arch = "wasm32"))]
use anyhow::{bail, Result};
use link_cli::{
    CommitMode, GenericVersionControlDecorator, LogRetentionPolicy, NamedTypeLinks,
    NamedTypesDecorator, TransactionsDecorator, VersionControlDecorator,
};
use std::{
    cell::RefCell,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    rc::Rc,
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};
#[path = "common/version_metadata.rs"]
mod metadata_fault;
#[path = "common/movement_progress.rs"]
mod movement_progress;
use metadata_fault::{Fault, Metadata};
const TOPIC: &str = "topic|雪\0🚀";

fn progress_phase() {
    static PHASE: AtomicUsize = AtomicUsize::new(0);
    let phase = PHASE.fetch_add(1, Ordering::Relaxed) + 1;
    assert!(
        phase <= movement_progress::MAX_PHASES,
        "finite movement phase bound"
    );
    let directory = std::env::var("LINK_MOVEMENT_EXIT_DIRECTORY").unwrap();
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(Path::new(&directory).join("progress"))
        .unwrap();
    writeln!(file, "{phase}").unwrap();
}

fn data(directory: &Path) -> Result<NamedTypesDecorator> {
    NamedTypesDecorator::new(directory.join("data/data.links"), false)
}
fn log(directory: &Path) -> Result<NamedTypesDecorator> {
    NamedTypesDecorator::new(directory.join("log.links"), false)
}
fn branches(directory: &Path) -> Result<NamedTypesDecorator> {
    NamedTypesDecorator::new(directory.join("branches/branches.links"), false)
}
fn tx(directory: &Path) -> Result<TransactionsDecorator> {
    Ok(TransactionsDecorator::new(
        data(directory)?,
        log(directory)?,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )?)
}

fn verify(point: &str, switch: bool, recovery_fault: Option<&str>) -> Result<()> {
    let directory = tempfile::tempdir()?;
    std::fs::create_dir(directory.path().join("data"))?;
    std::fs::create_dir(directory.path().join("branches"))?;
    // A pipe left unread until exit can itself stall a verbose child. A file
    // retains progress on deadline failures without introducing that backpressure.
    let diagnostics_path = directory.path().join("child.log");
    let diagnostics = std::fs::File::create(&diagnostics_path)?;
    let mut child = Command::new(std::env::current_exe()?)
        .args(["--exact", "movement_exit_child", "--ignored", "--nocapture"])
        .env("LINK_MOVEMENT_EXIT_DIRECTORY", directory.path())
        .env("LINK_MOVEMENT_EXIT_POINT", point)
        .env("LINK_MOVEMENT_EXIT_SWITCH", switch.to_string())
        .stdout(Stdio::from(diagnostics.try_clone()?))
        .stderr(Stdio::from(diagnostics))
        .spawn()?;
    // Setup contains several separate mapped-file persistence barriers. Bound
    // each interval of observed finite progress, not their accumulated duration.
    // Repeated/partial heartbeats do not reset the same ten-second deadline.
    let status = movement_progress::wait(
        &mut child,
        &directory.path().join("progress"),
        Duration::from_secs(10),
    )
    .map_err(|cause| {
        anyhow::anyhow!(
            "Finite movement child failed: {point}/{switch}: {cause}.\n{}",
            std::fs::read_to_string(&diagnostics_path).unwrap_or_default()
        )
    })?;
    assert!(
        status.success(),
        "{point}/{switch}: {}",
        std::fs::read_to_string(&diagnostics_path)?
    );
    let manifest = std::fs::read_to_string(directory.path().join("manifest"))?;
    let fields: Vec<_> = manifest.split(',').collect();
    let id: u32 = fields[0].parse()?;
    let original: i64 = fields[1].parse()?;
    let attempted: i64 = fields[2].parse()?;
    if let Some(point) = recovery_fault {
        let fault = Rc::new(RefCell::new(Fault {
            point: Some(point.into()),
            flushes: 1,
            on_check: None,
        }));
        let outcome = GenericVersionControlDecorator::new(
            tx(directory.path())?,
            Metadata {
                inner: branches(directory.path())?,
                fault: fault.clone(),
                intent: 0,
            },
            true,
        );
        assert!(outcome
            .err()
            .expect("Recovery omitted its fault phase")
            .to_string()
            .contains(point));
        assert!(fault.borrow().point.is_none());
    }
    let selected = if switch { TOPIC } else { "main" };
    let mut following = 0;
    for reopen in 0..2 {
        let mut vc = VersionControlDecorator::new(
            tx(directory.path())?,
            branches(directory.path())?,
            false,
        )?;
        assert_eq!(vc.current_branch(), selected, "{point}/{switch}");
        assert_eq!(
            vc.current_sequence(),
            if reopen == 0 { original } else { following }
        );
        assert_eq!(
            vc.get(id).unwrap().source,
            if reopen == 0 { 11 } else { 31 }
        );
        assert_eq!(vc.get_name(id)?.as_deref(), Some("original 雪|\0\n"));
        assert_eq!(
            vc.transactions().last_logged_sequence(),
            if reopen == 0 { attempted } else { following }
        );
        if reopen == 0 {
            vc.switch_branch("main")?;
            vc.checkout(attempted)?;
            assert_eq!(vc.get(id).unwrap().source, 21);
            assert_eq!(vc.get_name(id)?.as_deref(), Some("changed 雪|\0\n"));
            if switch {
                vc.switch_branch(TOPIC)?;
            } else {
                vc.checkout(original)?;
            }
            vc.update(id, 31, 32)?;
            following = vc.current_sequence();
        }
        vc.save()?;
    }
    Ok(())
}

#[test]
fn checkout_survives_exit_at_every_persisted_movement_phase() -> Result<()> {
    for point in [
        "intent-flush-after",
        "current-before",
        "current-after",
        "applied-after",
        "publication-flush-after",
        "intent-remove-after",
        "intent-delete-after",
        "cleanup-flush-after",
    ] {
        verify(point, false, None)?;
    }
    Ok(())
}
#[test]
fn switch_survives_exit_at_every_persisted_movement_phase() -> Result<()> {
    for point in [
        "intent-flush-after",
        "current-before",
        "current-after",
        "applied-after",
        "publication-flush-after",
        "intent-remove-after",
        "intent-delete-after",
        "cleanup-flush-after",
    ] {
        verify(point, true, None)?;
    }
    Ok(())
}

#[test]
fn interrupted_recovery_reopens_at_every_metadata_phase() -> Result<()> {
    for phase in [
        "current",
        "applied",
        "publication-flush",
        "intent-remove",
        "intent-delete",
        "cleanup-flush",
    ] {
        for edge in ["before", "after"] {
            for switch in [false, true] {
                verify(
                    "intent-flush-after",
                    switch,
                    Some(&format!("{phase}-{edge}")),
                )?;
            }
        }
    }
    Ok(())
}
#[test]
#[ignore = "Executed only by bounded movement parents"]
fn movement_exit_child() -> Result<()> {
    let started = Instant::now();
    let progress = |stage| {
        eprintln!("movement child {:?}: {stage}", started.elapsed());
        progress_phase();
    };
    progress("opening stores");
    let directory = std::env::var("LINK_MOVEMENT_EXIT_DIRECTORY")?;
    let directory = Path::new(&directory);
    let fault = Rc::new(RefCell::new(Fault::default()));
    let mut vc = GenericVersionControlDecorator::new(
        tx(directory)?,
        Metadata {
            inner: branches(directory)?,
            fault: fault.clone(),
            intent: 0,
        },
        true,
    )?;
    progress("creating row");
    let id = vc.create_and_update(11, 12)?;
    progress("naming original row");
    vc.set_name(id, "original 雪|\0\n")?;
    let original = vc.current_sequence();
    progress("creating branch");
    vc.branch(TOPIC, None)?;
    progress("updating row");
    vc.update(id, 21, 22)?;
    progress("naming changed row");
    vc.set_name(id, "changed 雪|\0\n")?;
    let attempted = vc.current_sequence();
    progress("saving setup");
    vc.save()?;
    std::fs::write(
        directory.join("manifest"),
        format!("{id},{original},{attempted}"),
    )?;
    fault.borrow_mut().flushes = 0;
    fault.borrow_mut().on_check = Some(|point| {
        eprintln!("movement child metadata: {point}");
        progress_phase();
        if std::env::var("LINK_MOVEMENT_EXIT_POINT").as_deref() == Ok(point) {
            std::process::exit(0);
        }
    });
    progress("beginning requested movement");
    if std::env::var("LINK_MOVEMENT_EXIT_SWITCH")? == "true" {
        vc.switch_branch(TOPIC)?;
    } else {
        vc.checkout(original)?;
    }
    bail!("Movement did not reach its requested exit point.")
}
