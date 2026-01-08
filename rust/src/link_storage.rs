//! LinkStorage - Persistent storage for links
//!
//! This module provides the LinkStorage struct for managing link persistence.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use crate::error::LinkError;
use crate::link::Link;

/// LinkStorage provides persistent storage for links
/// Corresponds to the storage functionality in NamedLinksDecorator in C#
pub struct LinkStorage {
    links: HashMap<u32, Link>,
    names: HashMap<u32, String>,
    name_to_id: HashMap<String, u32>,
    next_id: u32,
    db_path: String,
    trace: bool,
}

impl LinkStorage {
    /// Creates a new LinkStorage instance
    pub fn new(db_path: &str, trace: bool) -> Result<Self> {
        let mut storage = Self {
            links: HashMap::new(),
            names: HashMap::new(),
            name_to_id: HashMap::new(),
            next_id: 1,
            db_path: db_path.to_string(),
            trace,
        };

        // Load existing database if it exists
        if Path::new(db_path).exists() {
            storage.load()?;
        }

        Ok(storage)
    }

    /// Loads links from the database file
    fn load(&mut self) -> Result<()> {
        let file = File::open(&self.db_path)
            .with_context(|| format!("Failed to open database: {}", self.db_path))?;

        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse link format: (index source target) or (index source target "name")
            if let Some((link, name)) = self.parse_link_line(line) {
                self.links.insert(link.index, link);
                if link.index >= self.next_id {
                    self.next_id = link.index + 1;
                }
                if let Some(name) = name {
                    self.names.insert(link.index, name.clone());
                    self.name_to_id.insert(name, link.index);
                }
            }
        }

        if self.trace {
            eprintln!(
                "[TRACE] Loaded {} links from {}",
                self.links.len(),
                self.db_path
            );
        }

