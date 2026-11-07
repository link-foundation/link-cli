//! Edges of the public surface that embedders use and the rest of the suite
//! reaches only through its own helpers: the row formats of `LinkStorage`, the
//! raw doublets query shapes, what `TypesDecorator` passes to its store, the
//! accessors of `DoubletsStorage`, `FileLock` and `LinksSessions`, and the
//! `--flag=value` spelling of every CLI option.

use std::collections::HashSet;

use anyhow::Result;
use doublets::data::Flow;
use doublets::Links;
use link_cli::cli::{Cli, CliCommand};
use link_cli::protocol::LinksSessions;
use link_cli::{
    ensure_point, lock_file_path, DoubletsStorage, FileLock, Link, LinkStorage, LinksStorage, LockMode,
    NamedTypeLinks, TypesDecorator,
};
use tempfile::TempDir;

fn run(args: &[&str]) -> Cli {
    match Cli::parse_from(args).expect("arguments parse") {
        CliCommand::Run(cli) => *cli,
        other => panic!("expected a run, got {other:?}"),
    }
}

#[test]
fn a_row_is_formatted_with_its_names_where_it_has_them() {
    let mut storage = LinkStorage::in_memory(false);
    let first = storage.create(0, 0);
    let second = storage.create(first, 7);
    storage.set_name(first, "one");
    let named = *storage.get(first).unwrap();
    assert_eq!(storage.format(&Link::new(second, first, 7)), "(2 one 7)");
    assert_eq!(storage.format(&named), "(one 0 0)");
    assert!(!storage.is_trace_enabled());
    assert!(LinkStorage::in_memory(true).is_trace_enabled());
    // Printing goes to stdout; it must accept every shape of a change.
    storage.print_all_links();
    storage.print_change(&None, &None);
    storage.print_change(&Some(named), &None);
    storage.print_change(&None, &Some(named));
}

#[test]
fn writes_to_an_absent_row_are_refused_and_nothing_is_kept() -> Result<()> {
    let mut storage = LinkStorage::in_memory(false);
    assert_eq!(storage.ensure_created(0), 0);
    assert!(storage.update_raw(9, 1, 1).is_err());
    assert!(storage.delete(9).is_err());
    assert!(storage.all().is_empty());
    let made = storage.create(0, 0);
    assert_eq!(storage.ensure_created(made), made);
    Ok(())
}

#[test]
fn raw_queries_of_every_shape_answer_as_the_store_does() {
    let mut storage = LinkStorage::in_memory(false);
    let any = storage.constants().any;
    let first = storage.create(5, 6);
    let second = storage.create(5, 7);
    let count = |storage: &LinkStorage, query: &[u32]| Links::count_links(storage, query);
    assert_eq!(count(&storage, &[]), 2);
    assert_eq!(count(&storage, &[any]), 2);
    assert_eq!(count(&storage, &[first]), 1);
    assert_eq!(count(&storage, &[99]), 0);
    assert_eq!(count(&storage, &[any, any]), 2);
    assert_eq!(count(&storage, &[any, 5]), 2);
    assert_eq!(count(&storage, &[first, any]), 1);
    assert_eq!(count(&storage, &[first, 5]), 1);
    assert_eq!(count(&storage, &[first, 6]), 1);
    assert_eq!(count(&storage, &[first, 7]), 0);
    assert_eq!(count(&storage, &[any, 5, 6]), 1);
    assert_eq!(count(&storage, &[second, any, any]), 1);
    assert_eq!(count(&storage, &[second, 5, 7]), 1);
    assert_eq!(count(&storage, &[second, 6, any]), 0);
    assert_eq!(count(&storage, &[second, any, 6]), 0);
    assert_eq!(count(&storage, &[99, any, any]), 0);
    // Shapes the raw interface does not define match nothing.
    assert_eq!(count(&storage, &[any, any, any, any]), 0);
}

#[test]
fn a_raw_create_reports_a_blank_link_directly_and_through_a_reference() {
    let mut storage = LinkStorage::in_memory(false);
    let mut seen = Vec::new();
    Links::create_links(&mut storage, &[], &mut |before, after| {
        seen.push((before.index, after.index, after.source, after.target));
        Flow::Continue
    })
    .unwrap();
    assert_eq!(seen, [(0, 1, 0, 0)]);
    let mut borrowed = &mut storage;
    let flow = Links::create_links(&mut borrowed, &[], &mut |_, _| Flow::Break).unwrap();
    assert_eq!(flow, Flow::Break);
    assert_eq!(storage.all().len(), 2);
}

