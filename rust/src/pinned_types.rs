//! Pinned type allocation compatible with the C# `PinnedTypes` helper.

use anyhow::{bail, Result};
use std::path::Path;

use crate::link::Link;
use crate::link_storage::LinkStorage;

pub trait PinnedTypesAccess {
    fn pinned_types(&mut self, count: usize) -> Result<Vec<u32>>;

    fn deconstruct_pinned_types(&mut self) -> Result<(u32, u32, u32)> {
        let pinned_types = self.pinned_types(3)?;
        Ok((pinned_types[0], pinned_types[1], pinned_types[2]))
    }
}

/// Creates or validates the reserved type links at deterministic addresses.
pub struct PinnedTypes<'a> {
    links: &'a mut LinkStorage,
    current: u32,
    initial_source: u32,
}

impl<'a> PinnedTypes<'a> {
    pub fn new(links: &'a mut LinkStorage) -> Self {
        Self {
            links,
            current: 1,
            initial_source: 1,
        }
    }

    pub fn next_type(&mut self) -> Result<u32> {
        let address = self.current;
        if let Some(link) = self.links.get(address) {
            if link.index != address || link.source != self.initial_source || link.target != address
            {
                bail!(
                    "Unexpected link found at address {address}. Expected: ({address}: {} {address}), Found: ({}: {} {}).",
                    self.initial_source,
                    link.index,
                    link.source,
                    link.target
                );
            }
        } else {
            let created = self.links.get_or_create(self.initial_source, address);
            if created != address {
                bail!(
                    "Pinned type address {address} could not be created deterministically; got {created}."
                );
            }
        }

        self.current += 1;
        Ok(address)
    }

    pub fn take_types(&mut self, count: usize) -> Result<Vec<u32>> {
        (0..count).map(|_| self.next_type()).collect()
    }

    pub fn deconstruct(&mut self) -> Result<(u32, u32, u32)> {
        let pinned_types = self.take_types(3)?;
        Ok((pinned_types[0], pinned_types[1], pinned_types[2]))
    }
}

/// Link storage decorator that exposes pinned type allocation.
pub struct PinnedTypesDecorator {
    links: LinkStorage,
}

impl PinnedTypesDecorator {
    pub fn new<P>(database_filename: P, trace: bool) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let database_path = database_filename.as_ref().to_string_lossy().into_owned();
        let links = LinkStorage::new(&database_path, trace)?;
        Ok(Self::from_link_storage(links))
    }

    pub fn from_link_storage(links: LinkStorage) -> Self {
        Self { links }
    }

    pub fn links(&self) -> &LinkStorage {
        &self.links
    }

    pub fn links_mut(&mut self) -> &mut LinkStorage {
        &mut self.links
    }

    pub fn into_link_storage(self) -> LinkStorage {
        self.links
    }

    pub fn save(&self) -> Result<()> {
        self.links.save()
    }

    pub fn create(&mut self, source: u32, target: u32) -> u32 {
        self.links.create(source, target)
    }

    pub fn ensure_created(&mut self, id: u32) -> u32 {
        self.links.ensure_created(id)
    }

    pub fn get(&self, id: u32) -> Option<&Link> {
        self.links.get(id)
    }

    pub fn exists(&self, id: u32) -> bool {
        self.links.exists(id)
    }

    pub fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link> {
        self.links.update(id, source, target)
    }

    pub fn delete(&mut self, id: u32) -> Result<Link> {
        self.links.delete(id)
    }

    pub fn all(&self) -> Vec<&Link> {
        self.links.all()
    }

    pub fn query(
        &self,
        index: Option<u32>,
        source: Option<u32>,
        target: Option<u32>,
    ) -> Vec<&Link> {
        self.links.query(index, source, target)
    }

    pub fn search(&self, source: u32, target: u32) -> Option<u32> {
        self.links.search(source, target)
    }

    pub fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        self.links.get_or_create(source, target)
    }
}

impl PinnedTypesAccess for PinnedTypesDecorator {
    fn pinned_types(&mut self, count: usize) -> Result<Vec<u32>> {
        PinnedTypes::new(&mut self.links).take_types(count)
    }
}
