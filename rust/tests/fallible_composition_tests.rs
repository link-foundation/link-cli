//! Finite adapter faults must remain errors through query/trigger composition.
use anyhow::{bail, Result};
use link_cli::{
    Link, NamedTypeLinks, NamedTypesDecorator, PersistentTransformationDecorator,
    PersistentTransformationKind, QueryProcessor,
};
use std::{cell::RefCell, rc::Rc};
#[path = "common/import_operation.rs"]
mod import_operation;
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;

#[derive(Default)]
struct Control {
    calls: usize,
    fail_at: Option<usize>,
    infallible: usize,
}
struct Store {
    inner: NamedTypesDecorator,
    control: Rc<RefCell<Control>>,
}
impl Store {
    fn allocating(&self) -> Result<()> {
        let mut control = self.control.borrow_mut();
        control.calls += 1;
        if control.fail_at == Some(control.calls) {
            bail!("injected allocation");
        }
        Ok(())
    }
}
impl NamedTypeLinks for Store {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.control.borrow_mut().infallible += 1;
        NamedTypeLinks::create(&mut self.inner, source, target)
    }
    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        self.allocating()?;
        self.inner.try_create(source, target)
    }
    fn ensure_created(&mut self, id: u32) -> u32 {
        self.control.borrow_mut().infallible += 1;
        NamedTypeLinks::ensure_created(&mut self.inner, id)
    }
    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        self.allocating()?;
        self.inner.try_ensure_created(id)
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
        observer: &mut dyn FnMut(Link, Link),
    ) -> Result<Link> {
        self.inner.update_observed(id, source, target, observer)
    }
    fn delete_observed(&mut self, id: u32, observer: &mut dyn FnMut(Link, Link)) -> Result<Link> {
        self.inner.delete_observed(id, observer)
    }
    fn all_links(&mut self) -> Vec<Link> {
        self.inner.all_links()
    }
    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        self.inner.search(source, target)
    }
    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        self.control.borrow_mut().infallible += 1;
        self.inner.get_or_create(source, target)
    }
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        self.inner.get_name(id)
    }
    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        self.inner.set_name(id, name)
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        self.inner.get_by_name(name)
    }
    fn remove_name(&mut self, id: u32) -> Result<()> {
        self.inner.remove_name(id)
    }
    fn save(&mut self) -> Result<()> {
        self.inner.save()
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn trigger_allocation_faults_return_before_effects_and_allow_following_edits() -> Result<()> {
    for operation in ["create", "ensure", "query", "planner"] {
        let (_files, inner) = test_store::named()?;
        let control = Rc::new(RefCell::new(Control {
            fail_at: Some(1),
            ..Default::default()
        }));
        let mut links = PersistentTransformationDecorator::embedded(
            Store {
                inner,
                control: control.clone(),
            },
            false,
        );
        let outcome = match operation {
            "create" => links.try_create(0, 0).map(|_| ()),
            "ensure" => links.try_ensure_created(8).map(|_| ()),
            "query" => QueryProcessor::new(false)
                .process_query(&mut links, "(() ((8: 8 8)))")
                .map(|_| ()),
            "planner" => QueryProcessor::new(false)
                .process_query(&mut links, "(() ((0 0)))")
                .map(|_| ()),
            _ => unreachable!(),
        };
        assert!(
            outcome.is_err(),
            "{operation}: must return allocation fault"
        );
        assert!(outcome
            .unwrap_err()
            .to_string()
            .contains("injected allocation"));
        assert_eq!(control.borrow().infallible, 0, "{operation}: wrong bridge");
        assert!(
            links.all_links().is_empty(),
            "{operation}: fault before effects"
        );
        control.borrow_mut().fail_at = None;
        let id = links.try_create(0, 0)?;
        assert_eq!(links.get_link(id), Some(Link::new(id, 0, 0)));
        links.set_name(id, "following 雪=\0")?;
        assert_eq!(links.get_name(id)?.as_deref(), Some("following 雪=\0"));
        links.remove_name(id)?;
        links.delete(id)?;
        links.save()?;
        assert!(links.all_links().is_empty());
    }
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn trigger_bookkeeping_propagates_each_allocation_fault_and_retry_is_idempotent() -> Result<()> {
    // Six named schema points already exist. The remaining eleven allocations
    // are five schema relationships, two text points and four payload/root links.
    for fail_at in 1..=11 {
        let (_files, mut inner) = test_store::named()?;
        for name in [
            "Type",
            "Trigger",
            "Once",
            "Always",
            "Condition",
            "Substitution",
        ] {
            inner.get_or_create_named(name)?;
        }
        let control = Rc::new(RefCell::new(Control {
            fail_at: Some(fail_at),
            ..Default::default()
        }));
        let mut links = PersistentTransformationDecorator::embedded(
            Store {
                inner,
                control: control.clone(),
            },
            false,
        );
        let outcome = links.store_trigger(PersistentTransformationKind::Always, "(() ())");
        assert!(
            outcome.is_err(),
            "allocation {fail_at} must reject bookkeeping"
        );
        assert!(outcome
            .unwrap_err()
            .to_string()
            .contains("injected allocation"));
        assert_eq!(
            control.borrow().calls,
            fail_at,
            "stop at the failing allocation"
        );
        assert_eq!(control.borrow().infallible, 0);
        assert!(
            links.triggers()?.is_empty(),
            "no incomplete root may become visible"
        );
        control.borrow_mut().fail_at = None;
        let root = links.store_trigger(PersistentTransformationKind::Always, "(() ())")?;
        assert_eq!(
            links.store_trigger(PersistentTransformationKind::Always, "() ()")?,
            root
        );
        let stored = links.triggers()?;
        assert_eq!(stored.len(), 1);
        assert_eq!(stored[0].root, root);
        assert_eq!(stored[0].query(), "(() ())");
        assert_eq!(links.remove_triggers("(() ())")?, 1);
        assert_eq!(links.remove_triggers("(() ())")?, 0);
        links.save()?;
    }
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn successful_fallible_allocations_fire_triggers_but_existing_rows_do_not() -> Result<()> {
    for operation in ["create", "ensure", "pair"] {
        let (_data_files, inner) = test_store::named()?;
        let (_trigger_files, schema) = test_store::named()?;
        let control = Rc::new(RefCell::new(Control::default()));
        let mut links = PersistentTransformationDecorator::with_sidecar(
            Store {
                inner,
                control: control.clone(),
            },
            schema,
            false,
        )
        .with_auto_create_missing_references(true);
        links.store_trigger(
            PersistentTransformationKind::Always,
            "(((1: * *)) ((1: 1 2)))",
        )?;
        let id = match operation {
            "create" => links.try_create(1, 1)?,
            "ensure" => links.try_ensure_created(1)?,
            "pair" => links.try_get_or_create(1, 1)?,
            _ => unreachable!(),
        };
        assert_eq!(id, 1);
        assert_eq!(links.get_link(1), Some(Link::new(1, 1, 2)));
        // Put the condition back through the caller-owned inner adapter.
        // These no-op calls must leave the matching trigger unapplied.
        links.inner_mut().inner.update(1, 1, 1)?;
        let before = control.borrow().calls;
        assert_eq!(links.try_ensure_created(1)?, 1);
        assert_eq!(links.try_get_or_create(1, 1)?, 1);
        assert_eq!(links.get_link(1), Some(Link::new(1, 1, 1)));
        // Our mock's ensure bridge counts the call, but neither path allocates
        // a new row or passes through the legacy infallible bridge.
        assert_eq!(control.borrow().calls, before + 1);
        assert_eq!(control.borrow().infallible, 0);
        links.try_create(0, 0)?;
        assert_eq!(links.get_link(1), Some(Link::new(1, 1, 2)));
        assert!(links.try_ensure_created(0).is_err());
        assert!(links.try_ensure_created(u32::MAX).is_err());
        links.save()?;
    }
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn fallible_allocations_surface_a_parked_trigger_error_once() -> Result<()> {
    for operation in ["create", "ensure", "pair"] {
        let (_data_files, inner) = test_store::named()?;
        let (_trigger_files, schema) = test_store::named()?;
        let mut links = PersistentTransformationDecorator::with_sidecar(inner, schema, false);
        let query = "(((1: * *)) ((1: 1 7)))";
        links.store_trigger(PersistentTransformationKind::Always, query)?;
        assert_eq!(NamedTypeLinks::create(&mut links, 0, 0), 1);
        assert_eq!(links.get_link(1), Some(Link::new(1, 0, 0)));
        // Removing the failing trigger does not discard an already reported
        // operation's parked error. The next fallible bridge must return it.
        assert_eq!(links.remove_triggers(query)?, 1);
        let outcome = match operation {
            "create" => links.try_create(0, 0),
            "ensure" => links.try_ensure_created(1),
            "pair" => links.try_get_or_create(0, 0),
            _ => unreachable!(),
        };
        assert!(outcome
            .unwrap_err()
            .to_string()
            .contains("non-existent link '7'"));
        assert_eq!(links.try_ensure_created(1)?, 1);
        links.save()?;
    }
    Ok(())
}

fn import_fault(operation: &str, fail_at: usize) -> Result<()> {
    let (_files, mut inner) = test_store::named()?;
    let seed = inner.try_create(1, 1)?;
    assert_eq!(seed, 1);
    inner.set_name(seed, "seed 雪")?;
    let control = Rc::new(RefCell::new(Control {
        fail_at: Some(fail_at),
        ..Default::default()
    }));
    let mut links = PersistentTransformationDecorator::embedded(
        Store {
            inner,
            control: control.clone(),
        },
        false,
    );
    let before = links.all_links();
    let result = import_operation::run(&mut links, operation);
    assert!(
        result.is_err(),
        "{operation}: must return allocation fault {fail_at}"
    );
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("injected allocation"));
    assert_eq!(control.borrow().calls, fail_at);
    assert_eq!(control.borrow().infallible, 0, "{operation}: wrong bridge");
    assert_eq!(links.get_link(seed), Some(Link::new(seed, seed, seed)));
    assert_eq!(links.get_name(seed)?.as_deref(), Some("seed 雪"));
    if fail_at == 1 {
        assert_eq!(links.all_links(), before);
    }
    control.borrow_mut().fail_at = None;
    import_operation::run(&mut links, operation)?;
    let mut imported = links.all_links();
    imported.sort_by_key(|row| row.index);
    let allocations = control.borrow().calls;
    import_operation::run(&mut links, operation)?;
    let mut repeated = links.all_links();
    repeated.sort_by_key(|row| row.index);
    assert_eq!(repeated, imported, "retry preserves addresses and rows");
    assert_eq!(
        control.borrow().calls,
        allocations,
        "retry allocates nothing"
    );
    assert_eq!(control.borrow().infallible, 0);
    match operation {
        "import-explicit" | "import-archive" => {
            assert_eq!(links.get_link(8), Some(Link::new(8, 1, 8)));
            if operation == "import-archive" {
                assert_eq!(links.get_name(8)?.as_deref(), Some("imported 雪"));
            }
        }
        "import-nested" => {
            let nested = links.search(1, 8).expect("anonymous nested pair");
            assert_eq!(links.get_link(8), Some(Link::new(8, nested, 1)));
        }
        "import-named" => {
            let id = links.get_by_name("imported 雪")?.expect("imported name");
            assert_eq!(links.get_link(id), Some(Link::new(id, 1, 0)));
        }
        _ => unreachable!(),
    }
    let following = links.try_create(0, 0)?;
    links.set_name(following, "after import=雪\0")?;
    links.save()?;
    assert_eq!(
        links.get_name(following)?.as_deref(),
        Some("after import=雪\0")
    );
    Ok(())
}

macro_rules! import_case {
    ($name:ident, $operation:literal, $fault:literal) => {
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        fn $name() -> Result<()> {
            import_fault($operation, $fault)
        }
    };
}
import_case!(
    import_explicit_allocation_returns_errors,
    "import-explicit",
    1
);
import_case!(import_nested_allocation_returns_errors, "import-nested", 1);
import_case!(
    import_nested_later_allocation_returns_errors,
    "import-nested",
    2
);
import_case!(
    import_archive_allocation_returns_errors,
    "import-archive",
    1
);
import_case!(import_named_allocation_control, "import-named", 1);

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn import_reallocation_after_trigger_deletion_returns_errors() -> Result<()> {
    let (_files, mut inner) = test_store::named()?;
    inner.try_create(1, 1)?;
    let (_schema_files, schema) = test_store::named()?;
    let control = Rc::new(RefCell::new(Control {
        fail_at: Some(2),
        ..Default::default()
    }));
    let mut links = PersistentTransformationDecorator::with_sidecar(
        Store {
            inner,
            control: control.clone(),
        },
        schema,
        false,
    );
    // The first allocation succeeds, but the selected trigger removes its row.
    // The import update must propagate a fault while reserving that row again.
    links.store_trigger(PersistentTransformationKind::Once, "(((8: * *)) ())")?;
    let result = import_operation::run(&mut links, "import-explicit");
    assert!(result.is_err(), "reallocation must return the second fault");
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("injected allocation"));
    assert_eq!(control.borrow().calls, 2);
    assert_eq!(control.borrow().infallible, 0);
    assert!(!links.exists(8));
    assert!(links.triggers()?.is_empty());
    control.borrow_mut().fail_at = None;
    import_operation::run(&mut links, "import-explicit")?;
    assert_eq!(links.get_link(8), Some(Link::new(8, 1, 8)));
    links.save()
}
