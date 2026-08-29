//! LinkStorage - Persistent storage for links
//!
//! This module provides the LinkStorage struct for managing link persistence.

use anyhow::{Context, Result};
use doublets::decorators::DecoratorsExt;
use doublets::Doublets;
use std::collections::{HashMap, HashSet};
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use crate::error::LinkError;
use crate::link::Link;
use crate::storage::StorageRevision;

/// Callback invoked once per `(before, after)` change a write produced.
///
/// The upstream decorators turn a single write into a cascade of changes, so
/// the layers above the storage — names, transactions, the query processor —
/// only stay in sync if they can see all of them. This is the equivalent of the
/// `WriteHandler` the C# implementation threads through every decorator. A
/// change whose `after` [`is null`](Link::is_null) is a deletion.
pub type ChangeObserver<'a> = &'a mut dyn FnMut(Link, Link);

/// Adapts a [`ChangeObserver`] to the `doublets` write handler signature.
fn observe(
    observer: &mut dyn FnMut(Link, Link),
    before: doublets::Link<u32>,
    after: doublets::Link<u32>,
) -> doublets::data::Flow {
    observer(Link::from(before), Link::from(after));
    doublets::data::Flow::Continue
}

/// Prefix of the database line that records the freed addresses.
///
/// It is a comment so that a database written by this version still loads in
/// one that predates it, and so that a database written before it still loads
/// here — [`LinkStorage::restore_unused`] reconstructs the list when the line
/// is absent.
const UNUSED_HEADER: &str = "# unused:";

/// LinkStorage provides persistent storage for links
/// Corresponds to the storage functionality in NamedLinksDecorator in C#
pub struct LinkStorage {
    links: HashMap<u32, Link>,
    names: HashMap<u32, String>,
    name_to_id: HashMap<String, u32>,
    /// The highest address ever handed out and not given back, i.e. the
    /// `AllocatedLinks` counter of the C# store.
    allocated: u32,
    /// Addresses below [`Self::allocated`] that were freed and can be handed
    /// out again, most recently freed last.
    ///
    /// The C# store keeps the same set as a linked list threaded through the
    /// freed links themselves, pushing and popping at its head; a stack is the
    /// same structure without the threading.
    unused: Vec<u32>,
    db_path: PathBuf,
    revision: StorageRevision,
    trace: bool,
}

impl LinkStorage {
    /// Creates a new LinkStorage instance
    ///
    /// The database location is accepted as any [`AsRef<Path>`], so
    /// embedding applications can pass a `PathBuf` (or an `OsStr` on
    /// platforms with non-UTF-8 paths) instead of a `&str`.
    pub fn new<P: AsRef<Path>>(db_path: P, trace: bool) -> Result<Self> {
        let db_path = db_path.as_ref().to_path_buf();
        let exists = db_path.exists();
        let mut storage = Self {
            links: HashMap::new(),
            names: HashMap::new(),
            name_to_id: HashMap::new(),
            allocated: 0,
            unused: Vec::new(),
            db_path,
            revision: StorageRevision::default(),
            trace,
        };

        // Load existing database if it exists
        if exists {
            storage.load()?;
        }
        storage.revision = StorageRevision::of(&storage.db_path)?;

        Ok(storage)
    }

    /// The database file this storage reads from and writes to.
    pub fn database_path(&self) -> &Path {
        &self.db_path
    }

    /// The revision of the database file observed at the last load or save.
    pub fn observed_revision(&self) -> StorageRevision {
        self.revision
    }

    /// Re-reads the database file's revision fingerprint, marking the
    /// current on-disk state as "seen" for
    /// [`LinksStorage::has_external_changes`](crate::LinksStorage::has_external_changes).
    pub fn refresh_observed_revision(&mut self) -> Result<(), LinkError> {
        self.revision = StorageRevision::of(&self.db_path)?;
        Ok(())
    }

    /// Discards in-memory state and re-reads the database file.
    pub fn reload_from_disk(&mut self) -> Result<()> {
        self.links.clear();
        self.names.clear();
        self.name_to_id.clear();
        self.allocated = 0;
        self.unused.clear();
        if self.db_path.exists() {
            self.load()?;
        }
        self.revision = StorageRevision::of(&self.db_path)?;
        Ok(())
    }

