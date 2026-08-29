//! Storage backends for the transitions log.
//!
//! The transactions layer only needs three things from its log: append
//! one entry, read every entry back in write order, and make what has
//! been appended durable. [`TransitionLogStore`] captures exactly that,
//! so a consumer can plug in whichever log it already has.
//!
//! Two implementations ship with the library:
//!
//! * [`NamedTypesDecorator`] — the original sidecar *links* log, where
//!   each entry is the name of a freshly created link. Keeps the log in
//!   the same format the C# port uses.
//! * [`FileTransitionLog`] — a plain append-only text file, one entry
//!   per line, `fsync`ed on every append by default. Cheaper than a
//!   links database and crash-safe by construction: a torn write can
//!   only ever damage the final line, which [`FileTransitionLog`]
//!   discards on read.
//!
//! Reading takes `&mut self` because the links-backed log resolves
//! names through [`NamedTypes::get_name`], which needs mutable access
//! to its own caches.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::LinkError;
use crate::named_types::{NamedTypes, NamedTypesDecorator};

/// Append-only store of serialized transitions and recovery markers.
pub trait TransitionLogStore {
    /// Appends one entry. Entries never contain newlines.
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError>;

    /// Reads every entry back, oldest first.
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError>;

    /// Makes every appended entry durable.
    fn flush_log(&mut self) -> Result<(), LinkError>;
}

fn storage_error(error: anyhow::Error) -> LinkError {
    LinkError::StorageError(format!("{error:#}"))
}

impl TransitionLogStore for NamedTypesDecorator {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        // Always allocate a fresh link so entries never overwrite one
        // another (mirrors C# `CreateAndUpdate(Null, Null)`).
        let link = self.create(0, 0);
        self.set_name(link, entry).map_err(storage_error)?;
        Ok(())
    }

    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        let mut addresses: Vec<u32> = self.all().into_iter().map(|link| link.index).collect();
        addresses.sort_unstable();
        let mut entries = Vec::with_capacity(addresses.len());
        for address in addresses {
            if let Some(name) = NamedTypes::get_name(self, address).map_err(storage_error)? {
                entries.push(name);
            }
        }
        Ok(entries)
    }

    fn flush_log(&mut self) -> Result<(), LinkError> {
        NamedTypesDecorator::save(self).map_err(storage_error)
    }
}

/// Append-only, line-oriented transitions log backed by a single file.
///
/// # Durability
///
/// With `sync_on_append` enabled (the default) every
/// [`append_log_entry`](TransitionLogStore::append_log_entry) returns
/// only after the entry has reached stable storage, which is what makes
/// the write-ahead ordering of the transactions layer meaningful: a
/// transition is always durable before the data-store write it
/// describes is reported as committed. Disabling it trades that
/// guarantee for throughput — the log is then durable only as far as
/// the last [`flush_log`](TransitionLogStore::flush_log).
///
/// A crash can therefore only ever truncate the file mid-line.
/// [`read_log_entries`](TransitionLogStore::read_log_entries) drops a
/// trailing entry that is not newline-terminated, so a torn write costs
/// at most the single transition that was in flight.
#[derive(Debug)]
pub struct FileTransitionLog {
    path: PathBuf,
    file: File,
    sync_on_append: bool,
}

impl FileTransitionLog {
    /// Opens (creating if needed) the log at `path`.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, LinkError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&path)?;
        Ok(Self {
            path,
            file,
            sync_on_append: true,
        })
    }

    /// Path of the backing file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Whether every append is `fsync`ed. Enabled by default.
    pub fn sync_on_append(&self) -> bool {
        self.sync_on_append
    }

    /// Turns per-append `fsync` on or off — see the durability notes.
    pub fn set_sync_on_append(&mut self, value: bool) {
        self.sync_on_append = value;
    }
}

impl TransitionLogStore for FileTransitionLog {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        if entry.contains('\n') || entry.contains('\r') {
            return Err(LinkError::InvalidFormat(
                "transition log entries must not contain line breaks".to_string(),
            ));
        }
        writeln!(self.file, "{entry}")?;
        self.file.flush()?;
        if self.sync_on_append {
            self.file.sync_data()?;
        }
        Ok(())
    }

    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        let mut contents = String::new();
        File::open(&self.path)?.read_to_string(&mut contents)?;
        // Only newline-terminated lines were fully written; a trailing
        // fragment is the torn tail of a crashed append.
        Ok(contents
            .lines()
            .take(contents.matches('\n').count())
            .filter(|line| !line.is_empty())
            .map(|line| line.to_string())
            .collect())
    }

    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.file.flush()?;
        self.file.sync_data()?;
        Ok(())
    }
}
