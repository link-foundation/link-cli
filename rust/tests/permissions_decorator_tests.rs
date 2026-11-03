//! Permissions stored in links (issue #149). The cases are the "permissions"
//! tests of `@link-foundation/ui-links` plus what a store with cascades,
//! merges and names adds; the C# twin is
//! `csharp/Foundation.Data.Doublets.Cli.Tests/PermissionsDecoratorTests.cs`,
//! case for case.

use anyhow::Result;
use link_cli::{
    AccessPolicy, Forbidden, Link, NamedTypeLinks, PermissionAccess, PermissionMarkers,
    PermissionPolicy, PermissionsDecorator,
};

#[path = "common/permissions_world.rs"]
mod world;

use world::{all, World};

fn error_of<T>(result: Result<T>) -> Option<&'static str> {
    let error = result.err()?;
    if error.downcast_ref::<Forbidden>().is_some() {
        Some("forbidden")
    } else if error.to_string().starts_with("Link not found") {
        Some("not-found")
    } else {
        Some("bad-request")
    }
}

fn indexes(links: &mut dyn NamedTypeLinks) -> Vec<u32> {
    all(links).into_iter().map(|link| link.index).collect()
}

/// A policy of the application's own: anyone reads, no one writes.
struct ReadOnly(PermissionMarkers);

impl AccessPolicy for ReadOnly {
    fn markers(&self) -> PermissionMarkers {
        self.0
    }

    fn can_read(&mut self, _: &mut dyn NamedTypeLinks, _: u32, _: &Link) -> bool {
        true
    }

    fn can_write(&mut self, _: &mut dyn NamedTypeLinks, _: u32, _: &Link) -> bool {
        false
    }

    fn can_create(&mut self, _: &mut dyn NamedTypeLinks, _: u32) -> bool {
        false
    }
}

#[test]
fn implements_named_type_links() -> Result<()> {
    fn takes(links: &mut dyn NamedTypeLinks) -> usize {
        links.all_links().len()
    }
    let mut w = World::new(false)?;
    let alice = w.v["alice"];
    let mut view = w.as_(alice);
    assert_eq!(3, takes(&mut view));
    // Any policy will do, the stored one or an application's own.
    let markers = w.markers;
    let total = w.links.all_links().len();
    let mut read_only = PermissionsDecorator::new(&mut w.links, ReadOnly(markers), 0);
    assert_eq!(total, takes(&mut read_only));
    assert_eq!(Some("forbidden"), error_of(read_only.try_create(0, 0)));
    Ok(())
}

#[test]
fn markers_are_named_points() -> Result<()> {
    let mut w = World::new(false)?;
    let m = w.markers;
    for (marker, name) in [
        (m.allow_read, "AllowRead"),
        (m.allow_write, "AllowWrite"),
        (m.anything, "Anything"),
    ] {
        assert_eq!(Link::new(marker, marker, marker), w.link(marker));
        assert_eq!(
            Some(marker),
            NamedTypeLinks::get_by_name(&mut w.links, name)?
        );
    }
    assert_eq!(m, PermissionMarkers::ensure(&mut w.links)?);
    Ok(())
}

fn answers_by_type(separate_rules: bool) -> Result<()> {
    let mut w = World::new(separate_rules)?;
    let (alice, boss, note, shared, secret) = (
        w.v["alice"],
        w.v["boss"],
        w.v["note"],
        w.v["shared"],
        w.v["secret"],
    );
    let (note, shared, secret) = (w.link(note), w.link(shared), w.link(secret));
    let links = &mut w.links;
    let policy = &mut w.policy;
    assert!(policy.can_write(links, alice, &note));
    assert!(policy.can_read(links, alice, &shared));
    assert!(!policy.can_read(links, alice, &secret));
    assert!(policy.can_write(links, boss, &secret));
    assert!(!policy.can_read(links, 0, &note));
    assert!(policy.can_create(links, alice));
    assert!(!policy.can_create(links, 0));
    if separate_rules {
        // Three rules, each a pair and its mark.
        assert_eq!(6, w.rules().all_links().len());
    }
    Ok(())
}

#[test]
fn answers_by_type_rules_in_data() -> Result<()> {
    answers_by_type(false)
}

#[test]
fn answers_by_type_rules_apart() -> Result<()> {
    answers_by_type(true)
}

fn shows_a_subject_only_what_it_may_read(separate_rules: bool) -> Result<()> {
    let mut w = World::new(separate_rules)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let mut view = w.as_(v["alice"]);
    // The sub-type Shared is of type Note; its Extends pair is not.
    assert_eq!(
        vec![t["Shared"], v["note"], v["shared"]],
        indexes(&mut view)
    );
    assert_eq!(3, view.query_links(None, None, None).len());
    assert!(!view.exists(v["secret"]));
    assert!(view.exists(v["note"]));
    assert_eq!(Some("not-found"), error_of(view.delete(v["secret"])));
    assert_eq!(
        Some("not-found"),
        error_of(view.update(v["secret"], t["Note"], v["secret"]))
    );
    Ok(())
}

