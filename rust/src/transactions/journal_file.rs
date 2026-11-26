//! The append-only journal file `clink` keeps next to its database
//! (issue #195), and the one-time import of the older links-backed journal.
//!
//! Older versions stored each journal entry as the name of a fresh link in
//! `<db>.transitions.links` (plus `<db>.transitions.names.links`). Every
//! invocation re-read that whole store and rewrote it on save, so each write
//! got slower as the history grew. The journal is now a plain text file,
//! `<db>.transitions.log`, that a write appends to.
//!
//! Opening the journal imports a legacy store once: its entries are written
//! to a temporary file that is synced and renamed into place before the
//! legacy files are removed, so a crash leaves either the legacy store or a
//! complete journal. An explicit `--transactions-file` that still holds a
//! links store is converted in place the same way.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use crate::error::LinkError;
use crate::named_types::NamedTypesDecorator;

use super::log::{FileTransitionLog, TransitionLogStore};

/// Every journal entry starts with this, so a journal file does too.
const ENTRY_PREFIX: &[u8] = b"__transactions:";

/// Conventional journal path: `<stem>.transitions.log` next to `database`.
pub fn make_transitions_log_filename<P: AsRef<Path>>(database_filename: P) -> PathBuf {
    sibling(database_filename.as_ref(), "transitions.log")
}

/// Path of the links-backed journal older versions kept:
/// `<stem>.transitions.links`.
pub fn make_legacy_transitions_filename<P: AsRef<Path>>(database_filename: P) -> PathBuf {
    sibling(database_filename.as_ref(), "transitions.links")
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_default();
    let name = format!("{stem}.{suffix}");
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
        _ => PathBuf::from(name),
    }
}

/// Opens the journal of `database`: `explicit` when given, the conventional
/// `<stem>.transitions.log` otherwise, importing a legacy links-backed
/// journal first (see the module documentation).
pub fn open_transitions_journal(
    database: &Path,
    explicit: Option<&Path>,
    trace: bool,
) -> Result<FileTransitionLog, LinkError> {
    let path = match explicit {
        Some(path) => {
            if is_links_journal(path)? {
                import_links_journal(path, path, trace)?;
            }
            path.to_path_buf()
        }
        None => {
            let path = make_transitions_log_filename(database);
            let legacy = make_legacy_transitions_filename(database);
            if legacy.exists() {
                import_links_journal(&legacy, &path, trace)?;
            }
            path
        }
    };
    FileTransitionLog::open(path)
}

/// Whether `path` holds something other than a text journal: a non-empty
/// file whose first bytes are not (a prefix of) the journal entry prefix.
pub fn is_links_journal(path: &Path) -> Result<bool, LinkError> {
    let mut head = Vec::with_capacity(ENTRY_PREFIX.len());
    match File::open(path) {
        Ok(file) => {
            file.take(ENTRY_PREFIX.len() as u64)
                .read_to_end(&mut head)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error.into()),
    }
    Ok(!head.is_empty() && !ENTRY_PREFIX.starts_with(&head))
}

/// Copies every entry of the links-backed journal at `legacy` into the text
/// journal at `target`, then removes the legacy store. Returns whether
/// anything was imported.
///
/// When `target` already exists (a crash after publishing it), the legacy
/// store is removed only if each of its entries is already in `target`.
pub fn import_links_journal(legacy: &Path, target: &Path, trace: bool) -> Result<bool, LinkError> {
    let names = NamedTypesDecorator::make_names_database_filename(legacy);
    let entries = {
        let mut store = NamedTypesDecorator::with_names_database_path(legacy, &names, false)
            .map_err(|error| LinkError::StorageError(format!("{error:#}")))?
            .with_indexed_lookups();
        store.read_log_entries()?
    };
    let in_place = legacy == target;
    if !in_place && target.exists() {
        let mut existing = FileTransitionLog::open(target)?;
        let present: std::collections::HashSet<String> =
            existing.read_log_entries()?.into_iter().collect();
        if entries.iter().all(|entry| present.contains(entry)) {
            remove_legacy(legacy, &names)?;
        } else if trace {
            crate::diagnostics::stderr!(
                "[Transactions] Kept legacy journal {}: {} already exists.",
                legacy.display(),
                target.display()
            );
        }
        return Ok(false);
    }
    let mut temporary = target.as_os_str().to_os_string();
    temporary.push(".import");
    let temporary = PathBuf::from(temporary);
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temporary)?;
        for entry in &entries {
            writeln!(file, "{entry}")?;
        }
        file.sync_all()?;
    }
    std::fs::rename(&temporary, target)?;
    if !in_place {
        remove_legacy(legacy, &names)?;
    } else {
        remove_if_exists(&names)?;
    }
    FileTransitionLog::open(target)?; // syncs the directory entries
    if trace {
        crate::diagnostics::stderr!(
            "[Transactions] Imported {} entries from legacy journal {} into {}.",
            entries.len(),
            legacy.display(),
            target.display()
        );
    }
    Ok(true)
}

fn remove_legacy(legacy: &Path, names: &Path) -> Result<(), LinkError> {
    remove_if_exists(legacy)?;
    remove_if_exists(names)
}

fn remove_if_exists(path: &Path) -> Result<(), LinkError> {
    match std::fs::remove_file(path) {
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
        _ => Ok(()),
    }
}
