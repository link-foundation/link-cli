//! A reference needs a right (issue #161): without it an unreadable address
//! answers exactly as a missing one. The same cases are in the C#
//! `PermissionsReferencesTests`.

#[path = "common/permissions_world.rs"]
mod world;

use anyhow::Result;
use link_cli::{
    protocol::LinksSessions, AccessPolicy, Forbidden, Link, NamedTypeLinks, PermissionAccess,
    PermissionMarkers, PermissionsDecorator,
};
use world::{all, World};

const MISSING: u32 = 9999;

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

/// A note of alice's own, as the requests that follow may update it.
fn own_note(w: &mut World) -> Result<u32> {
    let (note, alice) = (w.t["Note"], w.v["alice"]);
    let mut view = w.as_(alice);
    let id = view.try_create(0, 0)?;
    view.update(id, note, 0)?;
    Ok(id)
}

/// The reply of `request` as `subject`.
fn ask(w: &mut World, subject: u32, request: &str) -> String {
    let mut sessions = LinksSessions::with_permissions(&mut w.links, &mut w.policy);
    let session = sessions.open(Some(subject));
    sessions.request(session, request)
}

fn answers_alike(reply_of: impl Fn(u32, u32, u32) -> String, not_found: bool) -> Result<()> {
    let mut w = World::new(false)?;
    let own = own_note(&mut w)?;
    let (note, secret, alice) = (w.t["Note"], w.v["secret"], w.v["alice"]);
    let before = all(&mut w.links);
    let hidden = ask(&mut w, alice, &reply_of(note, secret, own));
    let missing = ask(&mut w, alice, &reply_of(note, MISSING, own));
    assert!(hidden.starts_with("(error: "), "{hidden}");
    assert_eq!(
        hidden.replace(&secret.to_string(), "N"),
        missing.replace(&MISSING.to_string(), "N")
    );
    if not_found {
        assert_eq!(format!("(error: 'Link not found: {secret}')"), hidden);
    }
    assert_eq!(before, all(&mut w.links));
    Ok(())
}

#[test]
fn a_create_answers_alike_for_an_unreadable_and_a_missing_address() -> Result<()> {
    answers_alike(|note, a, _| format!("(create: ({note} {a}))"), true)?;
    answers_alike(|_, a, _| format!("(create: (0 {a}))"), true)
}

#[test]
fn an_update_answers_alike_for_an_unreadable_and_a_missing_address() -> Result<()> {
    answers_alike(|note, a, own| format!("(update: ({own} {note} {a}))"), true)
}

#[test]
fn a_query_answers_alike_for_an_unreadable_and_a_missing_address() -> Result<()> {
    answers_alike(|note, a, _| format!("() (({note} {a}))"), false)?;
    answers_alike(|_, a, _| format!("(({a}: * *)) ()"), false)
}

#[test]
fn picking_an_address_answers_alike_for_an_unreadable_and_a_missing_one() -> Result<()> {
    answers_alike(|note, a, _| format!("() (({a}: {note} {note}))"), false)
}

#[test]
fn a_type_it_may_not_write_is_forbidden_for_hidden_and_missing_alike() -> Result<()> {
    let mut w = World::new(false)?;
    let own = own_note(&mut w)?;
    let (secret_type, alice) = (w.t["Secret"], w.v["alice"]);
    for type_id in [secret_type, MISSING] {
        assert_eq!(
            format!("(error: 'Forbidden: update {own}')"),
            ask(&mut w, alice, &format!("(update: ({own} {type_id} {own}))"))
        );
    }
    Ok(())
}

#[test]
fn the_name_of_an_unreadable_link_is_an_unknown_name() -> Result<()> {
    let mut w = World::new(false)?;
    let (note, secret, alice) = (w.t["Note"], w.v["secret"], w.v["alice"]);
    assert_eq!(None, w.as_(alice).get_by_name("secret")?);
    assert_eq!(None, w.as_(alice).get_name(secret)?);
    for request in [
        format!("() (({note} NAME))"),
        "((NAME: * *)) ()".to_string(),
    ] {
        let hidden = ask(&mut w, alice, &request.replace("NAME", "secret"));
        let missing = ask(&mut w, alice, &request.replace("NAME", "nothing"));
        assert!(hidden.starts_with("(error: "), "{hidden}");
        assert_eq!(
            hidden.replace("secret", "N"),
            missing.replace("nothing", "N")
        );
    }
    Ok(())
}