#[test]
fn shows_a_subject_only_what_it_may_read_rules_in_data() -> Result<()> {
    shows_a_subject_only_what_it_may_read(false)
}

#[test]
fn shows_a_subject_only_what_it_may_read_rules_apart() -> Result<()> {
    shows_a_subject_only_what_it_may_read(true)
}

fn refuses_writes_the_store_unchanged(separate_rules: bool) -> Result<()> {
    let mut w = World::new(separate_rules)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let before = all(&mut w.links);
    let mut view = w.as_(v["alice"]);
    assert_eq!(
        Some("forbidden"),
        error_of(view.update(v["note"], t["Secret"], v["note"]))
    );
    assert_eq!(
        Some("forbidden"),
        error_of(view.update(v["shared"], t["Public"], v["shared"]))
    );
    view.update(v["shared"], t["Note"], v["shared"])?;
    view.update(v["shared"], t["Shared"], v["shared"])?;
    assert_eq!(before, all(view.inner_mut()));
    let id = view.try_create(0, 0)?;
    view.update(id, 0, id)?;
    view.update(id, t["Note"], id)?;
    assert_eq!(
        Some("forbidden"),
        error_of(view.update(id, t["Secret"], id))
    );
    view.delete(id)?;
    assert_eq!(before, all(view.inner_mut()));
    let mut guest = w.as_(0);
    assert_eq!(Some("forbidden"), error_of(guest.try_create(0, 0)));
    assert_eq!(before, all(&mut w.links));
    Ok(())
}

#[test]
fn refuses_writes_the_store_unchanged_rules_in_data() -> Result<()> {
    refuses_writes_the_store_unchanged(false)
}

#[test]
fn refuses_writes_the_store_unchanged_rules_apart() -> Result<()> {
    refuses_writes_the_store_unchanged(true)
}

#[test]
fn keeps_the_hierarchy_from_who_may_write_its_values() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let extends = w.types.extends.expect("an Extends marker");
    let pair = w.links.search(t["Shared"], t["Public"]).expect("the pair");
    let mut view = w.as_(v["alice"]);
    // The link of Note itself is of type Doc.
    assert_eq!(
        Some("not-found"),
        error_of(view.update(t["Note"], t["Admin"], t["Note"]))
    );
    // The pair Shared -> Public is of type Extends.
    assert_eq!(
        Some("not-found"),
        error_of(view.update(pair, t["Shared"], t["Admin"]))
    );
    let more = view.try_create(0, 0)?;
    view.update(more, t["Shared"], t["Admin"])?;
    let mark = view.try_create(0, 0)?;
    assert_eq!(
        Some("forbidden"),
        error_of(view.update(mark, extends, more))
    );
    let mut boss = w.as_(v["boss"]);
    boss.update(pair, t["Shared"], t["Public"])?;
    Ok(())
}

#[test]
fn refuses_a_marker_as_a_subject() -> Result<()> {
    let mut w = World::new(false)?;
    let m = w.markers;
    assert_eq!(
        Some("bad-request"),
        error_of(w.allow(PermissionAccess::Read, m.allow_read, 1))
    );
    assert_eq!(
        Some("bad-request"),
        error_of(w.allow(PermissionAccess::Write, m.allow_write, 1))
    );
    assert_eq!(
        Some("bad-request"),
        error_of(PermissionPolicy::parse_access("own"))
    );
    // An access other than read or write cannot be written in Rust.
    assert_eq!(
        PermissionAccess::Write,
        PermissionPolicy::parse_access("write")?
    );
    Ok(())
}

fn gives_a_rule_given_twice_once(separate_rules: bool) -> Result<()> {
    let mut w = World::new(separate_rules)?;
    let t = w.t.clone();
    let count = w.rules().all_links().len();
    let rule = w.rules().search(t["User"], t["Note"]).expect("the rule");
    assert_eq!(
        rule,
        w.allow(PermissionAccess::Write, t["User"], t["Note"])?
    );
    assert_eq!(count, w.rules().all_links().len());
    let (alice, note) = (w.v["alice"], w.link(w.v["note"]));
    assert!(w.policy.can_write(&mut w.links, alice, &note));
    // The same pair may carry both marks.
    assert_eq!(rule, w.allow(PermissionAccess::Read, t["User"], t["Note"])?);
    assert_eq!(count + 1, w.rules().all_links().len());
    Ok(())
}

#[test]
fn gives_a_rule_given_twice_once_rules_in_data() -> Result<()> {
    gives_a_rule_given_twice_once(false)
}

#[test]
fn gives_a_rule_given_twice_once_rules_apart() -> Result<()> {
    gives_a_rule_given_twice_once(true)
}

