//! Finite real-process exits skip all version-owner and storage destructors.
use anyhow::{bail, Result};
use link_cli::{
    CommitMode, LogRetentionPolicy, NamedTypeLinks, NamedTypesDecorator, TransactionsDecorator,
    VersionControlDecorator,
};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const TOPIC: &str = "topic|:parent=\u{96ea}\0";
fn open(directory: &Path) -> Result<VersionControlDecorator> {
    VersionControlDecorator::new(
        TransactionsDecorator::new(
            NamedTypesDecorator::new(directory.join("data/data.links"), false)?,
            NamedTypesDecorator::new(directory.join("log.links"), false)?,
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            false,
        )?,
        NamedTypesDecorator::new(directory.join("branches.links"), false)?,
        false,
    )
}
fn verify(mode: &str, automatic: bool, names: bool) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let mut child = Command::new(std::env::current_exe()?)
        .args(["--exact", "version_exit_child", "--ignored", "--nocapture"])
        .env("LINK_VERSION_EXIT_DIRECTORY", directory.path())
        .env("LINK_VERSION_EXIT_MODE", mode)
        .env("LINK_VERSION_EXIT_AUTOMATIC", automatic.to_string())
        .env("LINK_VERSION_EXIT_NAMES", names.to_string())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            child.kill()?;
            child.wait()?;
            bail!("finite version-exit child did not finish: {mode}/{automatic}");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    assert!(
        status.success(),
        "version-exit child failed at {mode}/{automatic}"
    );
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
    std::fs::create_dir(directory.join("data"))?;
    let mut vc = open(directory)?;
    let id = vc.create_and_update(11, 12)?;
    vc.set_name(id, "original")?;
    let original = vc.current_sequence();
    vc.branch(TOPIC, None)?;
    vc.switch_branch(TOPIC)?;
    vc.save()?;
    let mode = std::env::var("LINK_VERSION_EXIT_MODE")?;
    let automatic = std::env::var("LINK_VERSION_EXIT_AUTOMATIC")? == "true";
    let names = std::env::var("LINK_VERSION_EXIT_NAMES")? == "true";
    if !automatic {
        vc.begin_transaction()?;
    }
    if mode == "checkpoint" {
        std::fs::rename(directory.join("data"), directory.join("moved"))?;
    }
    let result = if names {
        vc.set_name(id, "changed 雪|\0\n").map(|_| ())
    } else {
        vc.update(id, 21, 22).map(|_| ())
    };
    let outcome = if automatic {
        result.map(|_| ())
    } else {
        result?;
        if mode == "pending" {
            Ok(())
        } else if mode == "rollback" {
            vc.rollback()
        } else {
            vc.commit()
        }
    };
    if mode == "checkpoint" {
        // Restore all paths before exiting. The error comes from the actual
        // checkpoint and no destructor/cleanup is used to finish publication.
        std::fs::rename(directory.join("moved"), directory.join("data"))?;
        assert!(outcome.is_err());
    } else {
        outcome?;
        if mode == "pending" || mode == "rollback" {
            vc.save()?;
        }
    }
    std::fs::write(
        directory.join("manifest"),
        format!(
            "{id},{original},{}",
            vc.transactions().last_logged_sequence()
        ),
    )?;
    std::process::exit(0);
}