#[test]
fn the_name_of_an_unreadable_link_cannot_be_taken() -> Result<()> {
    let mut w = World::new(false)?;
    let (note, secret, alice) = (w.v["note"], w.v["secret"], w.v["alice"]);
    let reply = ask(&mut w, alice, &format!("(set-name: ({note} 'secret'))"));
    assert!(reply.starts_with("(error: 'Forbidden: "), "{reply}");
    let reply = ask(&mut w, alice, &format!("() ((secret: {note} {note}))"));
    assert!(reply.starts_with("(error: 'Forbidden: "), "{reply}");
    assert_eq!(Some(secret), w.links.get_by_name("secret"));
    assert_eq!(Some("note".to_string()), w.links.get_name(note).cloned());
    Ok(())
}

#[test]
fn the_view_refuses_it_alone() -> Result<()> {
    let mut w = World::new(false)?;
    let (note, secret, alice) = (w.t["Note"], w.v["secret"], w.v["alice"]);
    let mut view = w.as_(alice);
    let id = view.try_create(0, 0)?;
    for address in [secret, MISSING] {
        assert_eq!(Some("not-found"), error_of(view.update(id, note, address)));
        assert_eq!(Some("not-found"), error_of(view.update(id, 0, address)));
        assert_eq!(Some("not-found"), error_of(view.try_create(note, address)));
        assert_eq!(
            Some("not-found"),
            error_of(view.try_ensure_created(address))
        );
        assert!(!view.reference_exists(address));
    }
    assert_eq!(Some(Link::new(id, 0, 0)), w.links.get_link(id));
    Ok(())
}

#[test]
fn a_rule_covers_its_object_and_the_links_of_its_type() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    w.allow(PermissionAccess::Reference, t["User"], t["Secret"])?;
    let mut view = w.as_(v["alice"]);
    view.try_create(t["Note"], v["secret"])?;
    view.try_create(t["Note"], t["Secret"])?;
    // Not its sibling, and a missing address stays missing.
    assert_eq!(
        Some("not-found"),
        error_of(view.try_create(t["Note"], t["Admin"]))
    );
    assert_eq!(
        Some("not-found"),
        error_of(view.try_create(t["Note"], MISSING))
    );
    // With the right the query answers that the address is taken.
    assert!(view.reference_exists(v["secret"]));
    assert!(!view.reference_exists(MISSING));
    assert_eq!(Some(v["secret"]), view.get_by_name("secret")?);
    let reply = view.try_ensure_created(v["secret"]).unwrap_err();
    assert_eq!(format!("Link {} exists", v["secret"]), reply.to_string());
    Ok(())
}

#[test]
fn the_right_does_not_allow_reading() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    w.allow(PermissionAccess::Reference, t["User"], t["Secret"])?;
    let (alice, secret) = (v["alice"], v["secret"]);
    {
        let mut view = w.as_(alice);
        assert!(view.can_reference(secret));
        assert!(!view.can_read(&Link::new(secret, t["Secret"], 0)));
        assert_eq!(None, view.get_link(secret));
        assert!(!view.all_links().iter().any(|link| link.index == secret));
        assert_eq!(Some("not-found"), error_of(view.delete(secret)));
    }
    assert_eq!("", ask(&mut w, alice, &format!("(each: ({secret}))")));
    assert_eq!("", ask(&mut w, alice, "(get-by-name: 'secret')"));
    Ok(())
}

#[test]
fn a_rule_applies_to_its_subjects_only() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    w.allow(PermissionAccess::Reference, t["Admin"], t["Secret"])?;
    // boss is an Admin, alice only a User, and a guest nobody.
    assert!(w.as_(v["boss"]).can_reference(v["secret"]));
    assert!(!w.as_(v["alice"]).can_reference(v["secret"]));
    assert!(!w.as_(0).can_reference(v["secret"]));
    Ok(())
}

