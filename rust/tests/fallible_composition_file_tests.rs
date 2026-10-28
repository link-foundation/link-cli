//! Result-returning public calls must preserve journal I/O errors through layers.
use anyhow::Result;
use link_cli::{
    CommitMode, Link, LogRetentionPolicy, NamedTypeLinks, NamedTypesDecorator,
    PersistentTransformationDecorator, PersistentTransformationKind, QueryProcessor,
    TransactionsDecorator, VersionControlDecorator,
};
use std::path::Path;
#[path = "common/import_operation.rs"]
mod import_operation;

fn transaction(directory: &Path) -> Result<TransactionsDecorator> {
    Ok(TransactionsDecorator::new(
        NamedTypesDecorator::new(directory.join("data"), false)?,
        NamedTypesDecorator::new(directory.join("log"), false)?,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )?)
}

fn blocked<L: NamedTypeLinks>(
    mut links: L,
    directory: &Path,
    operation: &str,
) -> Result<(Vec<Link>, u32)> {
    if operation.starts_with("import-") {
        assert_eq!(links.try_create(1, 1)?, 1);
        links.set_name(1, "seed 雪")?;
    }
    // Pre-existing schema points force the error into relationship allocation,
    // rather than the already fallible named-point helper.
    if operation == "schema" {
        for name in [
            "Type",
            "Trigger",
            "Once",
            "Always",
            "Condition",
            "Substitution",
        ] {
            links.get_or_create_named(name)?;
        }
    }
    std::fs::remove_file(directory.join("log"))?;
    std::fs::create_dir(directory.join("log"))?;
    let mut triggers = PersistentTransformationDecorator::embedded(links, false);
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> Result<()> {
        if operation == "planner" {
            QueryProcessor::new(false).process_query(triggers.inner_mut(), "(() ((0 0)))")?;
        } else {
            match operation {
                "create" => {
                    triggers.try_create(0, 0)?;
                }
                "ensure" => {
                    triggers.try_ensure_created(8)?;
                }
                "query" => {
                    QueryProcessor::new(false).process_query(&mut triggers, "(() ((8: 8 8)))")?;
                }
                "schema" => {
                    triggers.store_trigger(PersistentTransformationKind::Always, "(() ())")?;
                }
                name if name.starts_with("import-") => import_operation::run(&mut triggers, name)?,
                _ => unreachable!(),
            }
        }
        Ok(())
    }));
    assert!(
        matches!(result, Ok(Err(_))),
        "{operation} must return its journal error, got {result:?}"
    );
    // Repair only this fixture's blocked journal path. Cleanup resolves the
    // original owner before any following write; an error is not an undo claim.
    std::fs::remove_dir(directory.join("log"))?;
    triggers.save()?;
    if operation.starts_with("import-") {
        import_operation::run(&mut triggers, operation)?;
        let before = triggers.all_links();
        import_operation::run(&mut triggers, operation)?;
        assert_eq!(triggers.all_links(), before);
        assert_eq!(triggers.get_link(1), Some(Link::new(1, 1, 1)));
        assert_eq!(triggers.get_name(1)?.as_deref(), Some("seed 雪"));
        if operation == "import-archive" {
            assert_eq!(triggers.get_name(8)?.as_deref(), Some("imported 雪"));
        }
    }
    if operation == "schema" {
        let root = triggers.store_trigger(PersistentTransformationKind::Always, "(() ())")?;
        assert_eq!(
            triggers.store_trigger(PersistentTransformationKind::Always, "() ()")?,
            root
        );
        assert_eq!(triggers.remove_triggers("(() ())")?, 1);
    }
    let following = triggers.try_create(0, 0)?;
    triggers.set_name(following, "following 雪=\0")?;
    triggers.save()?;
    let mut rows = triggers.all_links();
    rows.sort_by_key(|row| row.index);
    Ok((rows, following))
}

fn reopened<L: NamedTypeLinks>(
    mut links: L,
    rows: &[Link],
    following: u32,
    operation: &str,
) -> Result<()> {
    let mut actual = links.all_links();
    actual.sort_by_key(|row| row.index);
    assert_eq!(actual, rows);
    assert_eq!(
        links.get_name(following)?.as_deref(),
        Some("following 雪=\0")
    );
    if operation.starts_with("import-") {
        assert_eq!(links.get_name(1)?.as_deref(), Some("seed 雪"));
        if operation == "import-named" {
            let id = links.get_by_name("imported 雪")?.expect("imported name");
            assert_eq!(links.get_link(id), Some(Link::new(id, 1, 0)));
        } else if operation == "import-archive" {
            assert_eq!(links.get_name(8)?.as_deref(), Some("imported 雪"));
        }
    }
    links.save()
}

fn check(versioned: bool, operation: &str) -> Result<()> {
    let directory = tempfile::tempdir()?;
    let tx = transaction(directory.path())?;
    if versioned {
        let version = VersionControlDecorator::new(
            tx,
            NamedTypesDecorator::new(directory.path().join("branches"), false)?,
            false,
        )?;
        let (rows, following) = blocked(version, directory.path(), operation)?;
        for _ in 0..2 {
            let version = VersionControlDecorator::new(
                transaction(directory.path())?,
                NamedTypesDecorator::new(directory.path().join("branches"), false)?,
                false,
            )?;
            reopened(version, &rows, following, operation)?;
        }
    } else {
        let (rows, following) = blocked(tx, directory.path(), operation)?;
        for _ in 0..2 {
            reopened(transaction(directory.path())?, &rows, following, operation)?;
        }
    }
    Ok(())
}

macro_rules! case {
    ($name:ident, $version:expr, $operation:literal) => {
        #[test]
        fn $name() -> Result<()> {
            check($version, $operation)
        }
    };
}
case!(trigger_transaction_create_returns_io_error, false, "create");
case!(trigger_version_create_returns_io_error, true, "create");
case!(trigger_transaction_ensure_returns_io_error, false, "ensure");
case!(trigger_version_ensure_returns_io_error, true, "ensure");
case!(
    planner_transaction_allocation_returns_io_error,
    false,
    "planner"
);
case!(planner_version_allocation_returns_io_error, true, "planner");
case!(trigger_transaction_query_returns_io_error, false, "query");
case!(trigger_version_query_returns_io_error, true, "query");
case!(trigger_transaction_schema_returns_io_error, false, "schema");
case!(trigger_version_schema_returns_io_error, true, "schema");
case!(
    import_transaction_explicit_returns_io_error,
    false,
    "import-explicit"
);
case!(
    import_version_explicit_returns_io_error,
    true,
    "import-explicit"
);
case!(
    import_transaction_nested_returns_io_error,
    false,
    "import-nested"
);
case!(
    import_version_nested_returns_io_error,
    true,
    "import-nested"
);
case!(
    import_transaction_archive_returns_io_error,
    false,
    "import-archive"
);
case!(
    import_version_archive_returns_io_error,
    true,
    "import-archive"
);
case!(import_transaction_named_control, false, "import-named");
case!(import_version_named_control, true, "import-named");