    /// Loads links from the database file
    fn load(&mut self) -> Result<()> {
        let file = File::open(&self.db_path)
            .with_context(|| format!("Failed to open database: {}", self.db_path.display()))?;

        let reader = BufReader::new(file);
        let mut recorded_unused = None;

        for line in reader.lines() {
            let line = line?;
            let line = line.trim();

            if let Some(addresses) = line.strip_prefix(UNUSED_HEADER) {
                recorded_unused = Some(Self::parse_unused_header(addresses));
                continue;
            }

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse link format: (index source target) or (index source target "name")
            if let Some((link, name)) = self.parse_link_line(line) {
                self.links.insert(link.index, link);
                if link.index > self.allocated {
                    self.allocated = link.index;
                }
                if let Some(name) = name {
                    self.names.insert(link.index, name.clone());
                    self.name_to_id.insert(name, link.index);
                }
            }
        }

        self.unused = self.restore_unused(recorded_unused);

        if self.trace {
            eprintln!(
                "[TRACE] Loaded {} links from {}",
                self.links.len(),
                self.db_path.display()
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

    /// Parses the addresses recorded by an [`UNUSED_HEADER`] line into the
    /// stack order the allocator uses.
    ///
    /// The line lists them the way the C# store's free list reads — most
    /// recently freed first — and the stack pops from its end, so the two are
    /// reverses of each other.
    fn parse_unused_header(addresses: &str) -> Vec<u32> {
        let mut unused: Vec<u32> = addresses
            .split_whitespace()
            .filter_map(|address| address.parse().ok())
            .collect();
        unused.reverse();
        unused
    }

    /// The freed-address stack to start from after a load.
    ///
    /// A database this version wrote records the stack, because the order
    /// decides which address the next link gets and nothing in the list of
    /// stored links implies it. A database written before this version (or by
    /// hand) does not, so the addresses missing below the highest stored one
    /// are recovered instead, lowest reused first — an order the file does at
    /// least determine.
    ///
    /// Addresses that a hand-edited file records but that are in use, or that
    /// sit above the highest stored link, are dropped: handing them out would
    /// overwrite a link or leave a hole the allocator would hand out twice.
    fn restore_unused(&self, recorded: Option<Vec<u32>>) -> Vec<u32> {
        match recorded {
            Some(recorded) => {
                let mut seen = HashSet::new();
                recorded
                    .into_iter()
                    .filter(|address| {
                        *address > 0
                            && *address < self.allocated
                            && !self.links.contains_key(address)
                            && seen.insert(*address)
                    })
                    .collect()
            }
            None => (1..self.allocated)
                .filter(|address| !self.links.contains_key(address))
                .rev()
                .collect(),
        }
    }

    /// Hands out the address of the next link, reusing a freed one first.
    ///
    /// This is `ResizableDirectMemoryLinks.AllocateLink` in the C#
    /// implementation: an address is only taken from beyond the end of the
    /// store when no freed one is left. Reuse is observable — it decides the
    /// address a query reports for a link it creates — so the two
    /// implementations have to agree on it.
    fn allocate(&mut self) -> u32 {
        match self.unused.pop() {
            Some(address) => address,
            None => {
                self.allocated += 1;
                self.allocated
            }
        }
    }

    /// Gives `address` back to the allocator.
    ///
    /// Freeing the highest allocated address shrinks the store rather than
    /// growing the free list, and takes with it every freed address that has
    /// become the new end — `ResizableDirectMemoryLinks.Delete` does exactly
    /// this, which is why C# reuses the address of a link it just appended
    /// before it reuses one freed earlier.
    fn release(&mut self, address: u32) {
        if address == 0 || address > self.allocated {
            return;
        }
        if address < self.allocated {
            self.unused.push(address);
            return;
        }
        self.allocated = address - 1;
        while let Some(position) = self
            .unused
            .iter()
            .position(|&freed| freed == self.allocated)
        {
            self.unused.remove(position);
            self.allocated -= 1;
        }
    }

    /// Saves all links to the database file
    pub fn save(&self) -> Result<()> {
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&self.db_path)
            .with_context(|| format!("Failed to create database: {}", self.db_path.display()))?;

        let mut writer = BufWriter::new(file);

        // The freed addresses first: which of them the next link gets is not
        // implied by the links that follow, and reloading has to resume the
        // allocator exactly where it stopped.
        if !self.unused.is_empty() {
            let recorded: Vec<String> = self
                .unused
                .iter()
                .rev()
                .map(|address| address.to_string())
                .collect();
            writeln!(writer, "{UNUSED_HEADER} {}", recorded.join(" "))?;
        }

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
                self.db_path.display()
            );
        }

        Ok(())
    }

