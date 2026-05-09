//! Link-backed names facade matching the C# `NamedLinks<uint>` role.

use anyhow::Result;

use crate::link_storage::LinkStorage;
use crate::unicode_string_storage::UnicodeStringStorage;

pub struct NamedLinks<'a> {
    storage: UnicodeStringStorage<'a>,
}

impl<'a> NamedLinks<'a> {
    pub fn new(links: &'a mut LinkStorage) -> Result<Self> {
        Ok(UnicodeStringStorage::new(links)?.into_named_links())
    }

    pub(crate) fn from_storage(storage: UnicodeStringStorage<'a>) -> Self {
        Self { storage }
    }

    pub fn set_name_for_external_reference(&mut self, link: u32, name: &str) -> Result<u32> {
        self.storage.set_name_for_external_reference(link, name)
    }

    pub fn set_name(&mut self, link: u32, name: &str) -> Result<u32> {
        self.storage.set_name(link, name)
    }

    pub fn get_name_by_external_reference(&self, link: u32) -> Result<Option<String>> {
        self.storage.get_name_by_external_reference(link)
    }

    pub fn get_name(&self, link: u32) -> Result<Option<String>> {
        self.storage.get_name(link)
    }

    pub fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        self.storage.get_by_name(name)
    }

    pub fn get_external_reference_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        self.storage.get_external_reference_by_name(name)
    }

    pub fn remove_name(&mut self, link: u32) -> Result<()> {
        self.storage.remove_name(link)
    }

    pub fn remove_name_by_external_reference(&mut self, external_reference_id: u32) -> Result<()> {
        self.storage
            .remove_name_by_external_reference(external_reference_id)
    }

    pub fn unicode_storage(&self) -> &UnicodeStringStorage<'a> {
        &self.storage
    }

    pub fn unicode_storage_mut(&mut self) -> &mut UnicodeStringStorage<'a> {
        &mut self.storage
    }
}
