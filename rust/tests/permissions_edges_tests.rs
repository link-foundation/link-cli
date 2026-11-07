//! What a permissions view answers at its edges, each right asked of a policy
//! that states it plainly, over a store whose writes can change other links
//! or fail. `permissions_decorator_tests.rs` has the rules themselves.

use std::collections::HashSet;
use std::panic::{catch_unwind, AssertUnwindSafe};

use anyhow::{bail, Result};
use link_cli::{
    AccessPolicy, Forbidden, Link, LinkStorage, NamedTypeLinks, PermissionAccess,
    PermissionMarkers, PermissionsDecorator,
};

#[path = "common/permissions_world.rs"]
mod world;

const MARKERS: PermissionMarkers = PermissionMarkers {
    allow_read: 1,
    allow_write: 2,
    anything: 3,
    allow_reference: 4,
};

/// Every right as plain data.
#[derive(Default)]
struct Plain {
    hidden: HashSet<u32>,
    locked: HashSet<u32>,
    referable: HashSet<u32>,
    no_create: bool,
}

impl AccessPolicy for Plain {
    fn markers(&self) -> PermissionMarkers {
        MARKERS
    }

    fn can_read(&mut self, _: &mut dyn NamedTypeLinks, _: u32, link: &Link) -> bool {
        !self.hidden.contains(&link.index)
    }

    fn can_write(&mut self, _: &mut dyn NamedTypeLinks, _: u32, link: &Link) -> bool {
        !self.locked.contains(&link.index)
    }

    fn can_create(&mut self, _: &mut dyn NamedTypeLinks, _: u32) -> bool {
        !self.no_create
    }

    fn can_reference(&mut self, _: &mut dyn NamedTypeLinks, _: u32, link: &Link) -> bool {
        self.referable.contains(&link.index)
    }
}

/// A step a write of [`Scripted`] takes besides the one it was asked for.
#[derive(Clone, Copy)]
enum Also {
    Create(u32, u32),
    Update(u32, u32, u32),
    Delete(u32),
    Nothing,
    Fail,
}

/// A store whose update also runs a script, and whose deletes can fail.
struct Scripted {
    links: LinkStorage,
    also: Vec<Also>,
    deletes_fail: bool,
}

impl Scripted {
    fn with_rows(count: u32) -> Self {
        let mut links = LinkStorage::in_memory(false);
        for _ in 0..count {
            links.create(0, 0);
        }
        Self {
            links,
            also: Vec::new(),
            deletes_fail: false,
        }
    }
}

impl NamedTypeLinks for Scripted {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.links.create(source, target)
    }
    fn ensure_created(&mut self, id: u32) -> u32 {
        NamedTypeLinks::ensure_created(&mut self.links, id)
    }
    fn get_link(&mut self, id: u32) -> Option<Link> {
        NamedTypeLinks::get_link(&mut self.links, id)
    }
    fn exists(&mut self, id: u32) -> bool {
        NamedTypeLinks::exists(&mut self.links, id)
    }
    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: &mut dyn FnMut(Link, Link),
    ) -> Result<Link> {
        let before = self.links.update_observed(id, source, target, observer)?;
        // The script runs once: the undo of this write comes through here too.
        for step in std::mem::take(&mut self.also) {
            match step {
                Also::Create(s, t) => {
                    let made = self.links.create(s, t);
                    observer(Link::null(), Link::new(made, s, t));
                }
                Also::Update(at, s, t) => {
                    self.links.update_observed(at, s, t, observer)?;
                }
                Also::Delete(at) => {
                    self.links.delete_observed(at, observer)?;
                }
                Also::Nothing => observer(Link::null(), Link::null()),
                Also::Fail => bail!("the store refused"),
            }
        }
        Ok(before)
    }
    fn delete_observed(&mut self, id: u32, observer: &mut dyn FnMut(Link, Link)) -> Result<Link> {
        if self.deletes_fail {
            bail!("deletes fail");
        }
        self.links.delete_observed(id, observer)
    }
    fn all_links(&mut self) -> Vec<Link> {
        NamedTypeLinks::all_links(&mut self.links)
    }
    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        NamedTypeLinks::search(&mut self.links, source, target)
    }
    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        NamedTypeLinks::get_or_create(&mut self.links, source, target)
    }
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        NamedTypeLinks::get_name(&mut self.links, id)
    }
    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        NamedTypeLinks::set_name(&mut self.links, id, name)
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        NamedTypeLinks::get_by_name(&mut self.links, name)
    }
    fn remove_name(&mut self, id: u32) -> Result<()> {
        NamedTypeLinks::remove_name(&mut self.links, id)
    }
    fn save(&mut self) -> Result<()> {
        NamedTypeLinks::save(&mut self.links)
    }
}