    /// Creates a new link and returns its ID
    ///
    /// The address is the one `allocate` hands out: a freed one
    /// when the store has any, and only otherwise a fresh one past the end.
    pub fn create(&mut self, source: u32, target: u32) -> u32 {
        let id = self.allocate();

        let link = Link::new(id, source, target);
        self.links.insert(id, link);

        if self.trace {
            eprintln!("[TRACE] Created link: ({} {} {})", id, source, target);
        }

        id
    }

    /// Creates the link at `id`, as an empty `(id: 0 0)` link.
    ///
    /// Reaching a specific address means asking the allocator for links until
    /// it hands that one out, and the ones it handed out on the way are freed
    /// again — they were never asked for. This is `ILinksExtensions.EnsureCreated`
    /// in the C# implementation:
    ///
    /// ```csharp
    /// do { createdLink = creator(); createdLinks.Add(createdLink); }
    /// while (createdLink != max);
    /// for (var i = 0; i < createdLinks.Count; i++)
    ///     if (!nonExistentAddresses.Contains(createdLinks[i]))
    ///         links.Delete(createdLinks[i]);
    /// ```
    ///
    /// Freeing them in the order they were created is what leaves the last one
    /// on top of the free list, so it is the address the next created link
    /// gets.
    pub fn ensure_created(&mut self, id: u32) -> u32 {
        if id == 0 || self.links.contains_key(&id) {
            return id;
        }

        let mut passed_over = Vec::new();
        loop {
            let created = self.create(0, 0);
            if created == id {
                break;
            }
            passed_over.push(created);
        }

        for address in passed_over {
            let _ = self.delete_raw(address);
        }

        if self.trace {
            eprintln!("[TRACE] Ensured link: ({} 0 0)", id);
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

    /// Updates a link's source and target **without** applying any policy.
    ///
    /// This is the raw store operation, the equivalent of writing straight to
    /// `UnitedMemoryLinks` in the C# implementation. [`LinkStorage::update`]
    /// wraps it with the upstream uniqueness/usages decorators; use this method
    /// when you are supplying your own decorator stack (or deliberately want
    /// none).
    pub fn update_raw(&mut self, id: u32, source: u32, target: u32) -> Result<Link> {
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
            Err(LinkError::not_found(id).into())
        }
    }

    /// Deletes a link by ID **without** applying any policy.
    ///
    /// The raw counterpart of [`LinkStorage::delete`]: it removes exactly the
    /// requested link (and its name), leaving any link that referenced it
    /// dangling.
    pub fn delete_raw(&mut self, id: u32) -> Result<Link> {
        // Also remove the name mapping
        if let Some(name) = self.names.remove(&id) {
            self.name_to_id.remove(&name);
        }

        if let Some(link) = self.links.remove(&id) {
            self.release(id);
            if self.trace {
                eprintln!(
                    "[TRACE] Deleted link: ({} {} {})",
                    link.index, link.source, link.target
                );
            }
            Ok(link)
        } else {
            Err(LinkError::not_found(id).into())
        }
    }

    /// Updates a link's source and target through the upstream
    /// `doublets` uniqueness and usages resolution stack.
    ///
    /// This mirrors the C# implementation, which always talks to a
    /// `UnitedMemoryLinks` wrapped in
    /// `DecorateWithAutomaticUniquenessAndUsagesResolution()`. Concretely: if
    /// another link already holds `(source, target)`, every reference to `id`
    /// is re-pointed at that link and `id` is deleted, instead of storing a
    /// duplicate doublet.
    ///
    /// Returns the state the link was in before the operation. Use
    /// [`LinkStorage::update_raw`] for the undecorated write.
    pub fn update(&mut self, id: u32, source: u32, target: u32) -> Result<Link> {
        self.update_observed(id, source, target, &mut |_, _| {})
    }

    /// [`LinkStorage::update`], reporting every change the decorator stack made.
    ///
    /// Resolving a duplicate doublet re-points and deletes other links, so one
    /// call can produce several changes. Layers above the storage need to see
    /// all of them — the C# implementation gets them for free because its
    /// decorators forward to a `WriteHandler`:
    ///
    /// ```csharp
    /// var result = _links.Update(restriction, substitution, (before, after) => { ... });
    /// ```
    ///
    /// `observer` is that handler. A change with a null `after` is a deletion.
    pub fn update_observed(
        &mut self,
        id: u32,
        source: u32,
        target: u32,
        observer: ChangeObserver<'_>,
    ) -> Result<Link> {
        let before = *self
            .links
            .get(&id)
            .ok_or_else(|| LinkError::not_found(id))?;
        let mut resolved = (&mut *self).with_automatic_uniqueness_and_usages_resolution();
        resolved
            .update_by_with([id], [id, source, target], &mut |before, after| {
                observe(observer, before, after)
            })
            .map_err(LinkError::from)?;
        Ok(before)
    }

    /// Deletes a link through the upstream `doublets` uniqueness and usages
    /// resolution stack, cascading to every link that references it.
    ///
    /// This mirrors the C# implementation's
    /// `DecorateWithAutomaticUniquenessAndUsagesResolution()` behaviour: the
    /// link is reset to `(null, null)`, everything that still references it is
    /// deleted first, and only then is the link itself removed. Cycles
    /// terminate rather than recursing forever.
    ///
    /// Returns the state the requested link was in before the operation. Use
    /// [`LinkStorage::delete_raw`] for the undecorated removal.
    pub fn delete(&mut self, id: u32) -> Result<Link> {
        self.delete_observed(id, &mut |_, _| {})
    }

    /// [`LinkStorage::delete`], reporting every change the decorator stack made.
    ///
    /// A cascading delete removes every link that still referenced `id`, so one
    /// call can produce several changes; see [`LinkStorage::update_observed`].
    pub fn delete_observed(&mut self, id: u32, observer: ChangeObserver<'_>) -> Result<Link> {
        let before = *self
            .links
            .get(&id)
            .ok_or_else(|| LinkError::not_found(id))?;
        let mut resolved = (&mut *self).with_automatic_uniqueness_and_usages_resolution();
        resolved
            .delete_by_with([id], &mut |before, after| observe(observer, before, after))
            .map_err(LinkError::from)?;
        Ok(before)
    }

    /// Every stored link, ordered by address.
    ///
    /// The order is part of the contract, not an implementation detail: the
    /// query processor enumerates links through this method, so an
    /// unspecified order would make pattern matching — and with it the order
    /// `--changes` reports and the order a cascading delete visits usages —
    /// vary between runs of the very same query. `HashMap::values` is exactly
    /// such an order, seeded randomly per process. Sorting reproduces what the
    /// C# store does naturally: `UnitedMemoryLinks` walks allocated addresses
    /// from `1` upwards.
    pub fn all(&self) -> Vec<&Link> {
        let mut links: Vec<&Link> = self.links.values().collect();
        links.sort_unstable_by_key(|link| link.index);
        links
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

    /// Searches for a link with the given source and target.
    ///
    /// When several links share the pair, the lowest address wins, so the
    /// result never depends on hash map iteration order.
    pub fn search(&self, source: u32, target: u32) -> Option<u32> {
        self.links
            .values()
            .filter(|link| link.source == source && link.target == target)
            .map(|link| link.index)
            .min()
    }

    /// Gets or creates a link with the given source and target.
    ///
    /// The two calls are fully qualified on purpose. [`LinkStorage`] also
    /// implements the upstream [`Doublets`] trait — including *for
    /// `&mut LinkStorage`*, so that a borrowed store can be decorated — and
    /// inside an inherent `&mut self` method the receiver's type is exactly
    /// `&mut LinkStorage`. Method resolution reaches the trait impl on the
    /// reference before it derefs to the inherent impl, so a bare
    /// `self.search(..)` silently resolves to [`Doublets::search`], which
    /// interprets [`LinksConstants::any`](doublets::data::LinksConstants) as a
    /// wildcard instead of matching it literally. Naming the inherent methods
    /// keeps the exact-match semantics this function documents.
    pub fn get_or_create(&mut self, source: u32, target: u32) -> u32 {
        if let Some(id) = Self::search(self, source, target) {
            id
        } else {
            Self::create(self, source, target)
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

    /// Formats a link as LiNo suitable for database export.
    pub fn format_lino(&self, link: &Link) -> String {
        format!(
            "({}: {} {})",
            self.format_lino_reference(link.index),
            self.format_lino_reference(link.source),
            self.format_lino_reference(link.target)
        )
    }

    /// Returns all database links as sorted LiNo lines.
    pub fn lino_lines(&self) -> Vec<String> {
        let mut links: Vec<_> = self.all();
        links.sort_by_key(|l| l.index);
        links
            .into_iter()
            .map(|link| self.format_lino(link))
            .collect()
    }

    /// Writes the complete database as LiNo.
    pub fn write_lino_output<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let path = path.as_ref();
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)
            .with_context(|| format!("Failed to create LiNo output: {}", path.display()))?;

        let mut writer = BufWriter::new(file);
        for line in self.lino_lines() {
            writeln!(writer, "{line}")?;
        }
        writer.flush()?;
        Ok(())
    }

    /// Formats the structure of a link
    pub fn format_structure(&self, id: u32) -> Result<String> {
        let mut visited = HashSet::new();
        self.format_structure_recursive(id, &mut visited)
    }

    /// Recursively formats a link structure
    fn format_structure_recursive(&self, id: u32, visited: &mut HashSet<u32>) -> Result<String> {
        let link = self.get(id).ok_or(LinkError::not_found(id))?;
        if !visited.insert(id) {
            return Ok(self.format_lino_reference(id));
        }

        let source = if self.exists(link.source) && !visited.contains(&link.source) {
            self.format_structure_recursive(link.source, visited)?
        } else {
            self.format_lino_reference(link.source)
        };
        let target = self.format_lino_reference(link.target);
        let index = self.format_lino_reference(link.index);
        visited.remove(&id);

        Ok(format!("({index}: {source} {target})"))
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
            // Fully qualified for the same reason as in
            // [`LinkStorage::get_or_create`]: the `Doublets` impl for
            // `&mut LinkStorage` shadows the inherent `create`/`update`.
            let id = Self::create(self, 0, 0);
            Self::update(self, id, id, id).ok();
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

    fn format_lino_reference(&self, id: u32) -> String {
        self.names
            .get(&id)
            .map(|name| escape_lino_reference(name))
            .unwrap_or_else(|| id.to_string())
    }
}

fn escape_lino_reference(reference: &str) -> String {
    if reference.is_empty() || reference.trim().is_empty() {
        return String::new();
    }

    let has_single_quote = reference.contains('\'');
    let has_double_quote = reference.contains('"');
    let needs_quoting = reference.contains(':')
        || reference.contains('(')
        || reference.contains(')')
        || reference.contains(' ')
        || reference.contains('\t')
        || reference.contains('\n')
        || reference.contains('\r')
        || has_single_quote
        || has_double_quote;

    if has_single_quote && has_double_quote {
        return format!("'{}'", reference.replace('\'', "\\'"));
    }

    if has_double_quote {
        return format!("'{reference}'");
    }

    if has_single_quote {
        return format!("\"{reference}\"");
    }

    if needs_quoting {
        return format!("'{reference}'");
    }

    reference.to_string()
}
