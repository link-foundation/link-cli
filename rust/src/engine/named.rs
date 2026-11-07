//! Adapter to the CLI/query/protocol surface. The raw engine's constraints
//! remain commit-time policies; these operations do not merge or cascade.
use super::EngineTransaction;
use crate::query_address::QueryAddress;
use crate::{GenericLink as Link, LinksStorage, NamedTypeLinks, TransitionLogStore};
use anyhow::Result;

impl<T: QueryAddress, L: TransitionLogStore> NamedTypeLinks<T> for EngineTransaction<T, L> {
    fn reserve_query_allocations(
        &mut self,
        processor: &crate::GenericQueryProcessor<T>,
        restriction: &crate::lino_link::LinoLink,
        substitution: &crate::lino_link::LinoLink,
    ) -> Result<Option<std::collections::HashSet<T>>> {
        self.reserve_query(processor, restriction, substitution)
    }
    fn create(&mut self, source: T, target: T) -> T {
        self.create_link(source, target)
            .expect("allocation failed; use try_create")
    }
    fn try_create(&mut self, source: T, target: T) -> Result<T> {
        Ok(self.create_link(source, target)?)
    }
    fn ensure_created(&mut self, id: T) -> T {
        self.ensure_link_created(id)
            .expect("allocation failed; use try_ensure_created")
    }
    fn try_ensure_created(&mut self, id: T) -> Result<T> {
        Ok(self.ensure_link_created(id)?)
    }
    fn get_link(&mut self, id: T) -> Option<Link<T>> {
        LinksStorage::get_link(self, id)
    }
    fn exists(&mut self, id: T) -> bool {
        self.link_exists(id)
    }
    fn update_observed(
        &mut self,
        id: T,
        source: T,
        target: T,
        observer: crate::link_storage::ChangeObserver<'_, T>,
    ) -> Result<Link<T>> {
        let before = self.update_link(id, source, target)?;
        observer(before, Link::new(id, source, target));
        Ok(before)
    }
    fn delete_observed(
        &mut self,
        id: T,
        observer: crate::link_storage::ChangeObserver<'_, T>,
    ) -> Result<Link<T>> {
        let before = self.delete_link(id)?;
        observer(before, Link::null());
        Ok(before)
    }
    fn all_links(&mut self) -> Vec<Link<T>> {
        LinksStorage::all_links(self)
    }
    fn search(&mut self, source: T, target: T) -> Option<T> {
        self.search_link(source, target)
    }
    fn get_or_create(&mut self, source: T, target: T) -> T {
        self.get_or_create_link(source, target)
            .expect("allocation failed; use fallible LinksStorage")
    }
    fn get_name(&mut self, id: T) -> Result<Option<String>> {
        Ok(EngineTransaction::get_name(self, id))
    }
    fn set_name(&mut self, id: T, name: &str) -> Result<T> {
        EngineTransaction::set_name(self, id, Some(name.into()))?;
        Ok(id)
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<T>> {
        Ok(EngineTransaction::get_by_name(self, name))
    }
    fn remove_name(&mut self, id: T) -> Result<()> {
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

impl<T: QueryAddress> NamedTypeLinks<T> for super::cluster::ShardSession<T> {
    fn reserve_query_allocations(
        &mut self,
        processor: &crate::GenericQueryProcessor<T>,
        restriction: &crate::lino_link::LinoLink,
        substitution: &crate::lino_link::LinoLink,
    ) -> Result<Option<std::collections::HashSet<T>>> {
        self.session
            .reserve_query_allocations(processor, restriction, substitution)
    }
    fn create(&mut self, source: T, target: T) -> T {
        NamedTypeLinks::create(&mut self.session, source, target)
    }
    fn try_create(&mut self, source: T, target: T) -> Result<T> {
        NamedTypeLinks::try_create(&mut self.session, source, target)
    }
    fn ensure_created(&mut self, id: T) -> T {
        NamedTypeLinks::ensure_created(&mut self.session, id)
    }
    fn try_ensure_created(&mut self, id: T) -> Result<T> {
        NamedTypeLinks::try_ensure_created(&mut self.session, id)
    }
    fn get_link(&mut self, id: T) -> Option<Link<T>> {
        NamedTypeLinks::get_link(&mut self.session, id)
    }
    fn exists(&mut self, id: T) -> bool {
        NamedTypeLinks::exists(&mut self.session, id)
    }
    fn update_observed(
        &mut self,
        id: T,
        source: T,
        target: T,
        observer: crate::link_storage::ChangeObserver<'_, T>,
    ) -> Result<Link<T>> {
        NamedTypeLinks::update_observed(&mut self.session, id, source, target, observer)
    }
    fn delete_observed(
        &mut self,
        id: T,
        observer: crate::link_storage::ChangeObserver<'_, T>,
    ) -> Result<Link<T>> {
        NamedTypeLinks::delete_observed(&mut self.session, id, observer)
    }
    fn all_links(&mut self) -> Vec<Link<T>> {
        NamedTypeLinks::all_links(&mut self.session)
    }
    fn search(&mut self, source: T, target: T) -> Option<T> {
        NamedTypeLinks::search(&mut self.session, source, target)
    }
    fn get_or_create(&mut self, source: T, target: T) -> T {
        NamedTypeLinks::get_or_create(&mut self.session, source, target)
    }
    fn get_name(&mut self, id: T) -> Result<Option<String>> {
        NamedTypeLinks::get_name(&mut self.session, id)
    }
    fn set_name(&mut self, id: T, name: &str) -> Result<T> {
        NamedTypeLinks::set_name(&mut self.session, id, name)
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<T>> {
        NamedTypeLinks::get_by_name(&mut self.session, name)
    }
    fn remove_name(&mut self, id: T) -> Result<()> {
        NamedTypeLinks::remove_name(&mut self.session, id)
    }
    fn save(&mut self) -> Result<()> {
        NamedTypeLinks::save(&mut self.session)
    }
}
