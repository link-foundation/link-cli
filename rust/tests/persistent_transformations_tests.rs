//! Rust counterpart of
//! `csharp/Foundation.Data.Doublets.Cli.Tests/PersistentTransformationDecoratorTests.cs`.
//!
//! The two implementations share the on-disk trigger schema, so these tests
//! assert the same observable behaviour as the C# ones: an `Always` trigger
//! keeps firing, a `Once` trigger removes itself after it first changed
//! something, and `--never` removes matching triggers.

use anyhow::Result;
use link_cli::{
    make_triggers_database_filename, Link, NamedTypeLinks, NamedTypesDecorator,
    PersistentTransformationDecorator, PersistentTransformationKind, PersistentTransformationQuery,
    QueryProcessor, TriggerStore,
};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// `(1: 1 1)` becomes `(1: 1 2)`.
const REWRITE_TARGET: &str = "(((1: 1 1)) ((1: 1 2)))";
/// The inverse of [`REWRITE_TARGET`].
const RESTORE_TARGET: &str = "(((1: 1 2)) ((1: 1 1)))";
/// Creates `(1: 1 1)`.
const CREATE_SELF_LINK: &str = "(() ((1: 1 1)))";

fn database_path(directory: &TempDir) -> PathBuf {
    directory.path().join("db.links")
}

/// A decorator with its triggers in the conventional sidecar database.
fn sidecar_decorator(
    database: &Path,
) -> Result<PersistentTransformationDecorator<NamedTypesDecorator>> {
    let links = NamedTypesDecorator::new(database, false)?;
    let triggers = TriggerStore::sidecar(make_triggers_database_filename(database), false)?;
    Ok(
        PersistentTransformationDecorator::new(links, triggers, false)
            .with_auto_create_missing_references(true),
    )
}

fn process(
    decorator: &mut PersistentTransformationDecorator<NamedTypesDecorator>,
    query: &str,
) -> Result<()> {
    QueryProcessor::new(false)
        .with_auto_create_missing_references(true)
        .process_query(decorator, query)?;
    Ok(())
}

#[test]
fn always_trigger_is_stored_in_links_and_applied_after_write() -> Result<()> {
    let directory = TempDir::new()?;
    let mut decorator = sidecar_decorator(&database_path(&directory))?;

    let root = decorator.store_trigger(PersistentTransformationKind::Always, REWRITE_TARGET)?;
    assert_ne!(0, root);

    let stored = decorator.triggers()?;
    assert_eq!(1, stored.len());
    assert_eq!(PersistentTransformationKind::Always, stored[0].kind);
    assert_eq!("((1: 1 1))", stored[0].condition);
    assert_eq!("((1: 1 2))", stored[0].substitution);

    process(&mut decorator, CREATE_SELF_LINK)?;

    assert_eq!(Some(Link::new(1, 1, 2)), decorator.get_link(1));
    Ok(())
}

#[test]
fn once_trigger_deletes_itself_after_first_match() -> Result<()> {
    let directory = TempDir::new()?;
    let mut decorator = sidecar_decorator(&database_path(&directory))?;

    decorator.store_trigger(PersistentTransformationKind::Once, REWRITE_TARGET)?;
    process(&mut decorator, CREATE_SELF_LINK)?;

    assert!(
        decorator.triggers()?.is_empty(),
        "a Once trigger must remove itself once it produced changes"
    );

    // The trigger is gone, so restoring `(1: 1 1)` is no longer undone.
    process(&mut decorator, RESTORE_TARGET)?;
    assert_eq!(Some(Link::new(1, 1, 1)), decorator.get_link(1));
    Ok(())
}

#[test]
fn always_trigger_keeps_firing() -> Result<()> {
    let directory = TempDir::new()?;
    let mut decorator = sidecar_decorator(&database_path(&directory))?;

    decorator.store_trigger(PersistentTransformationKind::Always, REWRITE_TARGET)?;
    process(&mut decorator, CREATE_SELF_LINK)?;
    assert_eq!(Some(Link::new(1, 1, 2)), decorator.get_link(1));

    // Unlike a `Once` trigger, this one survives and undoes the restore.
    process(&mut decorator, RESTORE_TARGET)?;
    assert_eq!(Some(Link::new(1, 1, 2)), decorator.get_link(1));
    assert_eq!(1, decorator.triggers()?.len());
    Ok(())
}

