//! Finite real-process exits skip all version-owner and storage destructors.
use anyhow::Result;
use link_cli::{
    CommitMode, LogRetentionPolicy, NamedTypeLinks, NamedTypesDecorator, TransactionsDecorator,
    VersionControlDecorator,
};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
#[path = "common/movement_progress.rs"]
mod movement_progress;

const TOPIC: &str = "topic|:parent=\u{96ea}\0";
fn open(directory: &Path) -> Result<VersionControlDecorator> {
    let trace = std::env::var_os("LINK_VERSION_EXIT_DIRECTORY").is_some();
    VersionControlDecorator::new(
        TransactionsDecorator::new(
            NamedTypesDecorator::new(directory.join("data/data.links"), false)?,
            NamedTypesDecorator::new(directory.join("log.links"), false)?,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            trace,
        )?,
        NamedTypesDecorator::new(directory.join("branches.links"), false)?,
        trace,
    )
}
fn verify(mode: &str, automatic: bool, names: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    // Preserve both streams without an unread pipe or a shared runner output
    // handle. Include progress on failure while keeping the existing deadline.
    let diagnostics_path = directory.path().join("child.log");
    let diagnostics = std::fs::File::create(&diagnostics_path)?;
    let mut child = Command::new(std::env::current_exe()?)
        .args(["--exact", "version_exit_child", "--ignored", "--nocapture"])
        .env("LINK_VERSION_EXIT_DIRECTORY", directory.path())
        .env("LINK_VERSION_EXIT_MODE", mode)
        .env("LINK_VERSION_EXIT_AUTOMATIC", automatic.to_string())
        .env("LINK_VERSION_EXIT_NAMES", names.to_string())
        .stdout(Stdio::from(diagnostics.try_clone()?))
        .stderr(Stdio::from(diagnostics))
        .spawn()?;
    // Several separate persistence barriers can exceed ten seconds in total
    // while each operation still progresses. Reuse the finite movement watch:
    // unchanged ten-second idle bound, strict monotonic phases, kill and reap.
    let status = movement_progress::wait(
        &mut child,
        &directory.path().join("progress"),
        Duration::from_secs(10),
    )
    .map_err(|cause| {
        anyhow::anyhow!(
            "finite version-exit child failed: {mode}/{automatic}/{names}: {cause}.\n{}",
            std::fs::read_to_string(&diagnostics_path).unwrap_or_default()
        )
    })?;
    assert!(
        status.success(),
        "version-exit child failed at {mode}/{automatic}/{names}: {}",
        std::fs::read_to_string(&diagnostics_path)?
    );
    if std::env::var_os("LINK_VERSION_EXIT_SHOW_DIAGNOSTICS").is_some() {
        eprintln!(
            "{mode}/{automatic}/{names}: {}",
            std::fs::read_to_string(&diagnostics_path)?
        );
    }
    let manifest = std::fs::read_to_string(directory.path().join("manifest"))?;
    let fields: Vec<_> = manifest.split(',').collect();
    let id: u32 = fields[0].parse()?;
    let original: i64 = fields[1].parse()?;
    let attempted: i64 = fields[2].parse()?;
    let committed = matches!(mode, "success" | "checkpoint");
    let expected = if committed { attempted } else { original };
    let mut vc = open(directory.path())?;
    assert_eq!(vc.current_branch(), TOPIC);
    assert_eq!(
        vc.get(id).unwrap().source,
        if committed && !names { 21 } else { 11 }
    );
    assert_eq!(
        vc.get_name(id)?.as_deref(),
        Some(if committed && names {
            "changed 雪|\0\n"
        } else {
            "original"
        })
    );
    assert_eq!(
        vc.current_sequence(),
        expected,
        "lost {mode}/{automatic} publication"
    );
    assert_eq!(
        vc.list_branches()
            .iter()
            .find(|b| b.name == TOPIC)
            .unwrap()
            .head,
        expected
    );
    vc.checkout(original)?;
    assert_eq!(vc.get(id).unwrap().source, 11);
    assert_eq!(vc.get_name(id)?.as_deref(), Some("original"));
    if committed {
        vc.checkout(attempted)?;
        assert_eq!(vc.get(id).unwrap().source, if names { 11 } else { 21 });
        assert_eq!(
            vc.get_name(id)?.as_deref(),
            Some(if names {
                "changed 雪|\0\n"
            } else {
                "original"
            })
        );
    } else {
        assert!(vc.checkout(attempted).is_err());
    }
    vc.update(id, 31, 32)?;
    let following = vc.current_sequence();
    vc.save()?;
    drop(vc);
    let mut vc = open(directory.path())?;
    assert_eq!(vc.current_sequence(), following);
    assert_eq!(vc.get(id).unwrap().source, 31);
    assert_eq!(
        vc.get_name(id)?.as_deref(),
        Some(if committed && names {
            "changed 雪|\0\n"
        } else {
            "original"
        })
    );
    Ok(())
}
macro_rules! contract {
    ($name:ident, $mode:literal, $automatic:literal) => {
        #[test]
        fn $name() -> Result<()> {
            verify($mode, $automatic, false)
        }
    };
}
contract!(successful_explicit_commit_survives_exit, "success", false);
contract!(successful_automatic_commit_survives_exit, "success", true);
contract!(
    explicit_commit_checkpoint_error_survives_exit,
    "checkpoint",
    false
);
contract!(
    automatic_commit_checkpoint_error_survives_exit,
    "checkpoint",
    true
);
contract!(
    pending_exit_does_not_publish_an_aborted_version,
    "pending",
    false
);
contract!(
    rollback_exit_does_not_publish_an_aborted_version,
    "rollback",
    false
);

