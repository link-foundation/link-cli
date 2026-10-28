//! Private allocation preview using the ordinary query execution path.
//! Only owned rows are changed. No journal, observer, or engine handle escapes.
use super::Row;
use crate::{
    link_storage::ChangeObserver, query_address::QueryAddress, query_options::QueryLimits,
    GenericLink, NamedTypeLinks,
};
use anyhow::{bail, Result};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(super) struct QueryScratch<T: QueryAddress> {
    pub rows: BTreeMap<T, Row<T>>,
    pub next: u128,
    pub last: u128,
    pub reserved: VecDeque<T>,
    pub allocated: Vec<T>,
    pub fresh_range: Option<(T, T, BTreeSet<T>)>,
    pub limits: Option<QueryLimits>,
}

impl<T: QueryAddress> NamedTypeLinks<T> for QueryScratch<T> {
    fn query_limits(&self) -> Option<QueryLimits> {
        self.limits
    }
    fn create(&mut self, source: T, target: T) -> T {
        self.try_create(source, target).expect("scratch allocation")
    }
    fn try_create(&mut self, source: T, target: T) -> Result<T> {
        let id = if let Some(id) = self.reserved.front().copied() {
            id
        } else {
            if self.next > self.last {
                bail!("query address space exhausted");
            }
            T::try_from(self.next).map_err(|_| anyhow::anyhow!("query address out of range"))?
        };
        if self.rows.contains_key(&id) {
            bail!("reserved query address already exists");
        }
        if self.reserved.front() == Some(&id) {
            self.reserved.pop_front();
        } else {
            self.next = self
                .next
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("query address space exhausted"))?;
        }
        self.rows.insert(
            id,
            Row {
                link: GenericLink::new(id, source, target),
                name: None,
            },
        );
        self.allocated.push(id);
        Ok(id)
    }
    fn ensure_created(&mut self, id: T) -> T {
        self.try_ensure_created(id).expect("scratch ensure")
    }
    fn try_ensure_created(&mut self, id: T) -> Result<T> {
        if id == T::from_byte(0) || id == T::MAX {
            bail!("unsupported query address");
        }
        if let Some((first, last, retained)) = &self.fresh_range {
            if !(*first..=*last).contains(&id) && !retained.contains(&id) {
                bail!("query address is outside the reservation and retained versions");
            }
        }
        let missing = !self.rows.contains_key(&id);
        if missing
            || self
                .fresh_range
                .as_ref()
                .is_some_and(|(first, last, _)| (*first..=*last).contains(&id))
        {
            let address: u128 = id
                .try_into()
                .map_err(|_| anyhow::anyhow!("query address conversion failed"))?;
            self.next = self.next.max(
                address
                    .checked_add(1)
                    .ok_or_else(|| anyhow::anyhow!("query address space exhausted"))?,
            );
        }
        self.rows.entry(id).or_insert_with(|| Row {
            link: GenericLink::new(id, T::from_byte(0), T::from_byte(0)),
            name: None,
        });
        self.reserved.retain(|pending| *pending != id);
        Ok(id)
    }
    fn get_link(&mut self, id: T) -> Option<GenericLink<T>> {
        self.rows.get(&id).map(|row| row.link)
    }
    fn exists(&mut self, id: T) -> bool {
        self.rows.contains_key(&id)
    }
    fn update_observed(
        &mut self,
        id: T,
        source: T,
        target: T,
        observer: ChangeObserver<'_, T>,
    ) -> Result<GenericLink<T>> {
        let row = self
            .rows
            .get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("query link does not exist"))?;
        let before = row.link;
        row.link = GenericLink::new(id, source, target);
        observer(before, row.link);
        Ok(before)
    }
    fn delete_observed(
        &mut self,
        id: T,
        observer: ChangeObserver<'_, T>,
    ) -> Result<GenericLink<T>> {
        let row = self
            .rows
            .remove(&id)
            .ok_or_else(|| anyhow::anyhow!("query link does not exist"))?;
        observer(row.link, GenericLink::null());
        Ok(row.link)
    }
    fn all_links(&mut self) -> Vec<GenericLink<T>> {
        self.rows.values().map(|row| row.link).collect()
    }
    fn search(&mut self, source: T, target: T) -> Option<T> {
        self.rows
            .values()
            .find(|row| row.link.source == source && row.link.target == target)
            .map(|row| row.link.index)
    }
    fn get_or_create(&mut self, source: T, target: T) -> T {
        self.try_get_or_create(source, target)
            .expect("scratch get or create")
    }
    fn get_name(&mut self, id: T) -> Result<Option<String>> {
        Ok(self.rows.get(&id).and_then(|row| row.name.clone()))
    }
    fn set_name(&mut self, id: T, name: &str) -> Result<T> {
        self.rows
            .get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("query link does not exist"))?
            .name = Some(name.into());
        Ok(id)
    }
    fn get_by_name(&mut self, name: &str) -> Result<Option<T>> {
        Ok(self
            .rows
            .values()
            .find(|row| row.name.as_deref() == Some(name))
            .map(|row| row.link.index))
    }
    fn remove_name(&mut self, id: T) -> Result<()> {
        self.rows
            .get_mut(&id)
            .ok_or_else(|| anyhow::anyhow!("query link does not exist"))?
            .name = None;
        Ok(())
    }
    fn save(&mut self) -> Result<()> {
        Ok(())
    }
}