#[test]
fn never_removes_matching_stored_trigger() -> Result<()> {
    let directory = TempDir::new()?;
    let mut decorator = sidecar_decorator(&database_path(&directory))?;

    decorator.store_trigger(PersistentTransformationKind::Always, REWRITE_TARGET)?;
    assert_eq!(1, decorator.remove_triggers(REWRITE_TARGET)?);
    assert!(decorator.triggers()?.is_empty());

    // Removing again is a no-op rather than an error.
    assert_eq!(0, decorator.remove_triggers(REWRITE_TARGET)?);
    Ok(())
}

#[test]
fn storing_the_same_trigger_twice_is_idempotent() -> Result<()> {
    let directory = TempDir::new()?;
    let mut decorator = sidecar_decorator(&database_path(&directory))?;

    let first = decorator.store_trigger(PersistentTransformationKind::Always, REWRITE_TARGET)?;
    // The bare spelling parses to the same condition and substitution.
    let second = decorator.store_trigger(
        PersistentTransformationKind::Always,
        "((1: 1 1)) ((1: 1 2))",
    )?;

    assert_eq!(first, second);
    assert_eq!(1, decorator.triggers()?.len());
    Ok(())
}

#[test]
fn sidecar_store_keeps_the_main_database_free_of_trigger_bookkeeping() -> Result<()> {
    let directory = TempDir::new()?;
    let database = database_path(&directory);
    let mut decorator = sidecar_decorator(&database)?;

    decorator.store_trigger(PersistentTransformationKind::Always, REWRITE_TARGET)?;
    assert!(
        decorator.all_links().is_empty(),
        "trigger bookkeeping must not leak into the decorated database"
    );

    decorator.save()?;
    assert!(make_triggers_database_filename(&database).exists());
    Ok(())
}

#[test]
fn embedded_store_keeps_triggers_in_the_decorated_database() -> Result<()> {
    let directory = TempDir::new()?;
    let database = database_path(&directory);
    let links = NamedTypesDecorator::new(&database, false)?;
    let mut decorator = PersistentTransformationDecorator::embedded(links, false)
        .with_auto_create_missing_references(true);

    let root = decorator.store_trigger(PersistentTransformationKind::Always, REWRITE_TARGET)?;
    assert!(decorator.exists(root));
    assert_eq!(1, decorator.triggers()?.len());

    decorator.save()?;
    assert!(
        !make_triggers_database_filename(&database).exists(),
        "--embed-triggers must not create a sidecar database"
    );
    Ok(())
}

#[test]
fn triggers_survive_a_reopen() -> Result<()> {
    let directory = TempDir::new()?;
    let database = database_path(&directory);

    let mut decorator = sidecar_decorator(&database)?;
    decorator.store_trigger(PersistentTransformationKind::Always, REWRITE_TARGET)?;
    decorator.save()?;
    drop(decorator);

    let mut reopened = sidecar_decorator(&database)?;
    let stored = reopened.triggers()?;
    assert_eq!(1, stored.len());
    assert_eq!(REWRITE_TARGET, stored[0].query());
    Ok(())
}

#[test]
fn query_parsing_accepts_the_wrapped_and_the_bare_form() -> Result<()> {
    let wrapped = PersistentTransformationQuery::parse(REWRITE_TARGET)?;
    let bare = PersistentTransformationQuery::parse("((1: 1 1)) ((1: 1 2))")?;

    assert_eq!(wrapped, bare);
    assert_eq!("((1: 1 1))", wrapped.condition);
    assert_eq!("((1: 1 2))", wrapped.substitution);
    assert_eq!(REWRITE_TARGET, wrapped.query());
    Ok(())
}

#[test]
fn query_parsing_rejects_an_incomplete_query() {
    let error = PersistentTransformationQuery::parse("((1: 1 1))")
        .expect_err("a query without a substitution must be rejected");
    assert!(
        format!("{error}").contains("condition and a substitution"),
        "unexpected error: {error}"
    );
}

#[test]
fn triggers_database_filename_follows_the_names_database_convention() {
    assert_eq!(
        PathBuf::from("/tmp/example.triggers.links"),
        make_triggers_database_filename("/tmp/example.links")
    );
    assert_eq!(
        PathBuf::from("example.triggers.links"),
        make_triggers_database_filename("example.links")
    );
}
