//! Adapter to the CLI/query/protocol surface. The raw engine's constraints
//! remain commit-time policies; these operations do not merge or cascade.
use super::EngineTransaction;
use crate::{Link, LinksStorage, NamedTypeLinks, TransitionLogStore};
use anyhow::Result;

impl<L: TransitionLogStore> NamedTypeLinks for EngineTransaction<u32, L> {
    fn create(&mut self, source: u32, target: u32) -> u32 {
        self.create_link(source, target)
            .expect("allocation failed; use try_create")
    }
    fn try_create(&mut self, source: u32, target: u32) -> Result<u32> {
        Ok(self.create_link(source, target)?)
    }
    fn ensure_created(&mut self, id: u32) -> u32 {
        self.ensure_link_created(id)
            .expect("allocation failed; use try_ensure_created")
    }
    fn try_ensure_created(&mut self, id: u32) -> Result<u32> {
        Ok(self.ensure_link_created(id)?)
    }
    fn get_link(&mut self, id: u32) -> Option<Link> {
        LinksStorage::get_link(self, id)
    }
    fn exists(&mut self, id: u32) -> bool {
        self.link_exists(id)
    }
    fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: crate::link_storage::ChangeObserver<'_>,
    ) -> Result<Link> {
        let before = self.update_link(id, source, target)?;
        observer(before, Link::new(id, source, target));
        Ok(before)
    }
    fn delete_observed(
        &mut self,
        id: u32,
        observer: crate::link_storage::ChangeObserver<'_>,
    ) -> Result<Link> {
        let before = self.delete_link(id)?;
        observer(before, Link::null());
        Ok(before)
    }
    fn all_links(&mut self) -> Vec<Link> {
        LinksStorage::all_links(self)
    }
    fn search(&mut self, source: u32, target: u32) -> Option<u32> {
        self.search_link(source, target)
    }
    fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        self.get_or_create_link(source, target)
            .expect("allocation failed; use fallible LinksStorage")
    }
    fn get_name(&mut self, id: u32) -> Result<Option<String>> {
        Ok(EngineTransaction::get_name(self, id))
    }
    fn set_name(&mut self, id: u32, name: &str) -> Result<u32> {
        EngineTransaction::set_name(self, id, Some(name.into()))?;
        Ok(id)
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        Ok(EngineTransaction::get_by_name(self, name))
    }
    fn remove_name(&mut self, id: u32) -> Result<()> {
        Ok(EngineTransaction::set_name(self, id, None)?)
    }
    fn save(&mut self) -> Result<()> {
        // The protocol saves after individual writes. A session must keep
        // them private until its caller explicitly commits the whole batch.
        if self.closed {
            anyhow::bail!("transaction is closed");
        }
        Ok(())
    }
}
