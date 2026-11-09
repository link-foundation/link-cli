//! Check and repair a damaged engine journal. `Engine::open` refuses a journal
//! with any bad record; these functions apply the same rules to report the
//! longest prefix that still replays, and to rebuild a journal from it or from
//! an intact copy. See `docs/ENGINE-RECOVERY.md` for what each step can and
//! cannot get back.
use super::{EngineOptions, State};
use crate::LinkError;
use doublets::data::LinkReference;

/// The first record that does not replay, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalDamage {
    /// Zero-based index of the record.
    pub record: usize,
    pub reason: String,
}

/// What `Engine::open` would make of a journal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalCheck {
    /// Records that replay, counted from the first. They form the valid prefix.
    pub valid_records: usize,
    pub total_records: usize,
    /// Revision after the valid prefix; zero when nothing replays.
    pub revision: u64,
    /// `None` when every record replays.
    pub damage: Option<JournalDamage>,
}

impl JournalCheck {
    pub fn is_intact(&self) -> bool {
        self.damage.is_none()
    }
    /// Records after the valid prefix that a repair from this copy drops.
    pub fn discarded_records(&self) -> usize {
        self.total_records - self.valid_records
    }
}

/// Replay `entries` with the checksum, format, revision, allocator and
/// selected constraint rules of `Engine::open`. Never fails: damage is part of
/// the report. Costs the same O(N) replay as opening, without a log.
pub fn check_journal<T: LinkReference>(
    entries: &[String],
    options: &EngineOptions,
) -> JournalCheck {
    let mut state = State::<T, ()>::empty((), options.clone());
    for (index, entry) in entries.iter().enumerate() {
        if let Err(error) = state.replay(index, entry) {
            return JournalCheck {
                valid_records: index,
                total_records: entries.len(),
                revision: state.current.revision,
                damage: Some(JournalDamage {
                    record: index,
                    reason: error.to_string(),
                }),
            };
        }
    }
    JournalCheck {
        valid_records: entries.len(),
        total_records: entries.len(),
        revision: state.current.revision,
        damage: None,
    }
}

/// The journal a repair keeps, and where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JournalRecovery {
    /// The longest valid prefix among the copies.
    pub entries: Vec<String>,
    /// Index of the copy that supplied `entries`; the first wins a tie.
    pub source: usize,
    /// Revision after `entries`.
    pub revision: u64,
    /// One check per copy, in the order given.
    pub checks: Vec<JournalCheck>,
}

/// Pick the longest valid prefix among copies of one journal: the damaged
/// primary first, then replicas, mirrors or a restored backup. Every other
/// copy's valid prefix must be a prefix of the chosen one. Copies that disagree
/// are refused rather than guessed between, because each may hold commits the
/// other lacks; choose one explicitly. A copy compacted into an engine3
/// checkpoint disagrees with an uncompacted copy of the same history.
pub fn recover_journal<T: LinkReference>(
    copies: &[Vec<String>],
    options: &EngineOptions,
) -> Result<JournalRecovery, LinkError> {
    if copies.is_empty() {
        return Err(LinkError::Transaction(
            "journal recovery needs at least one copy".into(),
        ));
    }
    let checks = copies
        .iter()
        .map(|copy| check_journal::<T>(copy, options))
        .collect::<Vec<_>>();
    let mut source = 0;
    for (index, check) in checks.iter().enumerate() {
        if check.valid_records > checks[source].valid_records {
            source = index;
        }
    }
    let chosen = &copies[source][..checks[source].valid_records];
    for (index, check) in checks.iter().enumerate() {
        if copies[index][..check.valid_records] != chosen[..check.valid_records] {
            return Err(LinkError::Transaction(format!(
                "journal copies {source} and {index} diverge; choose one explicitly"
            )));
        }
    }
    Ok(JournalRecovery {
        entries: chosen.to_vec(),
        source,
        revision: checks[source].revision,
        checks,
    })
}

#[cfg(not(target_arch = "wasm32"))]
pub use file::{read_journal_file_lossy, repair_journal_file, JournalRepair};

#[cfg(not(target_arch = "wasm32"))]
mod file {
    use super::{recover_journal, JournalRecovery};
    use crate::engine::EngineOptions;
    use crate::LinkError;
    use doublets::data::LinkReference;
    use std::fs::{File, OpenOptions};
    use std::io::{ErrorKind, Read, Write};
    use std::path::{Path, PathBuf};