fn view(links: Scripted, policy: Plain) -> PermissionsDecorator<Scripted, Plain> {
    PermissionsDecorator::new(links, policy, 0)
}

fn is_forbidden<T: std::fmt::Debug>(result: Result<T>) -> bool {
    result.unwrap_err().downcast_ref::<Forbidden>().is_some()
}

fn is_not_found<T: std::fmt::Debug>(result: Result<T>) -> bool {
    result
        .unwrap_err()
        .to_string()
        .starts_with("Link not found")
}

fn set(items: &[u32]) -> HashSet<u32> {
    items.iter().copied().collect()
}

#[test]
fn a_boxed_policy_is_asked_the_same_questions() -> Result<()> {
    let mut boxed: Box<dyn AccessPolicy> = Box::new(Plain {
        hidden: set(&[1]),
        locked: set(&[2]),
        referable: set(&[1]),
        no_create: true,
    });
    let mut data = LinkStorage::in_memory(false);
    assert_eq!(boxed.markers().allow_read, MARKERS.allow_read);
    assert!(!boxed.can_read(&mut data, 0, &Link::new(1, 0, 0)));
    assert!(!boxed.can_write(&mut data, 0, &Link::new(2, 0, 0)));
    assert!(!boxed.can_create(&mut data, 0));
    assert!(boxed.can_reference(&mut data, 0, &Link::new(1, 0, 0)));
    Ok(())
}

#[test]
fn a_view_hands_out_its_parts() {
    let mut links = view(Scripted::with_rows(1), Plain::default());
    links.policy_mut().no_create = true;
    assert!(is_forbidden(links.try_create(0, 0)));
    assert!(links.inner().also.is_empty());
    links.inner_mut().also.push(Also::Nothing);
    assert_eq!(links.into_inner().also.len(), 1);
}

#[test]
fn the_rules_keep_to_their_own_store_and_skip_marks_of_nothing() -> Result<()> {
    let mut apart = world::World::new(true)?;
    assert!(apart.policy.rules_mut().is_some());
    let mut together = world::World::new(false)?;
    assert!(together.policy.rules_mut().is_none());

    let marker = together.markers.of(PermissionAccess::Read);
    let granted = together.policy.granted(&mut together.links, marker);
    together.links.create(marker, 0);
    together.links.create(marker, 9_999);
    assert_eq!(
        together.policy.granted(&mut together.links, marker),
        granted
    );
    Ok(())
}

#[test]
fn a_write_to_nothing_answers_what_the_store_answers() {
    let mut links = view(Scripted::with_rows(1), Plain::default());
    assert!(links.update(0, 0, 0).is_err());
    assert!(is_not_found(links.update(99, 0, 0)));
    assert!(links.delete(0).is_err());
    assert!(is_not_found(links.delete(99)));
}

#[test]
fn a_hidden_link_is_not_found_and_a_locked_one_is_forbidden() -> Result<()> {
    let policy = Plain {
        hidden: set(&[1]),
        locked: set(&[2, 3]),
        ..Plain::default()
    };
    let mut links = view(Scripted::with_rows(4), policy);
    assert!(is_not_found(links.update(1, 0, 0)));
    assert!(is_not_found(links.delete(1)));
    assert!(is_forbidden(links.update(2, 0, 0)));
    assert!(is_forbidden(links.delete(2)));
    // A link it may write cannot be made into a link it may not write.
    links.policy_mut().locked = set(&[3]);
    assert!(is_forbidden(links.update(3, 4, 4)));
    assert_eq!(links.update(2, 4, 4)?, Link::new(2, 0, 0));
    Ok(())
}

