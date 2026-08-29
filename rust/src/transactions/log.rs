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
use std::io::{Read, Seek, SeekFrom, Write};
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
/// [`open`](FileTransitionLog::open) discards such a torn tail before
/// the log is used again — otherwise the next append would be glued
/// onto the fragment and lost with it — and
/// [`read_log_entries`](TransitionLogStore::read_log_entries) ignores
/// one defensively. A torn write costs at most the single transition
/// that was in flight.
#[derive(Debug)]
pub struct FileTransitionLog {
    path: PathBuf,
    file: File,
    sync_on_append: bool,
}

impl FileTransitionLog {
    /// Opens (creating if needed) the log at `path`, discarding a
    /// trailing entry left half-written by a crash.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, LinkError> {
        let path = path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        // The torn tail is trimmed through a dedicated read/write handle:
        // Windows grants an append-only handle `FILE_APPEND_DATA` without
        // `FILE_WRITE_DATA`, so `set_len` on it fails with `ERROR_ACCESS_DENIED`.
        let repair = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)?;
        truncate_torn_tail(&repair)?;
        drop(repair);
        let file = OpenOptions::new().read(true).append(true).open(&path)?;
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

/// Shrinks `file` to its last complete (newline-terminated) line.
///
/// A crash can leave the log ending in a fragment of an entry. Appending
/// after that fragment would concatenate the next entry onto it, turning
/// one lost transition into two, so the fragment is dropped when the log
/// is opened.
fn truncate_torn_tail(file: &File) -> Result<(), LinkError> {
    let len = file.metadata()?.len();
    if len == 0 {
        return Ok(());
    }
    let mut file = file;
    let mut end = len;
    let mut buffer = [0u8; 8192];
    while end > 0 {
        let chunk = std::cmp::min(end, buffer.len() as u64);
        let start = end - chunk;
        file.seek(SeekFrom::Start(start))?;
        let slice = &mut buffer[..chunk as usize];
        file.read_exact(slice)?;
        if let Some(offset) = slice.iter().rposition(|byte| *byte == b'\n') {
            let complete = start + offset as u64 + 1;
            if complete != len {
                file.set_len(complete)?;
                file.sync_all()?;
            }
            return Ok(());
        }
        end = start;
    }
    // No newline anywhere: the whole file is one torn entry.
    file.set_len(0)?;
    file.sync_all()?;
    Ok(())
}
