//! Checked named/query facade over the sealed u64 workspace. The wider query
//! marker can never equal a stored identity or reference.
use super::causal_session::CausalWorkspace;
use crate::{
    link_storage::ChangeObserver, query_options::QueryLimits, GenericLink, LinkError, LinksStorage,
    NamedTypeLinks,
};
use anyhow::Result;

pub struct CausalNamedLinks<'a> {
    pub(super) workspace: &'a mut CausalWorkspace,
}

fn address(value: u128) -> Result<u64> {
    Ok(u64::try_from(value).map_err(|_| LinkError::AddressOutOfRange(value))?)
}

fn wide(link: GenericLink<u64>) -> GenericLink<u128> {
    GenericLink::new(link.index.into(), link.source.into(), link.target.into())
}

impl NamedTypeLinks<u128> for CausalNamedLinks<'_> {
    fn query_limits(&self) -> Option<QueryLimits> {
        Some(QueryLimits::causal())
    }
    fn reserve_query_allocations(
        &mut self,
        processor: &crate::GenericQueryProcessor<u128>,
        restriction: &crate::lino_link::LinoLink,
        substitution: &crate::lino_link::LinoLink,
    ) -> Result<Option<std::collections::HashSet<u128>>> {
        self.workspace
            .reserve_query(processor, restriction, substitution)
    }
    fn create(&mut self, source: u128, target: u128) -> u128 {
        self.try_create(source, target)
            .expect("allocation failed; use try_create")
    }
    fn try_create(&mut self, source: u128, target: u128) -> Result<u128> {
        let source = address(source)?;
        let target = address(target)?;
        Ok(self.workspace.create_link(source, target)?.into())
    }
    fn ensure_created(&mut self, id: u128) -> u128 {
        self.try_ensure_created(id)
            .expect("allocation failed; use try_ensure_created")
    }
    fn try_ensure_created(&mut self, id: u128) -> Result<u128> {
        Ok(self.workspace.ensure_link_created(address(id)?)?.into())
    }
    fn get_link(&mut self, id: u128) -> Option<GenericLink<u128>> {
        self.workspace.get_link(u64::try_from(id).ok()?).map(wide)
    }
    fn exists(&mut self, id: u128) -> bool {
        self.get_link(id).is_some()
    }
    fn update_observed(
        &mut self,
        id: u128,
        source: u128,
        target: u128,
        observer: ChangeObserver<'_, u128>,
    ) -> Result<GenericLink<u128>> {
        let id = address(id)?;
        let source = address(source)?;
        let target = address(target)?;
        let before = wide(self.workspace.update_link(id, source, target)?);
        observer(before, wide(GenericLink::new(id, source, target)));
        Ok(before)
    }
    fn delete_observed(
        &mut self,
        id: u128,
        observer: ChangeObserver<'_, u128>,
    ) -> Result<GenericLink<u128>> {
        let before = wide(self.workspace.delete_link(address(id)?)?);
        observer(before, GenericLink::null());
        Ok(before)
    }
    fn all_links(&mut self) -> Vec<GenericLink<u128>> {
        self.workspace.all_links().into_iter().map(wide).collect()
    }
    fn search(&mut self, source: u128, target: u128) -> Option<u128> {
        self.all_links()
            .into_iter()
            .filter(|link| link.source == source && link.target == target)
            .map(|link| link.index)
            .min()
    }
    fn get_or_create(&mut self, source: u128, target: u128) -> u128 {
        self.try_get_or_create(source, target)
            .expect("allocation failed; use try_get_or_create")
    }
    fn get_name(&mut self, id: u128) -> Result<Option<String>> {
        Ok(self.workspace.get_name(address(id)?))
    }
    fn format_reference(&mut self, id: u128) -> Result<String> {
        Ok(self
            .get_name(id)?
            .map(|name| links_notation::encode_reference_literal(&name))
            .unwrap_or_else(|| id.to_string()))
    }
    fn set_name(&mut self, id: u128, name: &str) -> Result<u128> {
        self.workspace
            .set_name(address(id)?, Some(name.to_owned()))?;
        Ok(id)
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<u128>> {
        Ok(self.workspace.get_by_name(name).map(u128::from))
    }
    fn remove_name(&mut self, id: u128) -> Result<()> {
        Ok(self.workspace.set_name(address(id)?, None)?)
    }
    fn save(&mut self) -> Result<()> {
        Ok(self.workspace.query_save()?)
    }
}