#[test]
fn a_point_at_an_address_needs_every_right_and_a_free_address() -> Result<()> {
    let mut links = view(Scripted::with_rows(2), Plain::default());
    assert_eq!(links.try_ensure_created(2)?, 2, "it is there");
    let null = links.try_ensure_created(0).unwrap_err().to_string();
    assert!(null.contains("unsupported link address 0"), "{null}");
    assert!(!links.created().contains(&0));

    links.policy_mut().hidden = set(&[1, 7]);
    assert!(
        is_not_found(links.try_ensure_created(7)),
        "hidden and not referable"
    );
    links.policy_mut().referable = set(&[1, 7]);
    let taken = links.try_ensure_created(1).unwrap_err().to_string();
    assert!(taken.contains("Link 1 exists"), "{taken}");
    assert_eq!(links.ensure_created(7), 7);
    assert!(links.created().contains(&7));

    links.policy_mut().no_create = true;
    assert!(is_forbidden(links.try_ensure_created(9)));
    Ok(())
}

#[test]
fn the_calls_that_cannot_answer_an_error_panic_with_the_refusal() {
    let policy = Plain {
        no_create: true,
        ..Plain::default()
    };
    let mut links = view(Scripted::with_rows(1), policy);
    let panicked = |run: &mut dyn FnMut()| {
        let payload = catch_unwind(AssertUnwindSafe(run)).unwrap_err();
        let text = payload
            .downcast_ref::<String>()
            .cloned()
            .unwrap_or_else(|| payload.downcast_ref::<&str>().unwrap_or(&"").to_string());
        assert!(text.contains("Forbidden"), "{text}");
    };
    panicked(&mut || {
        links.create(0, 0);
    });
    panicked(&mut || {
        links.ensure_created(5);
    });
    panicked(&mut || {
        links.get_or_create(9, 9);
    });
}

#[test]
fn the_answers_about_addresses_and_saving_come_from_the_store() -> Result<()> {
    let mut links = view(Scripted::with_rows(1), Plain::default());
    assert_eq!(links.claims(1), None);
    assert_eq!(links.next_created_addresses(1)?, [2]);
    assert!(links.reference_exists(1));
    links.save()?;
    Ok(())
}

#[test]
fn names_follow_the_rights_of_their_links() -> Result<()> {
    let mut store = Scripted::with_rows(4);
    NamedTypeLinks::set_name(&mut store, 1, "hidden")?;
    NamedTypeLinks::set_name(&mut store, 2, "locked")?;
    NamedTypeLinks::set_name(&mut store, 3, "free")?;
    let policy = Plain {
        hidden: set(&[1]),
        locked: set(&[2]),
        ..Plain::default()
    };
    let mut links = view(store, policy);

    assert_eq!(links.get_name(1)?, None);
    assert_eq!(links.get_name(2)?.as_deref(), Some("locked"));
    assert_eq!(links.get_by_name("hidden")?, None);
    assert_eq!(links.get_by_name("locked")?, Some(2));
    assert_eq!(links.get_by_name("absent")?, None);

    assert!(is_not_found(links.set_name(1, "x")));
    assert!(is_not_found(links.set_name(99, "x")));
    assert!(is_forbidden(links.set_name(2, "x")));
    assert!(
        is_forbidden(links.set_name(4, "locked")),
        "it takes a name from a locked link"
    );
    assert_eq!(
        links.set_name(4, "hidden")?,
        4,
        "the rules below decide a hidden holder's name"
    );
    assert_eq!(links.set_name(3, "free")?, 3, "a link's own name");

    links.remove_name(1)?;
    assert_eq!(
        links.inner_mut().get_name(1)?,
        None,
        "the name moved to link 4"
    );
    assert!(is_forbidden(links.remove_name(2)));
    links.remove_name(3)?;
    assert_eq!(links.get_name(3)?, None);
    links.remove_name(99)?;
    Ok(())
}