    /// Complete lines of a file journal, in the form `FileTransitionLog` reads
    /// them, but tolerating damage it would reject: invalid UTF-8 becomes U+FFFD
    /// (so the checksum flags that record) and a missing file is empty.
    pub fn read_journal_file_lossy(path: &Path) -> Result<Vec<String>, LinkError> {
        Ok(lines(&read_bytes(path)?))
    }

    fn read_bytes(path: &Path) -> Result<Vec<u8>, LinkError> {
        let mut bytes = Vec::new();
        match File::open(path) {
            Ok(mut file) => {
                file.read_to_end(&mut bytes)?;
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        Ok(bytes)
    }

    fn lines(bytes: &[u8]) -> Vec<String> {
        let complete = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |end| end + 1);
        bytes[..complete]
            .split(|byte| *byte == b'\n')
            .filter(|line| !line.is_empty())
            .map(|line| String::from_utf8_lossy(line).into_owned())
            .collect()
    }

    /// Outcome of `repair_journal_file`.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct JournalRepair {
        pub recovery: JournalRecovery,
        /// False when the file already held exactly the recovered journal.
        pub changed: bool,
        /// Where the original bytes were kept before the file was replaced.
        pub preserved: Option<PathBuf>,
    }

    /// Rebuild the journal at `path` from the longest valid prefix among it and
    /// `copies` (see `recover_journal`). The original bytes are first copied to
    /// `<path>.damaged-<n>`; the replacement is flushed, then renamed over
    /// `path`. Holds the writer lock that `FileTransitionLog::open_exclusive`
    /// takes, so it fails while an engine owns the journal. Copies are only
    /// read; their owners must not be writing them.
    pub fn repair_journal_file<T: LinkReference>(
        path: &Path,
        copies: &[&Path],
        options: &EngineOptions,
    ) -> Result<JournalRepair, LinkError> {
        let _lock =
            crate::FileLock::try_acquire(crate::lock_file_path(path), crate::LockMode::Exclusive)?
                .ok_or_else(|| LinkError::Lock("journal writer is busy".into()))?;
        let original = read_bytes(path)?;
        let mut sources = vec![lines(&original)];
        for copy in copies {
            sources.push(read_journal_file_lossy(copy)?);
        }
        let recovery = recover_journal::<T>(&sources, options)?;
        let mut repaired = Vec::new();
        for entry in &recovery.entries {
            repaired.extend_from_slice(entry.as_bytes());
            repaired.push(b'\n');
        }
        if repaired == original {
            return Ok(JournalRepair {
                recovery,
                changed: false,
                preserved: None,
            });
        }
        let preserved = if original.is_empty() {
            None
        } else {
            Some(preserve(path, &original)?)
        };
        let temporary = sibling(path, ".repair");
        {
            let mut file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(&temporary)?;
            file.write_all(&repaired)?;
            file.sync_all()?;
        }
        std::fs::rename(&temporary, path)?;
        sync_directory(path)?;
        Ok(JournalRepair {
            recovery,
            changed: true,
            preserved,
        })
    }

    fn sibling(path: &Path, suffix: &str) -> PathBuf {
        let mut name = path.as_os_str().to_owned();
        name.push(suffix);
        PathBuf::from(name)
    }

    /// Claim the first free `<path>.damaged-<n>` so no earlier evidence is lost.
    fn preserve(path: &Path, bytes: &[u8]) -> Result<PathBuf, LinkError> {
        for attempt in 1u32.. {
            let target = sibling(path, &format!(".damaged-{attempt}"));
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)
            {
                Ok(mut file) => {
                    file.write_all(bytes)?;
                    file.sync_all()?;
                    return Ok(target);
                }
                Err(error) if error.kind() == ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        unreachable!("u32 attempts exhausted")
    }

    #[cfg(unix)]
    fn sync_directory(path: &Path) -> Result<(), LinkError> {
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        File::open(parent)?.sync_all()?;
        Ok(())
    }

    #[cfg(not(unix))]
    fn sync_directory(_: &Path) -> Result<(), LinkError> {
        Ok(())
    }
}
