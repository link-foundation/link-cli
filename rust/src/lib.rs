//! Link CLI Library - Core functionality for links manipulation
//!
//! This module provides the core data structures and functionality
//! for the link-cli tool.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use thiserror::Error;

/// Link represents a doublet (source, target) pair with an index
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Link {
    pub index: u32,
    pub source: u32,
    pub target: u32,
}

impl Link {
    /// Creates a new link with the given index, source, and target
    pub fn new(index: u32, source: u32, target: u32) -> Self {
        Self {
            index,
            source,
            target,
        }
    }

    /// Returns true if this link is null (all zeros)
    pub fn is_null(&self) -> bool {
        self.index == 0 && self.source == 0 && self.target == 0
    }

    /// Returns true if this is a full point (self-referential link)
    pub fn is_full_point(&self) -> bool {
        self.index == self.source && self.source == self.target
    }
}

/// Error types for link operations
#[derive(Error, Debug)]
pub enum LinkError {
    #[error("Link not found: {0}")]
    NotFound(u32),

    #[error("Invalid link format: {0}")]
    InvalidFormat(String),

    #[error("Storage error: {0}")]
    StorageError(String),

    #[error("Query error: {0}")]
    QueryError(String),
}

/// LinkStorage provides persistent storage for links
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
            if let Some(link) = self.parse_link_line(line) {
                self.links.insert(link.index, link);
                if link.index >= self.next_id {
                    self.next_id = link.index + 1;
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
    fn parse_link_line(&self, line: &str) -> Option<Link> {
        // Simple format: (index source target)
        let line = line.trim_matches(|c| c == '(' || c == ')');
        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.len() >= 3 {
            let index = parts[0].parse().ok()?;
            let source = parts[1].parse().ok()?;
            let target = parts[2].parse().ok()?;
            return Some(Link::new(index, source, target));
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

        for link in self.links.values() {
            writeln!(writer, "({} {} {})", link.index, link.source, link.target)?;
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

    /// Gets a link by ID
    pub fn get(&self, id: u32) -> Option<&Link> {
        self.links.get(&id)
    }

    /// Updates a link's source and target
    pub fn update(&mut self, id: u32, source: u32, target: u32) -> Result<()> {
        if let Some(link) = self.links.get_mut(&id) {
            if self.trace {
                eprintln!(
                    "[TRACE] Updated link {} from ({} {}) to ({} {})",
                    id, link.source, link.target, source, target
                );
            }
            link.source = source;
            link.target = target;
            Ok(())
        } else {
            Err(LinkError::NotFound(id).into())
        }
    }

    /// Deletes a link by ID
    pub fn delete(&mut self, id: u32) -> Result<Link> {
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

    /// Formats a link for display
    pub fn format(&self, link: &Link) -> String {
        format!("({} {} {})", link.index, link.source, link.target)
    }

    /// Formats the structure of a link
    pub fn format_structure(&self, id: u32) -> Result<String> {
        if let Some(link) = self.get(id) {
            Ok(self.format(link))
        } else {
            Err(LinkError::NotFound(id).into())
        }
    }

    /// Prints all links
    pub fn print_all_links(&self) {
        for link in self.all() {
            println!("{}", self.format(link));
        }
    }

    /// Prints a change (before -> after)
    pub fn print_change(&self, before: &Option<Link>, after: &Option<Link>) {
        let before_text = before.map(|l| self.format(&l)).unwrap_or_default();
        let after_text = after.map(|l| self.format(&l)).unwrap_or_default();
        println!("({}) ({})", before_text, after_text);
    }

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
            id
        }
    }

    /// Gets the name of a link
    pub fn get_name(&self, id: u32) -> Option<&String> {
        self.names.get(&id)
    }
}

/// QueryProcessor handles LiNo query parsing and execution
pub struct QueryProcessor {
    trace: bool,
}

impl QueryProcessor {
    /// Creates a new QueryProcessor
    pub fn new(trace: bool) -> Self {
        Self { trace }
    }

    /// Processes a LiNo query and returns the list of changes
    pub fn process_query(
        &self,
        storage: &mut LinkStorage,
        query: &str,
    ) -> Result<Vec<(Option<Link>, Option<Link>)>> {
        if self.trace {
            eprintln!("[TRACE] Processing query: {}", query);
        }

        let mut changes = Vec::new();

        // Parse and execute the query
        // This is a simplified implementation - the full LiNo parser would be more complex
        let query = query.trim();

        // Handle basic link creation: (source target) or (name)
        if query.starts_with('(') && query.ends_with(')') {
            let content = &query[1..query.len() - 1];
            let parts: Vec<&str> = content.split_whitespace().collect();

            match parts.len() {
                1 => {
                    // Single name - create a named point
                    let name = parts[0];
                    let id = storage.get_or_create_named(name);
                    if let Some(link) = storage.get(id) {
                        changes.push((None, Some(*link)));
                    }
                }
                2 => {
                    // Two parts - create a link
                    let source = self.resolve_part(storage, parts[0])?;
                    let target = self.resolve_part(storage, parts[1])?;
                    let id = storage.create(source, target);
                    if let Some(link) = storage.get(id) {
                        changes.push((None, Some(*link)));
                    }
                }
                3 => {
                    // Three parts - index, source, target
                    // This could be an update or a specific create
                    let index: u32 = parts[0].parse().map_err(|_| {
                        LinkError::InvalidFormat(format!("Invalid index: {}", parts[0]))
                    })?;
                    let source = self.resolve_part(storage, parts[1])?;
                    let target = self.resolve_part(storage, parts[2])?;

                    if let Some(existing) = storage.get(index) {
                        let before = *existing;
                        storage.update(index, source, target)?;
                        if let Some(after) = storage.get(index) {
                            changes.push((Some(before), Some(*after)));
                        }
                    } else {
                        // Create with specific index is not directly supported
                        // Create a new link instead
                        let id = storage.create(source, target);
                        if let Some(link) = storage.get(id) {
                            changes.push((None, Some(*link)));
                        }
                    }
                }
                _ => {
                    return Err(LinkError::InvalidFormat(format!(
                        "Invalid query format: {}",
                        query
                    ))
                    .into());
                }
            }
        }

        // Save changes
        storage.save()?;

        Ok(changes)
    }

    /// Resolves a query part (name or ID) to a link ID
    fn resolve_part(&self, storage: &mut LinkStorage, part: &str) -> Result<u32> {
        // Try to parse as number first
        if let Ok(id) = part.parse::<u32>() {
            Ok(id)
        } else {
            // Treat as name
            Ok(storage.get_or_create_named(part))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_link_creation() {
        let link = Link::new(1, 2, 3);
        assert_eq!(link.index, 1);
        assert_eq!(link.source, 2);
        assert_eq!(link.target, 3);
    }

    #[test]
    fn test_link_is_null() {
        let null_link = Link::default();
        assert!(null_link.is_null());

        let non_null_link = Link::new(1, 2, 3);
        assert!(!non_null_link.is_null());
    }

    #[test]
    fn test_link_is_full_point() {
        let point = Link::new(1, 1, 1);
        assert!(point.is_full_point());

        let non_point = Link::new(1, 2, 3);
        assert!(!non_point.is_full_point());
    }

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
}
