//! `--lock pid`: a lock directory, `<db>.lock.d`, holding an `owner` record.
//!
//! The directory is built under a private name with its record inside, then
//! renamed to `<db>.lock.d`. A rename onto an existing directory fails, so
//! exactly one process publishes the lock, and a published lock always has
//! its record. The operating system does not release it when the owner
//! exits, so a waiter breaks the lock when the record names a process on
//! this host, in this PID namespace, that is gone. Works where `flock` does
//! not, such as on some network file systems.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use super::owner::{Liveness, LockOwner};
use crate::diagnostics;
use crate::storage::lock::unique_token;

/// A directory without a readable record is treated as abandoned after this
/// long (an owner crashed between creating and renaming it, or another tool
/// made it).
const UNOWNED_GRACE: Duration = Duration::from_secs(10);
const OWNER_FILE: &str = "owner";
const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_LOCK_VIOLATION: i32 = 33;

/// `<db>.lock.d`.
pub fn lock_dir_path(store: &Path) -> PathBuf {
    sibling(store, ".lock.d")
}

/// A held `--lock pid` lock; dropping it removes the directory.
#[derive(Debug)]
pub struct PidLock {
    dir: PathBuf,
    token: String,
    trace: bool,
}

/// The outcome of one attempt.
pub enum Attempt {
    Acquired(PidLock),
    /// Held by the owner of this record, or by a directory without one.
    Held(Option<LockOwner>),
}

/// The owner record of `dir`, if it has a readable one.
pub fn read_owner(dir: &Path) -> Option<LockOwner> {
    LockOwner::parse(&fs::read_to_string(dir.join(OWNER_FILE)).ok()?)
}

/// Why the lock in `dir` may be broken, if it may.
pub fn stale_reason(dir: &Path, owner: Option<&LockOwner>) -> Option<String> {
    match owner {
        Some(owner) => match owner.liveness() {
            Liveness::Gone(reason) => Some(reason),
            Liveness::Alive | Liveness::Unknown(_) => None,
        },
        None => {
            let age = fs::metadata(dir)
                .and_then(|metadata| metadata.modified())
                .ok()
                .and_then(|modified| SystemTime::now().duration_since(modified).ok())?;
            (age >= UNOWNED_GRACE)
                .then(|| format!("no owner record for {} s", UNOWNED_GRACE.as_secs()))
        }
    }
}

/// Publishes the lock once, breaking a stale one first.
pub fn try_acquire(dir: &Path, trace: bool) -> std::io::Result<Attempt> {
    if let Some(parent) = dir.parent().filter(|parent| !parent.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let token = unique_token();
    let owner = LockOwner::current(&token);
    let mut last_error = None;
    for _ in 0..4 {
        let staging = sibling(dir, &format!(".{}.{}.tmp", owner.pid, unique_token()));
        fs::create_dir(&staging)?;
        let published = write_owner(&staging, &owner).and_then(|()| fs::rename(&staging, dir));
        let error = match published {
            Ok(()) => {
                return Ok(Attempt::Acquired(PidLock {
                    dir: dir.to_path_buf(),
                    token,
                    trace,
                }))
            }
            Err(error) => error,
        };
        let _ = retry_cleanup(|| fs::remove_dir_all(&staging), trace);
        if fs::symlink_metadata(dir).is_err() {
            // Released between the rename and this check, or a real error.
            last_error = Some(error);
            continue;
        }
        let holder = read_owner(dir);
        match stale_reason(dir, holder.as_ref()) {
            Some(reason) => {
                if trace {
                    diagnostics::stderr!("[TRACE] Breaking stale lock {}: {reason}", dir.display());
                }
                break_stale(dir, holder.as_ref(), trace);
            }
            None => return Ok(Attempt::Held(holder)),
        }
    }
    match last_error {
        Some(error) => Err(error),
        None => Ok(Attempt::Held(read_owner(dir))),
    }
}

/// Moves a stale lock aside and removes it. Should a live owner have
/// published a new lock in between, its directory is moved back. A lock
/// that cannot be moved (another waiter moved it first, or the directory is
/// not writable) is left alone: the next attempt sees what became of it.
fn break_stale(dir: &Path, expected: Option<&LockOwner>, trace: bool) {
    let tombstone = sibling(
        dir,
        &format!(".{}.{}.stale", std::process::id(), unique_token()),
    );
    if fs::rename(dir, &tombstone).is_err() {
        return;
    }
    let moved = read_owner(&tombstone);
    if moved.as_ref().map(|owner| &owner.token) == expected.map(|owner| &owner.token) {
        let _ = retry_cleanup(|| fs::remove_dir_all(&tombstone), trace);
    } else if fs::rename(&tombstone, dir).is_err() {
        // A third process published meanwhile; the moved owner has lost its
        // lock. Keep its directory for inspection.
    }
}

fn write_owner(dir: &Path, owner: &LockOwner) -> std::io::Result<()> {
    let mut file = fs::File::create(dir.join(OWNER_FILE))?;
    file.write_all(owner.to_record().as_bytes())?;
    file.sync_all()
}

impl PidLock {
    pub fn path(&self) -> &Path {
        &self.dir
    }
}

impl Drop for PidLock {
    fn drop(&mut self) {
        // Only remove the lock while it is still ours.
        if read_owner(&self.dir).is_some_and(|owner| owner.token == self.token) {
            let tombstone = sibling(&self.dir, &format!(".{}.released", self.token));
            if retry_cleanup(|| fs::rename(&self.dir, &tombstone), self.trace).is_ok() {
                let _ = retry_cleanup(|| fs::remove_dir_all(&tombstone), self.trace);
            }
        }
    }
}

fn retry_cleanup(
    mut cleanup: impl FnMut() -> std::io::Result<()>,
    trace: bool,
) -> std::io::Result<()> {
    let mut delay_ms = 5;
    loop {
        let error = match cleanup() {
            Ok(()) => return Ok(()),
            Err(error) => error,
        };
        if trace {
            diagnostics::stderr!("[TRACE] PID-lock cleanup: {error}");
        }
        // Windows may block renaming a directory while a waiter reads its
        // owner record. The four delays add at most 75 ms per operation.
        if !cfg!(windows)
            || delay_ms > 40
            || !matches!(
                error.raw_os_error(),
                Some(ERROR_ACCESS_DENIED | ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)
            )
        {
            return Err(error);
        }
        std::thread::sleep(Duration::from_millis(delay_ms));
        delay_ms *= 2;
    }
}

fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::retry_cleanup;
    use std::io::Error;

    #[test]
    fn pid_cleanup_retries_windows_sharing_errors_only() {
        let mut attempts = 0;
        let result = retry_cleanup(
            || {
                attempts += 1;
                if attempts == 1 {
                    return Err(Error::from_raw_os_error(32));
                }
                Ok(())
            },
            false,
        );
        assert_eq!(result.is_ok(), cfg!(windows));
        assert_eq!(attempts, if cfg!(windows) { 2 } else { 1 });

        attempts = 0;
        let error = retry_cleanup(
            || {
                attempts += 1;
                Err(Error::from_raw_os_error(2))
            },
            false,
        )
        .unwrap_err();
        assert_eq!(error.raw_os_error(), Some(2));
        assert_eq!(attempts, 1);
    }
}