        Ok(())
    }

    /// Parses a single link line from the database
    fn parse_link_line(&self, line: &str) -> Option<(Link, Option<String>)> {
        // Simple format: (index source target) or (index source target "name")
        let line = line.trim_matches(|c| c == '(' || c == ')');
        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.len() >= 3 {
            let index = parts[0].parse().ok()?;
            let source = parts[1].parse().ok()?;
            let target = parts[2].parse().ok()?;
            let name = if parts.len() > 3 {
                Some(parts[3].trim_matches('"').to_string())
            } else {
                None
            };
            return Some((Link::new(index, source, target), name));
        }

        None
    }

    /// Saves all links to the database file
    pub fn save(&self) -> Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.db_path)
            .with_context(|| format!("Failed to create database: {}", self.db_path))?;

        let mut writer = BufWriter::new(file);

        // Sort by index for consistent output
        let mut links: Vec<_> = self.links.values().collect();
        links.sort_by_key(|l| l.index);

        for link in links {
            if let Some(name) = self.names.get(&link.index) {
                writeln!(
                    writer,
                    "({} {} {} \"{}\")",
                    link.index, link.source, link.target, name
                )?;
            } else {
                writeln!(writer, "({} {} {})", link.index, link.source, link.target)?;
            }
        }

        writer.flush()?;

        if self.trace {
            eprintln!(
                "[TRACE] Saved {} links to {}",
                self.links.len(),
                self.db_path
            );
        }

        Ok(())
    }

    /// Creates a new link and returns its ID
    pub fn create(&mut self, source: u32, target: u32) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        let link = Link::new(id, source, target);
        self.links.insert(id, link);

        if self.trace {
            eprintln!("[TRACE] Created link: ({} {} {})", id, source, target);
        }

        id
    }

    /// Creates a link with a specific ID, ensuring all links up to that ID exist
    pub fn ensure_created(&mut self, id: u32) -> u32 {
        if self.links.contains_key(&id) {
            return id;
        }

        // Create placeholder links up to the requested ID
        while self.next_id <= id {
            let placeholder_id = self.next_id;
            self.next_id += 1;
            if placeholder_id == id {
                let link = Link::new(id, 0, 0);
                self.links.insert(id, link);
                if self.trace {
                    eprintln!("[TRACE] Ensured link: ({} 0 0)", id);
                }
                return id;
            }
        }

        id
    }

    /// Gets a link by ID
    pub fn get(&self, id: u32) -> Option<&Link> {
        self.links.get(&id)
    }

    /// Checks if a link exists
    pub fn exists(&self, id: u32) -> bool {
        self.links.contains_key(&id)
    }

    /// Updates a link's source and target
    pub fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link> {
        if let Some(link) = self.links.get_mut(&id) {
            let before = *link;
            if self.trace {
                eprintln!(
                    "[TRACE] Updating link {} from ({} {}) to ({} {})",
                    id, link.source, link.target, source, target
                );
            }
            link.source = source;
            link.target = target;
            Ok(before)
        } else {
            Err(LinkError::NotFound(id).into())
        }
    }

    /// Deletes a link by ID
    pub fn delete(&mut self, id: u32) -> Result<Link> {
        // Also remove the name mapping
        if let Some(name) = self.names.remove(&id) {
            self.name_to_id.remove(&name);
        }

        if let Some(link) = self.links.remove(&id) {
            if self.trace {
                eprintln!(
                    "[TRACE] Deleted link: ({} {} {})",
                    link.index, link.source, link.target
                );
            }
            Ok(link)
        } else {
            Err(LinkError::NotFound(id).into())
        }
    }

    /// Returns all links
    pub fn all(&self) -> Vec<&Link> {
        self.links.values().collect()
    }

    /// Returns all links matching a query pattern
    pub fn query(
        &self,
        index: Option<u32>,
        source: Option<u32>,
        target: Option<u32>,
    ) -> Vec<&Link> {
        self.links
            .values()
            .filter(|link| {
                (index.is_none() || index == Some(link.index))
                    && (source.is_none() || source == Some(link.source))
                    && (target.is_none() || target == Some(link.target))
            })
            .collect()
    }

    /// Searches for a link with the given source and target
    pub fn search(&self, source: u32, target: u32) -> Option<u32> {
        for link in self.links.values() {
            if link.source == source && link.target == target {
                return Some(link.index);
            }
        }
        None
    }

    /// Gets or creates a link with the given source and target
    pub fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        if let Some(id) = self.search(source, target) {
            id
        } else {
            self.create(source, target)
        }
    }

    /// Formats a link for display
    pub fn format(&self, link: &Link) -> String {
        // Use name if available
        let index_str = self
            .names
            .get(&link.index)
            .cloned()
            .unwrap_or_else(|| link.index.to_string());
        let source_str = self
            .names
            .get(&link.source)
            .cloned()
            .unwrap_or_else(|| link.source.to_string());
        let target_str = self
            .names
            .get(&link.target)
            .cloned()
            .unwrap_or_else(|| link.target.to_string());
        format!("({} {} {})", index_str, source_str, target_str)
    }

    /// Formats the structure of a link
    pub fn format_structure(&self, id: u32) -> Result<String> {
        if let Some(link) = self.get(id) {
            Ok(self.format_structure_recursive(link, true))
        } else {
            Err(LinkError::NotFound(id).into())
        }
    }

    /// Recursively formats a link structure
    fn format_structure_recursive(&self, link: &Link, is_root: bool) -> String {
        if link.is_full_point() && !is_root {
            // Self-referential point - just show the name/id
            return self
                .names
                .get(&link.index)
                .cloned()
                .unwrap_or_else(|| link.index.to_string());
        }

        let source_str = if link.source == link.index {
            self.names
                .get(&link.index)
                .cloned()
                .unwrap_or_else(|| link.index.to_string())
        } else if let Some(source_link) = self.get(link.source) {
            self.format_structure_recursive(source_link, false)
        } else {
            link.source.to_string()
        };

        let target_str = if link.target == link.index {
            self.names
                .get(&link.index)
                .cloned()
                .unwrap_or_else(|| link.index.to_string())
        } else if let Some(target_link) = self.get(link.target) {
            self.format_structure_recursive(target_link, false)
        } else {
            link.target.to_string()
        };

        format!("({} {})", source_str, target_str)
    }

    /// Prints all links
    pub fn print_all_links(&self) {
        let mut links: Vec<_> = self.all();
        links.sort_by_key(|l| l.index);
        for link in links {
            println!("{}", self.format(link));
        }
    }

    /// Prints a change (before -> after)
    pub fn print_change(&self, before: &Option<Link>, after: &Option<Link>) {
        let before_text = before.map(|l| self.format(&l)).unwrap_or_default();
        let after_text = after.map(|l| self.format(&l)).unwrap_or_default();
        println!("({}) ({})", before_text, after_text);
    }

    // Named links functionality (corresponds to NamedLinks.cs)

    /// Gets or creates a link with a name
    pub fn get_or_create_named(&mut self, name: &str) -> u32 {
        if let Some(&id) = self.name_to_id.get(name) {
            id
        } else {
            // Create a self-referential link for the name
            let id = self.create(0, 0);
            self.update(id, id, id).ok();
            self.names.insert(id, name.to_string());
            self.name_to_id.insert(name.to_string(), id);
            if self.trace {
                eprintln!("[TRACE] Created named link: {} => {}", name, id);
            }
            id
        }
    }

    /// Sets the name for a link
    pub fn set_name(&mut self, id: u32, name: &str) {
        // Remove old name mapping if exists
        if let Some(old_name) = self.names.remove(&id) {
            self.name_to_id.remove(&old_name);
        }
        self.names.insert(id, name.to_string());
        self.name_to_id.insert(name.to_string(), id);
        if self.trace {
            eprintln!("[TRACE] Set name: {} => {}", id, name);
        }
    }

    /// Gets the name of a link
    pub fn get_name(&self, id: u32) -> Option<&String> {
        self.names.get(&id)
    }

    /// Gets a link ID by name
    pub fn get_by_name(&self, name: &str) -> Option<u32> {
        self.name_to_id.get(name).copied()
    }

    /// Removes the name for a link
    pub fn remove_name(&mut self, id: u32) {
        if let Some(name) = self.names.remove(&id) {
            self.name_to_id.remove(&name);
            if self.trace {
                eprintln!("[TRACE] Removed name: {} => {}", id, name);
            }
        }
    }

    /// Returns true if trace mode is enabled
    pub fn is_trace_enabled(&self) -> bool {
        self.trace
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_storage_create() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let id = storage.create(2, 3);

        assert!(id > 0);
        let link = storage.get(id).unwrap();
        assert_eq!(link.source, 2);
        assert_eq!(link.target, 3);

        Ok(())
    }

    #[test]
    fn test_storage_update() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let id = storage.create(2, 3);
        storage.update(id, 4, 5)?;

        let link = storage.get(id).unwrap();
        assert_eq!(link.source, 4);
        assert_eq!(link.target, 5);

        Ok(())
    }

    #[test]
    fn test_storage_delete() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let id = storage.create(2, 3);
        storage.delete(id)?;

        assert!(storage.get(id).is_none());

        Ok(())
    }

    #[test]
    fn test_storage_persistence() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        // Create and save
        {
            let mut storage = LinkStorage::new(db_path, false)?;
            storage.create(2, 3);
            storage.save()?;
        }

        // Load and verify
        {
            let storage = LinkStorage::new(db_path, false)?;
            let links = storage.all();
            assert_eq!(links.len(), 1);
            assert_eq!(links[0].source, 2);
            assert_eq!(links[0].target, 3);
        }

        Ok(())
    }

    #[test]
    fn test_storage_named_links() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let id = storage.get_or_create_named("test");

        assert!(id > 0);
        assert_eq!(storage.get_name(id), Some(&"test".to_string()));
        assert_eq!(storage.get_by_name("test"), Some(id));

        Ok(())
    }

    #[test]
    fn test_storage_search() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let id = storage.create(2, 3);

        assert_eq!(storage.search(2, 3), Some(id));
        assert_eq!(storage.search(1, 1), None);

        Ok(())
    }

    #[test]
    fn test_storage_get_or_create() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;

        let id1 = storage.get_or_create(2, 3);
        let id2 = storage.get_or_create(2, 3);

        assert_eq!(id1, id2);

        Ok(())
    }
}