#[test]
fn the_object_of_a_rule_of_the_subject_needs_no_more_rules() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let mut view = w.as_(v["alice"]);
    // User writes Note: enough to type a link as Note, yet Note stays unread.
    assert_eq!(None, view.get_link(t["Note"]));
    view.try_create(t["Note"], 0)?;
    // User reads Public, readable links, and the subject itself.
    view.try_create(t["Note"], t["Public"])?;
    view.try_create(t["Note"], v["shared"])?;
    view.try_create(t["Note"], v["alice"])?;
    // Who it says nothing of: the guest.
    let mut guest = w.as_(0);
    assert!(!guest.can_reference(t["Note"]));
    Ok(())
}

#[test]
fn a_reference_it_holds_may_stay_but_cannot_be_put_back() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let held = {
        let mut boss = w.as_(v["boss"]);
        boss.try_create(t["Note"], v["secret"])?
    };
    let mut view = w.as_(v["alice"]);
    // It stays, while the update changes what the subject may change.
    view.update(held, t["Shared"], v["secret"])?;
    view.update(held, t["Note"], v["secret"])?;
    // Once replaced, it cannot be put back.
    view.update(held, t["Note"], 0)?;
    assert_eq!(
        Some("not-found"),
        error_of(view.update(held, t["Note"], v["secret"]))
    );
    Ok(())
}

#[test]
fn anything_to_anything_restores_the_previous_behaviour_as_a_choice() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v, m) = (w.t.clone(), w.v.clone(), w.markers);
    w.allow(PermissionAccess::Reference, m.anything, m.anything)?;
    let (alice, secret) = (v["alice"], v["secret"]);
    let mut view = w.as_(alice);
    view.try_create(t["Note"], secret)?;
    assert_eq!(Some(secret), view.get_by_name("secret")?);
    // The address of a hidden link is taken, a free one is not.
    assert_eq!(
        format!("Link {secret} exists"),
        view.try_ensure_created(secret).unwrap_err().to_string()
    );
    // A query tells them apart: the link exists, the other one does not.
    assert!(view.reference_exists(secret));
    assert!(!view.reference_exists(MISSING));
    view.try_ensure_created(MISSING)?;
    assert!(view.exists(MISSING));
    // It reads no more than it did.
    assert_eq!(None, view.get_link(secret));
    // A query refers to it by name, here to the link made above.
    let reply = ask(&mut w, alice, &format!("() (({} secret))", t["Note"]));
    assert!(!reply.starts_with("(error"), "{reply}");
    Ok(())
}

#[test]
fn an_administrator_is_not_affected() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let boss = v["boss"];
    let reply = ask(
        &mut w,
        boss,
        &format!("() (({} {}))", t["Note"], v["secret"]),
    );
    assert!(reply.starts_with("() (("), "{reply}");
    let reply = ask(&mut w, boss, &format!("() ((12345: {0} {0}))", t["Note"]));
    assert!(reply.starts_with("() (("), "{reply}");
    Ok(())
}

/// Everyone reads what starts at a type it names, and says nothing of references.
struct ReadsPublicOnly(u32, PermissionMarkers);

impl AccessPolicy for ReadsPublicOnly {
    fn markers(&self) -> PermissionMarkers {
        self.1
    }

    fn can_read(&mut self, _: &mut dyn NamedTypeLinks, _: u32, link: &Link) -> bool {
        link.source == self.0
    }

    fn can_write(&mut self, _: &mut dyn NamedTypeLinks, _: u32, _: &Link) -> bool {
        true
    }

    fn can_create(&mut self, _: &mut dyn NamedTypeLinks, _: u32) -> bool {
        true
    }
}

#[test]
fn a_policy_without_the_right_allows_references_to_readable_links_only() -> Result<()> {
    let mut w = World::new(false)?;
    let (t, v) = (w.t.clone(), w.v.clone());
    let readable = w.links.try_create(t["Public"], 0)?;
    let mut view = PermissionsDecorator::new(
        &mut w.links,
        ReadsPublicOnly(t["Public"], w.markers),
        v["alice"],
    );
    view.try_create(0, readable)?;
    assert_eq!(Some("not-found"), error_of(view.try_create(0, v["secret"])));
    Ok(())
}
