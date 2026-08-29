//! Advisory file locking for multi-process access to a links database.
//!
//! Uses the standard library's [`std::fs::File`] advisory locks (`flock`
//! on Unix, `LockFileEx` on Windows). Locks are taken on a dedicated
//! sidecar `*.lock` file rather than on the database itself, so that the
//! lock survives operations that rewrite or remap the database file.
//!
//! Advisory locks are held per *open file description*, which means two
//! [`FileLock`] values inside a single process contend exactly the same
//! way two separate processes do.

use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use crate::error::LinkError;

/// Requested sharing mode for a [`FileLock`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LockMode {
    /// Multiple readers may hold the lock at the same time.
    Shared,
    /// Only one holder at a time; excludes all shared holders.
    Exclusive,
}

/// Conventional sidecar lock filename for a links database.
pub fn lock_file_path<P: AsRef<Path>>(database_filename: P) -> PathBuf {
    let path = database_filename.as_ref();
    let mut name = path
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".lock");
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.join(name),
        _ => PathBuf::from(name),
    }
}

/// RAII guard around an advisory lock on a sidecar lock file.
///
/// The lock is released when the guard is dropped (and, as a backstop,
/// by the operating system when the process exits — so a crashed writer
/// never leaves a database permanently locked).
#[derive(Debug)]
pub struct FileLock {
    file: File,
    path: PathBuf,
    mode: LockMode,
}

impl FileLock {
    /// Acquires the lock, blocking until it becomes available.
    pub fn acquire<P: AsRef<Path>>(lock_path: P, mode: LockMode) -> Result<Self, LinkError> {
        let (file, path) = Self::open(lock_path)?;
        let result = match mode {
            LockMode::Shared => file.lock_shared(),
            LockMode::Exclusive => file.lock(),
        };
        result.map_err(|error| {
            LinkError::Lock(format!("failed to lock {}: {error}", path.display()))
        })?;
        Ok(Self { file, path, mode })
    }

    /// Tries to acquire the lock, returning `Ok(None)` when another
    /// holder currently owns a conflicting lock.
    pub fn try_acquire<P: AsRef<Path>>(
        lock_path: P,
        mode: LockMode,
    ) -> Result<Option<Self>, LinkError> {
        let (file, path) = Self::open(lock_path)?;
        let result = match mode {
            LockMode::Shared => file.try_lock_shared(),
            LockMode::Exclusive => file.try_lock(),
        };
        match result {
            Ok(()) => Ok(Some(Self { file, path, mode })),
            Err(TryLockError::WouldBlock) => Ok(None),
            Err(TryLockError::Error(error)) => Err(LinkError::Lock(format!(
                "failed to lock {}: {error}",
                path.display()
            ))),
        }
    }

    /// The sidecar lock file this guard holds.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// The sharing mode this guard was acquired with.
    pub fn mode(&self) -> LockMode {
        self.mode
    }

    fn open<P: AsRef<Path>>(lock_path: P) -> Result<(File, PathBuf), LinkError> {
        let path = lock_path.as_ref().to_path_buf();
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() && !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(&path)?;
        Ok((file, path))
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