#[test]
fn a_type_store_passes_every_write_to_the_store_below() -> Result<()> {
    let mut types = TypesDecorator::with_named_markers(LinkStorage::in_memory(false))?;
    let base = types.links().all().len() as u32;
    let made = NamedTypeLinks::create(&mut types, 0, 0);
    assert_eq!(made, base + 1);
    assert_eq!(NamedTypeLinks::ensure_created(&mut types, made), made);
    assert_eq!(types.try_ensure_created(made)?, made);
    assert!(types.reference_exists(made));
    assert!(!types.reference_exists(made + 40));
    let next = types.next_created_addresses(2)?;
    assert_eq!(next.len(), 2);
    let pair = NamedTypeLinks::get_or_create(&mut types, made, made);
    assert_eq!(NamedTypeLinks::get_or_create(&mut types, made, made), pair);
    types.set_name(pair, "pair")?;
    assert_eq!(types.get_name(pair)?.as_deref(), Some("pair"));
    types.remove_name(pair)?;
    assert_eq!(types.get_name(pair)?, None);
    let named = types.get_or_create_named("named")?;
    assert_eq!(types.get_or_create_named("named")?, named);
    let mut observed = Vec::new();
    let removed = types.delete_observed(named, &mut |before, after| {
        observed.push((before.index, after.index));
    })?;
    assert_eq!(removed.index, named);
    assert!(observed.iter().any(|(before, after)| *before == named && *after == 0));
    Ok(())
}

#[test]
fn a_point_named_after_a_removed_link_is_made_again() -> Result<()> {
    let mut storage = LinkStorage::in_memory(false);
    let point = ensure_point(&mut storage, "Thing")?;
    assert_eq!(ensure_point(&mut storage, "Thing")?, point);
    storage.update_raw(point, 0, 0)?;
    NamedTypeLinks::delete(&mut storage, point)?;
    let again = ensure_point(&mut storage, "Thing")?;
    assert!(NamedTypeLinks::exists(&mut storage, again));
    assert_eq!(storage.get_by_name("Thing"), Some(again));
    Ok(())
}

#[test]
fn a_doublets_store_makes_its_directory_and_offers_its_locks() -> Result<()> {
    let dir = TempDir::new()?;
    let path = dir.path().join("a/b/links.doublets");
    {
        let mut storage = DoubletsStorage::<u32, _>::open(&path)?;
        storage.create_link(0, 0)?;
        let _ = storage.store();
        let _ = storage.store_mut();
        let exclusive = storage.lock_exclusive()?;
        assert_eq!(exclusive.mode(), LockMode::Exclusive);
        assert_eq!(exclusive.path(), lock_file_path(&path));
        drop(exclusive);
        assert_eq!(storage.lock_shared()?.mode(), LockMode::Shared);
    }
    let shared = DoubletsStorage::<u32, _>::open_shared(&path)?;
    assert_eq!(shared.lock_shared()?.mode(), LockMode::Shared);
    Ok(())
}

#[test]
fn a_lock_makes_its_directory_and_a_bare_name_stays_relative() -> Result<()> {
    assert_eq!(
        lock_file_path("links.db"),
        std::path::PathBuf::from("links.db.lock")
    );
    let dir = TempDir::new()?;
    let path = dir.path().join("x/y/z.lock");
    let lock = FileLock::acquire(&path, LockMode::Shared)?;
    assert_eq!(lock.path(), path);
    Ok(())
}

#[test]
fn sessions_answer_about_themselves() -> Result<()> {
    let mut plain = LinksSessions::<LinkStorage>::new(LinkStorage::in_memory(false));
    assert!(!plain.has_permissions());
    assert!(plain.policy().is_none());
    assert!(!plain.has_triplets());
    let plain_session = plain.open(None);
    assert_eq!(plain.created(plain_session), Some(&HashSet::new()));
    assert_eq!(plain.into_inner().all().len(), 0);

    let triplets = LinksSessions::<LinkStorage>::new(LinkStorage::in_memory(false)).with_triplets()?;
    assert!(triplets.has_triplets());
    let mut triplets = triplets;
    let first = triplets.ensure_triplets()?;
    assert_eq!(triplets.ensure_triplets()?, first);
    Ok(())
}

#[test]
fn every_option_has_an_inline_spelling() {
    let cli = run(&[
        "clink",
        "--jwt-secret=s3cret",
        "--graphql-admin-secret=admin",
        "--deep-package=a.json",
        "--permissions=rules.links",
        "--tokens=tokens.links",
        "--revoke-token=t",
    ]);
    assert_eq!(cli.jwt_secret.as_deref(), Some("s3cret"));
    assert_eq!(cli.graphql_admin_secret.as_deref(), Some("admin"));
    assert_eq!(cli.deep_packages, ["a.json"]);
    assert_eq!(cli.permissions.as_deref(), Some("rules.links"));
    assert_eq!(cli.tokens.as_deref(), Some("tokens.links"));
    assert_eq!(cli.revoke_token.as_deref(), Some("t"));
}

#[test]
fn the_graphql_options_take_the_next_argument() {
    let cli = run(&[
        "clink",
        "--graphql",
        "--jwt-secret",
        "s3cret",
        "--graphql-admin-secret",
        "admin",
        "--deep-package",
        "a.json",
        "--deep-package",
        "b.json",
    ]);
    assert!(cli.graphql);
    assert_eq!(cli.jwt_secret.as_deref(), Some("s3cret"));
    assert_eq!(cli.graphql_admin_secret.as_deref(), Some("admin"));
    assert_eq!(cli.deep_packages, ["a.json", "b.json"]);
}