#[test]
fn owns_what_it_created_while_untyped() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let mut view = w.as_(v["alice"]);
    let id = view.try_create(0, 0)?;
    assert!(view.exists(id));
    let link = view.inner_mut().get_link(id).unwrap();
    assert!(view.owns(&link));
    let created = view.created().clone();
    // Another subject does not own it, nor does a new view.
    assert!(!w.as_(v["alice"]).exists(id));
    let mut view = w.as_(v["alice"]).with_created(created);
    view.update(id, t["Note"], id)?;
    let link = view.inner_mut().get_link(id).unwrap();
    assert!(!view.owns(&link));
    // Undoing the update that typed it makes it its own again.
    view.inner_mut().update(id, 0, 0)?;
    let link = view.inner_mut().get_link(id).unwrap();
    assert!(view.owns(&link));
    view.forget(id);
    assert!(!view.exists(id));
    assert_eq!(Some("forbidden"), error_of(w.as_(0).try_create(0, 0)));
    Ok(())
}

#[test]
fn refuses_a_delete_that_would_take_a_hidden_link_with_it() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    // The secret refers to alice's note: deleting the note deletes it.
    let mut boss = w.as_(v["boss"]);
    let secret_use = boss.try_create(0, 0)?;
    boss.update(secret_use, t["Secret"], v["note"])?;
    NamedTypeLinks::set_name(&mut w.links, secret_use, "use")?;
    let before = all(&mut w.links);
    let mut view = w.as_(v["alice"]);
    assert_eq!(Some("forbidden"), error_of(view.delete(v["note"])));
    assert_eq!(before, all(&mut w.links));
    assert_eq!(
        Some("note".to_string()),
        NamedTypeLinks::get_name(&mut w.links, v["note"])?
    );
    assert_eq!(
        Some("use".to_string()),
        NamedTypeLinks::get_name(&mut w.links, secret_use)?
    );
    // Boss may delete both.
    w.as_(v["boss"]).delete(v["note"])?;
    assert!(!w.links.exists(secret_use));
    assert!(!w.links.exists(v["note"]));
    Ok(())
}

#[test]
fn refuses_a_merge_that_would_repoint_a_hidden_link() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let mut boss = w.as_(v["boss"]);
    // Two equal notes but for their targets; the secret refers to one.
    let other = boss.try_create(0, 0)?;
    boss.update(other, t["Note"], v["shared"])?;
    let secret_use = boss.try_create(0, 0)?;
    boss.update(secret_use, t["Secret"], other)?;
    let before = all(&mut w.links);
    // Making `other` equal to the note merges it into the note, repointing
    // the secret.
    assert_eq!(
        Link::new(v["note"], t["Note"], v["note"]),
        w.link(v["note"])
    );
    let mut view = w.as_(v["alice"]);
    assert_eq!(
        Some("forbidden"),
        error_of(view.update(other, t["Note"], v["note"]))
    );
    assert_eq!(before, all(&mut w.links));
    // Boss may.
    w.as_(v["boss"]).update(other, t["Note"], v["note"])?;
    assert!(!w.links.exists(other));
    assert_eq!(
        Link::new(secret_use, t["Secret"], v["note"]),
        w.link(secret_use)
    );
    Ok(())
}

#[test]
fn reports_every_change_of_a_write() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let mut view = w.as_(v["alice"]);
    let id = view.try_create(0, 0)?;
    let mut changes = Vec::new();
    view.update_observed(id, t["Note"], id, &mut |before, after| {
        changes.push((before, after))
    })?;
    assert_eq!(
        vec![(Link::new(id, 0, 0), Link::new(id, t["Note"], id))],
        changes
    );
    Ok(())
}

#[test]
fn names_need_the_right_to_write() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let mut view = w.as_(v["alice"]);
    // Names are references: any name resolves.
    assert_eq!(Some(v["secret"]), view.get_by_name("secret")?);
    assert_eq!(Some("Note".to_string()), view.get_name(t["Note"])?);
    view.set_name(v["note"], "my note")?;
    assert_eq!(
        Some("my note".to_string()),
        view.inner_mut().get_name(v["note"])?
    );
    assert_eq!(Some("not-found"), error_of(view.set_name(v["secret"], "x")));
    assert_eq!(
        Some("not-found"),
        error_of(w.as_(0).set_name(v["note"], "y"))
    );
    let mut view = w.as_(v["alice"]);
    // Taking the name of a link it may not write is taking a write.
    assert_eq!(
        Some("forbidden"),
        error_of(view.set_name(v["note"], "secret"))
    );
    assert_eq!(Some(v["secret"]), view.inner_mut().get_by_name("secret")?);
    view.remove_name(v["secret"])?;
    assert_eq!(
        Some("secret".to_string()),
        NamedTypeLinks::get_name(&mut w.links, v["secret"])?
    );
    w.as_(v["alice"]).remove_name(v["note"])?;
    assert_eq!(None, NamedTypeLinks::get_name(&mut w.links, v["note"])?);
    Ok(())
}
