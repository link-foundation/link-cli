//! Named type decorator for Rust link storage.
//!
//! This mirrors the C# `NamedTypesDecorator<uint>` role: link operations are
//! delegated to the wrapped `LinkStorage`, while names are stored as external
//! references in a separate links database.

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::link::Link;
use crate::link_storage::LinkStorage;
use crate::named_links::NamedLinks;

pub trait NamedTypes {
    fn get_name(&mut self, link: u32) -> Result<Option<String>>;
    fn set_name(&mut self, link: u32, name: &str) -> Result<u32>;
    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>>;
    fn remove_name(&mut self, link: u32) -> Result<()>;
}

pub struct NamedTypesDecorator {
    links: LinkStorage,
    names_links: LinkStorage,
    trace: bool,
}

impl NamedTypesDecorator {
    pub fn new<P>(database_filename: P, trace: bool) -> Result<Self>
    where
        P: AsRef<Path>,
    {
        let names_database_filename =
            Self::make_names_database_filename(database_filename.as_ref());
        Self::with_names_database_path(database_filename, names_database_filename, trace)
    }

    pub fn with_names_database_path<P, N>(
        database_filename: P,
        names_database_filename: N,
        trace: bool,
    ) -> Result<Self>
    where
        P: AsRef<Path>,
        N: AsRef<Path>,
    {
        let database_path = path_to_string(database_filename.as_ref());
        let names_database_path = path_to_string(names_database_filename.as_ref());
        let links = LinkStorage::new(&database_path, trace)?;
        let names_links = LinkStorage::new(&names_database_path, trace)?;
        Ok(Self::from_link_storages_with_trace(
            links,
            names_links,
            trace,
        ))
    }

    pub fn from_link_storages(links: LinkStorage, names_links: LinkStorage) -> Self {
        Self::from_link_storages_with_trace(links, names_links, false)
    }

    pub fn make_names_database_filename<P>(database_filename: P) -> PathBuf
    where
        P: AsRef<Path>,
    {
        let path = database_filename.as_ref();
        let filename_without_extension = path
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let names_filename = format!("{filename_without_extension}.names.links");

        path.parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .map(|parent| parent.join(&names_filename))
            .unwrap_or_else(|| PathBuf::from(names_filename))
    }

    pub fn links(&self) -> &LinkStorage {
        &self.links
    }

    pub fn links_mut(&mut self) -> &mut LinkStorage {
        &mut self.links
    }

    pub fn names_links(&self) -> &LinkStorage {
        &self.names_links
    }

    pub fn names_links_mut(&mut self) -> &mut LinkStorage {
        &mut self.names_links
    }

    pub fn into_link_storages(self) -> (LinkStorage, LinkStorage) {
        (self.links, self.names_links)
    }

    pub fn save(&self) -> Result<()> {
        self.links.save()?;
        self.names_links.save()?;
        Ok(())
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
        let deleted = self.links.delete(id)?;
        self.remove_name(id)?;
        Ok(deleted)
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

    fn from_link_storages_with_trace(
        links: LinkStorage,
        names_links: LinkStorage,
        trace: bool,
    ) -> Self {
        Self {
            links,
            names_links,
            trace,
        }
    }

    fn with_named_links<T>(
        &mut self,
        action: impl FnOnce(&mut NamedLinks<'_>) -> Result<T>,
    ) -> Result<T> {
        let mut named_links = NamedLinks::new(&mut self.names_links)?;
        action(&mut named_links)
    }
}

impl NamedTypes for NamedTypesDecorator {
    fn get_name(&mut self, link: u32) -> Result<Option<String>> {
        if self.trace {
            eprintln!("[TRACE] NamedTypesDecorator get_name for link {link}");
        }
        self.with_named_links(|named_links| named_links.get_name_by_external_reference(link))
    }

    fn set_name(&mut self, link: u32, name: &str) -> Result<u32> {
        if self.trace {
            eprintln!("[TRACE] NamedTypesDecorator set_name for link {link}: {name}");
        }
        if let Some(existing_link_with_name) = self.get_by_name(name)? {
            if existing_link_with_name != link {
                self.remove_name(existing_link_with_name)?;
            }
        }
        self.remove_name(link)?;
        self.with_named_links(|named_links| named_links.set_name_for_external_reference(link, name))
    }

    fn get_by_name(&mut self, name: &str) -> Result<Option<u32>> {
        if self.trace {
            eprintln!("[TRACE] NamedTypesDecorator get_by_name for name {name}");
        }
        self.with_named_links(|named_links| named_links.get_external_reference_by_name(name))
    }

    fn remove_name(&mut self, link: u32) -> Result<()> {
        if self.trace {
            eprintln!("[TRACE] NamedTypesDecorator remove_name for link {link}");
        }
        self.with_named_links(|named_links| named_links.remove_name_by_external_reference(link))
    }
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}
