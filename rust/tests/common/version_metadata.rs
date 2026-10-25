//! Shared portable faults for version sidecar persistence.
use anyhow::{bail, Result};
use link_cli::{Link, NamedTypeLinks, NamedTypesDecorator};
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Default)]
pub(crate) struct Fault {
    pub(crate) point: Option<String>,
    pub(crate) flushes: usize,
    pub(crate) on_check: Option<fn(&str)>,
}
impl Fault {
    pub(crate) fn check(&mut self, point: &str) -> Result<()> {
        if let Some(on_check) = self.on_check {
            on_check(point);
        }
        if self.point.as_deref() == Some(point) {
            self.point = None;
            bail!("injected {point}");
        }
        Ok(())
    }
}
pub(crate) struct Metadata {
    pub(crate) inner: NamedTypesDecorator,
    pub(crate) fault: Rc<RefCell<Fault>>,
    pub(crate) intent: u32,
}
impl NamedTypeLinks for Metadata {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.inner.create(source, target)
    }
    fn ensure_created(&mut self, id: u32) -> u32 {
        self.inner.ensure_created(id)
    }
    fn get_link(&mut self, id: u32) -> Option<Link> {
        self.inner.get(id).copied()
    }
    fn exists(&mut self, id: u32) -> bool {
        self.inner.exists(id)
    }
    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: link_cli::link_storage::ChangeObserver<'_>,
    ) -> Result<Link> {
        self.inner.update_observed(id, source, target, observer)
    }
    fn delete_observed(
        &mut self,
        id: u32,
        observer: link_cli::link_storage::ChangeObserver<'_>,
    ) -> Result<Link> {
        let intent = id == self.intent;
        if intent {
            self.fault.borrow_mut().check("intent-delete-before")?;
        }
        let result = self.inner.delete_observed(id, observer)?;
        if intent {
            self.fault.borrow_mut().check("intent-delete-after")?;
        }
        Ok(result)
    }
    fn all_links(&mut self) -> Vec<Link> {
        self.inner.all().into_iter().copied().collect()
    }
    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        self.inner.search(source, target)
    }
    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        self.inner.get_or_create(source, target)
    }
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        let name = link_cli::NamedTypes::get_name(&mut self.inner, id)?;
        if name
            .as_deref()
            .is_some_and(|name| name.starts_with("__vc:pending:") || name.starts_with("__vc:move:"))
        {
            self.intent = id;
        }
        Ok(name)
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        link_cli::NamedTypes::get_by_name(&mut self.inner, name)
    }
    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        let phase = if name.starts_with("__vc:pending:") || name.starts_with("__vc:move:") {
            self.intent = id;
            Some("intent-write")
        } else if name.starts_with("__vc:trans:") {
            Some("transition")
        } else if name.starts_with("__vc:branch:") {
            Some("head")
        } else if name.starts_with("__vc:current=") {
            Some("current")
        } else if name.starts_with("__vc:applied=") {
            Some("applied")
        } else {
            None
        };
        if let Some(phase) = phase {
            self.fault.borrow_mut().check(&format!("{phase}-before"))?;
        }
        let result = link_cli::NamedTypes::set_name(&mut self.inner, id, name)?;
        if let Some(phase) = phase {
            self.fault.borrow_mut().check(&format!("{phase}-after"))?;
        }
        Ok(result)
    }
    fn remove_name(&mut self, id: u32) -> Result<()> {
        let intent = self.get_name(id)?.is_some_and(|name| {
            name.starts_with("__vc:pending:") || name.starts_with("__vc:move:")
        });
        if intent {
            self.fault.borrow_mut().check("intent-remove-before")?;
        }
        link_cli::NamedTypes::remove_name(&mut self.inner, id)?;
        if intent {
            self.fault.borrow_mut().check("intent-remove-after")?;
        }
        Ok(())
    }
    fn save(&mut self) -> Result<()> {
        let phase = {
            let mut fault = self.fault.borrow_mut();
            fault.flushes += 1;
            match fault.flushes {
                1 => "intent-flush",
                2 => "publication-flush",
                _ => "cleanup-flush",
            }
        };
        self.fault.borrow_mut().check("flush-before")?;
        self.fault.borrow_mut().check(&format!("{phase}-before"))?;
        self.inner.save()?;
        self.fault.borrow_mut().check(&format!("{phase}-after"))?;
        self.fault.borrow_mut().check("flush-after")
    }
}
