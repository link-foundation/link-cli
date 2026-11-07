//! Sessions with permissions over a store that fails, or panics, mid request.

use std::cell::Cell;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

use anyhow::{bail, Result};
use link_cli::link_storage::ChangeObserver;
use link_cli::protocol::{error_message, parse_document, query_document, LinksSessions};
use link_cli::query_options::QueryLimits;
use link_cli::{AccessPolicy, Link, LinkStorage, NamedTypeLinks, PermissionMarkers};

/// Steps until the store fails, or panics once when told to.
#[derive(Clone)]
struct Steps {
    left: Rc<Cell<usize>>,
    panics: Rc<Cell<bool>>,
}

impl Steps {
    fn new() -> Self {
        Self {
            left: Rc::new(Cell::new(usize::MAX)),
            panics: Rc::new(Cell::new(false)),
        }
    }

    fn step(&self) -> Result<()> {
        match self.left.get() {
            0 if self.panics.replace(false) => {
                self.left.set(usize::MAX);
                panic!("injected panic")
            }
            0 => bail!("injected fault"),
            left => {
                self.left.set(left - 1);
                Ok(())
            }
        }
    }
}

struct Faulty {
    inner: LinkStorage,
    steps: Steps,
}

impl NamedTypeLinks for Faulty {
    fn query_limits(&self) -> Option<QueryLimits> {
        Some(QueryLimits::causal())
    }
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.inner.create(source, target)
    }
    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        self.steps.step()?;
        Ok(self.inner.create(source, target))
    }
    fn ensure_created(&mut self, id: u32) -> u32 {
        self.inner.ensure_created(id)
    }
    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        self.steps.step()?;
        Ok(self.inner.ensure_created(id))
    }
    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.inner.get_link(id)
    }
    fn exists(&mut self, id: u32) -> bool {
        self.inner.exists(id)
    }
    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        changed: ChangeObserver<'_>,
    ) -> Result<Link> {
        self.steps.step()?;
        self.inner.update_observed(id, source, target, changed)
    }
    fn delete_observed(&mut self, id: u32, changed: ChangeObserver<'_>) -> Result<Link> {
        self.steps.step()?;
        self.inner.delete_observed(id, changed)
    }
    fn all_links(&mut self) -> Vec<Link> {
        self.inner.all_links()
    }
    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        self.inner.search(source, target)
    }
    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        self.inner.get_or_create(source, target)
    }
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        NamedTypeLinks::get_name(&mut self.inner, id)
    }
    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        NamedTypeLinks::set_name(&mut self.inner, id, name)
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        self.steps.step()?;
        NamedTypeLinks::get_by_name(&mut self.inner, name)
    }
    fn remove_name(&mut self, id: u32) -> Result<()> {
        NamedTypeLinks::remove_name(&mut self.inner, id)
    }
    fn save(&mut self) -> Result<()> {
        self.inner.save()
    }
}

/// Everyone may do everything, except read a link it was told not to.
struct Open {
    hidden: u32,
}

impl AccessPolicy for Open {
    fn markers(&self) -> PermissionMarkers {
        PermissionMarkers {
            allow_read: 1,
            allow_write: 2,
            anything: 3,
            allow_reference: 4,
        }
    }
    fn can_read(&mut self, _: &mut dyn NamedTypeLinks, _: u32, link: &Link) -> bool {
        link.index != self.hidden
    }
    fn can_write(&mut self, _: &mut dyn NamedTypeLinks, _: u32, _: &Link) -> bool {
        true
    }
    fn can_create(&mut self, _: &mut dyn NamedTypeLinks, _: u32) -> bool {
        true
    }
    fn can_reference(&mut self, _: &mut dyn NamedTypeLinks, _: u32, _: &Link) -> bool {
        true
    }
}

fn sessions(steps: &Steps, hidden: u32) -> LinksSessions<Faulty, Open> {
    let mut inner = LinkStorage::in_memory(false);
    inner.create(1, 1);
    LinksSessions::with_permissions(
        Faulty {
            inner,
            steps: steps.clone(),
        },
        Open { hidden },
    )
}

const TWO_LINKS: &str = "() ((7: 7 7) (8: 8 8))";

#[test]
fn a_malformed_request_is_answered_as_an_error() {
    let steps = Steps::new();
    let mut sessions = sessions(&steps, 0);
    let session = sessions.open(None);
    for (text, expected) in [
        ("(get-name: x)", "expected a number"),
        ("(get-by-name: (a b))", "expected get-by-name"),
    ] {
        let reply = sessions.execute(session, &parse_document(text).unwrap());
        assert!(error_message(&reply).unwrap().contains(expected), "{text}");
    }
}

#[test]
fn a_failed_name_lookup_is_answered_as_an_error() {
    let steps = Steps::new();
    let mut sessions = sessions(&steps, 0);
    let session = sessions.open(None);
    steps.left.set(0);
    let reply = sessions.execute(session, &parse_document("(get-by-name: a)").unwrap());
    assert_eq!(error_message(&reply), Some("injected fault"));
}

#[test]
fn a_failed_undo_is_reported_along_with_the_failure() {
    let steps = Steps::new();
    let mut sessions = sessions(&steps, 0);
    let session = sessions.open(None);

    // Nothing was written, so there is nothing to undo.
    steps.left.set(0);
    let reply = sessions.execute(session, &query_document(TWO_LINKS));
    assert_eq!(error_message(&reply), Some("injected fault"));
    assert_eq!(sessions.store_mut().inner.all_links().len(), 1);

    // The first write landed and the budget is gone before its undo.
    steps.left.set(1);
    let reply = sessions.execute(session, &query_document(TWO_LINKS));
    assert_eq!(
        error_message(&reply),
        Some("injected fault; undoing it failed: injected fault")
    );
    assert_eq!(sessions.store_mut().inner.all_links().len(), 2);
}

#[test]
fn a_panicking_store_rolls_the_request_back_and_keeps_the_session() {
    let steps = Steps::new();
    let mut sessions = sessions(&steps, 0);
    let session = sessions.open(None);

    steps.left.set(1);
    steps.panics.set(true);
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        sessions.execute(session, &query_document(TWO_LINKS))
    }));
    assert!(outcome.is_err());
    assert_eq!(sessions.store_mut().inner.all_links().len(), 1);

    let reply = sessions.execute(session, &query_document(TWO_LINKS));
    assert_eq!(error_message(&reply), None);
    assert_eq!(sessions.store_mut().inner.all_links().len(), 3);
}

#[test]
fn the_policy_and_the_store_can_be_taken_back() {
    let steps = Steps::new();
    let mut sessions = sessions(&steps, 5);
    assert_eq!(sessions.policy().unwrap().hidden, 5);

    // A session that is not open reads as a guest under the same policy.
    let session = sessions.open(None);
    sessions.close(session);
    assert!(sessions.can_read(session, &Link::new(4, 1, 1)));
    assert!(!sessions.can_read(session, &Link::new(5, 1, 1)));

    assert_eq!(sessions.into_inner().inner.all_links().len(), 1);
}