#[test]
fn abrupt_name_only_versions_recover_exact_names_and_attribution() -> Result<()> {
    for (mode, automatic) in [
        ("success", false),
        ("success", true),
        ("checkpoint", false),
        ("checkpoint", true),
        ("pending", false),
        ("rollback", false),
    ] {
        verify(mode, automatic, true)?;
    }
    Ok(())
}

#[test]
#[ignore = "finite child process entry point"]
fn version_exit_child() -> Result<()> {
    let Some(directory) = std::env::var_os("LINK_VERSION_EXIT_DIRECTORY") else {
        return Ok(());
    };
    let directory = Path::new(&directory);
    let started = Instant::now();
    let mut count = 0;
    let mut phase = |name: &str| {
        use std::io::Write;
        count += 1;
        assert!(
            count <= movement_progress::MAX_PHASES,
            "finite version phase bound"
        );
        let mut progress = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(directory.join("progress"))
            .unwrap();
        writeln!(progress, "{count}").unwrap();
        eprintln!("version-exit +{:?}: {name}", started.elapsed());
    };
    phase("create directory");
    std::fs::create_dir(directory.join("data"))?;
    phase("open");
    let mut vc = open(directory)?;
    phase("create row");
    let id = vc.create_and_update(11, 12)?;
    phase("set original name");
    vc.set_name(id, "original")?;
    let original = vc.current_sequence();
    phase("create branch");
    vc.branch(TOPIC, None)?;
    phase("switch branch");
    vc.switch_branch(TOPIC)?;
    phase("save seed");
    vc.save()?;
    phase("seed ready");
    let mode = std::env::var("LINK_VERSION_EXIT_MODE")?;
    let automatic = std::env::var("LINK_VERSION_EXIT_AUTOMATIC")? == "true";
    let names = std::env::var("LINK_VERSION_EXIT_NAMES")? == "true";
    if !automatic {
        phase("begin explicit");
        vc.begin_transaction()?;
    }
    if mode == "checkpoint" {
        phase("move checkpoint path");
        std::fs::rename(directory.join("data"), directory.join("moved"))?;
    }
    phase("attempt change");
    let result = if names {
        vc.set_name(id, "changed 雪|\0\n").map(|_| ())
    } else {
        vc.update(id, 21, 22).map(|_| ())
    };
    phase("change returned");
    let outcome = if automatic {
        result.map(|_| ())
    } else {
        result?;
        if mode == "pending" {
            Ok(())
        } else if mode == "rollback" {
            phase("rollback");
            vc.rollback()
        } else {
            phase("commit");
            vc.commit()
        }
    };
    phase("decision returned");
    if mode == "checkpoint" {
        // Restore all paths before exiting. The error comes from the actual
        // checkpoint and no destructor/cleanup is used to finish publication.
        std::fs::rename(directory.join("moved"), directory.join("data"))?;
        assert!(outcome.is_err());
    } else {
        outcome?;
        if mode == "pending" || mode == "rollback" {
            phase("save pending or rollback");
            vc.save()?;
        }
    }
    phase("write manifest");
    std::fs::write(
        directory.join("manifest"),
        format!(
            "{id},{original},{}",
            vc.transactions().last_logged_sequence()
        ),
    )?;
    phase("exit without destructors");
    std::process::exit(0);
}