fn scripted(also: Vec<Also>, policy: Plain) -> PermissionsDecorator<Scripted, Plain> {
    let mut store = Scripted::with_rows(4);
    store.also = also;
    view(store, policy)
}

#[test]
fn a_write_that_creates_another_link_needs_the_right_to_create_it() -> Result<()> {
    let mut links = scripted(vec![Also::Create(0, 0)], Plain::default());
    links.update(1, 1, 1)?;
    assert!(links.inner_mut().exists(5));

    let policy = Plain {
        no_create: true,
        ..Plain::default()
    };
    let mut links = scripted(vec![Also::Create(0, 0)], policy);
    assert!(is_forbidden(links.update(1, 1, 1)));
    assert!(!links.inner_mut().exists(5), "undone");
    assert_eq!(links.inner_mut().get_link(1), Some(Link::new(1, 0, 0)));

    let policy = Plain {
        locked: set(&[5]),
        ..Plain::default()
    };
    let mut links = scripted(vec![Also::Create(1, 1)], policy);
    assert!(
        is_forbidden(links.update(1, 1, 1)),
        "a new link it may not write"
    );
    assert!(!links.inner_mut().exists(5));
    Ok(())
}

#[test]
fn a_write_that_changes_another_link_needs_the_right_to_write_it() -> Result<()> {
    let locked = || Plain {
        locked: set(&[2]),
        ..Plain::default()
    };
    let mut links = scripted(vec![Also::Update(2, 2, 2)], locked());
    assert!(is_forbidden(links.update(1, 1, 1)));
    assert_eq!(links.inner_mut().get_link(2), Some(Link::new(2, 0, 0)));

    let mut links = scripted(vec![Also::Delete(2)], locked());
    assert!(is_forbidden(links.update(1, 1, 1)));
    assert!(links.inner_mut().exists(2), "undone");

    // Unchanged in the end, or still its own without a type.
    let steps = vec![Also::Update(2, 3, 3), Also::Update(2, 0, 0), Also::Nothing];
    let mut links = scripted(steps, locked());
    links.update(1, 1, 1)?;
    let mut links = scripted(vec![Also::Update(2, 0, 0)], locked()).with_created(set(&[2]));
    links.update(1, 1, 1)?;

    // A link that went away stops being the subject's own.
    let mut links = scripted(vec![Also::Delete(3)], Plain::default()).with_created(set(&[3]));
    let mut seen = Vec::new();
    links.update_observed(1, 1, 1, &mut |before, after| seen.push((before, after)))?;
    assert!(!links.created().contains(&3));
    assert_eq!(seen[0], (Link::new(1, 0, 0), Link::new(1, 1, 1)));
    Ok(())
}

#[test]
fn a_failed_write_is_undone_and_a_failed_undo_is_told() {
    let steps = vec![Also::Create(0, 0), Also::Fail];
    let mut links = scripted(steps.clone(), Plain::default());
    let refused = links.update(1, 1, 1).unwrap_err().to_string();
    assert_eq!(refused, "the store refused");
    assert!(!links.inner_mut().exists(5), "undone");
    assert_eq!(links.inner_mut().get_link(1), Some(Link::new(1, 0, 0)));

    let mut links = scripted(steps, Plain::default());
    links.inner_mut().deletes_fail = true;
    let told = format!("{:#}", links.update(1, 1, 1).unwrap_err());
    assert!(
        told.contains("and undoing it failed: deletes fail"),
        "{told}"
    );
    assert!(told.contains("the store refused"), "{told}");

    let mut links = scripted(vec![Also::Delete(2)], Plain::default());
    links.policy_mut().locked = set(&[2]);
    links.inner_mut().deletes_fail = true;
    // Deleting is the store's step, so a store that refuses it refuses the write.
    assert!(links.update(1, 1, 1).is_err());
}
