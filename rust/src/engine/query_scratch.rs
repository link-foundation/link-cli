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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(next: u128, last: u128) -> QueryScratch<u32> {
        QueryScratch {
            rows: BTreeMap::new(),
            next,
            last,
            reserved: VecDeque::new(),
            allocated: Vec::new(),
            fresh_range: None,
            limits: None,
        }
    }

    #[test]
    fn the_panicking_writes_are_the_checked_ones() {
        let mut scratch = scratch(1, 10);
        let made = scratch.create(0, 0);
        assert_eq!(made, 1);
        assert_eq!(scratch.ensure_created(5), 5);
        assert_eq!(scratch.next, 6);
        let pair = scratch.get_or_create(5, 5);
        assert_eq!(scratch.get_or_create(5, 5), pair);
    }

    #[test]
    fn a_name_is_set_and_removed_only_on_a_row() {
        let mut scratch = scratch(1, 10);
        let made = scratch.create(0, 0);
        scratch.set_name(made, "name").unwrap();
        assert_eq!(scratch.get_by_name("name").unwrap(), Some(made));
        scratch.remove_name(made).unwrap();
        assert_eq!(scratch.get_name(made).unwrap(), None);
        assert!(scratch.remove_name(9).is_err());
    }

    #[test]
    fn the_address_space_and_the_reservation_bound_what_is_made() {
        let mut full = scratch(3, 2);
        assert!(full.try_create(0, 0).is_err());
        let mut taken = scratch(1, 10);
        taken.reserved.push_back(4);
        taken.rows.insert(
            4,
            Row {
                link: GenericLink::new(4, 0, 0),
                name: None,
            },
        );
        assert!(taken.try_create(0, 0).is_err());

        let mut ranged = scratch(1, 10);
        assert!(ranged.try_ensure_created(0).is_err());
        assert!(ranged.try_ensure_created(u32::MAX).is_err());
        ranged.fresh_range = Some((5, 6, BTreeSet::from([2])));
        assert!(ranged.try_ensure_created(9).is_err());
        // A retained version outside the range is allowed and does not move the
        // allocator when its row is already there.
        ranged.rows.insert(
            2,
            Row {
                link: GenericLink::new(2, 0, 0),
                name: None,
            },
        );
        ranged.next = 2;
        assert_eq!(ranged.try_ensure_created(2).unwrap(), 2);
        assert_eq!(ranged.next, 2);
        // An address of the fresh range moves it even with a row present.
        ranged.rows.insert(
            6,
            Row {
                link: GenericLink::new(6, 0, 0),
                name: None,
            },
        );
        assert_eq!(ranged.try_ensure_created(6).unwrap(), 6);
        assert_eq!(ranged.next, 7);
    }
}
