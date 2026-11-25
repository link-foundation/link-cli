//! Store locking for `clink` invocations (issue #194).
//!
//! Every invocation loads the whole store, changes it in memory and saves it
//! back. Two invocations that overlap would each save their own copy, and the
//! later save would drop the writes of the earlier one. A [`StoreLock`] makes
//! that read-modify-write exclusive: writers take it exclusively, readers
//! share it, and a waiter gives up with a "store is locked" error naming the
//! owner once `--lock-timeout` passes.
//!
//! | `--lock` | Lock                         | Released by                           |
//! | -------- | ---------------------------- | ------------------------------------- |
//! | `flock`  | `flock`/`LockFileEx` on `<db>.lock` | the operating system when the holder exits |
//! | `pid`    | the directory `<db>.lock.d`  | the holder; a waiter breaks it when the owner process is gone |
//! | `none`   | nothing                      | -                                     |
//!
//! An exclusive `flock` holder writes its owner record to `<db>.lock.owner`
//! and removes it on release, so that waiters and `--lock-status` can name
//! it. The record is a file of its own because on Windows the lock keeps
//! every other process from reading `<db>.lock` itself.

mod owner;
mod pid_lock;

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use owner::{Liveness, LockOwner};
pub use pid_lock::lock_dir_path;

use crate::diagnostics;
use crate::error::LinkError;
use crate::storage::lock::unique_token;
use crate::storage::{lock_file_path, FileLock, LockMode};
use pid_lock::{Attempt, PidLock};

/// How long a waiter waits by default, in milliseconds.
pub const DEFAULT_LOCK_TIMEOUT_MS: u64 = 30_000;

/// The `--lock` mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreLockKind {
    Flock,
    Pid,
    None,
}

/// The parsed `--lock`, `--no-lock` and `--lock-timeout` options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoreLockOptions {
    pub kind: StoreLockKind,
    /// How long to wait for a held lock; zero tries once.
    pub timeout: Duration,
    /// Report waiting, breaking and acquiring on stderr.
    pub trace: bool,
}

impl Default for StoreLockOptions {
    fn default() -> Self {
        Self {
            kind: StoreLockKind::Flock,
            timeout: Duration::from_millis(DEFAULT_LOCK_TIMEOUT_MS),
            trace: false,
        }
    }
}

impl StoreLockOptions {
    /// Reads the raw values of `--lock` and `--lock-timeout`.
    pub fn parse(kind: Option<&str>, timeout: Option<&str>) -> Result<Self, LinkError> {
        let mut options = Self::default();
        if let Some(value) = kind.map(str::trim).filter(|value| !value.is_empty()) {
            options.kind = match value.to_ascii_lowercase().as_str() {
                "flock" => StoreLockKind::Flock,
                "pid" => StoreLockKind::Pid,
                "none" => StoreLockKind::None,
                _ => {
                    return Err(LinkError::InvalidFormat(format!(
                        "Invalid --lock value '{value}'. Use 'flock', 'pid' or 'none'."
                    )))
                }
            };
        }
        if let Some(value) = timeout.map(str::trim) {
            let milliseconds = value.parse::<u64>().map_err(|_| {
                LinkError::InvalidFormat(format!(
                    "Invalid --lock-timeout value '{value}'. Use a whole number of milliseconds."
                ))
            })?;
            options.timeout = Duration::from_millis(milliseconds);
        }
        Ok(options)
    }
}

/// The stores of one invocation without duplicates, in the order every
/// process locks them: sorted by absolute path. Each keeps the spelling it
/// was given, which is what messages show.
pub fn lock_order<I, P>(stores: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut paths: Vec<(PathBuf, PathBuf)> = stores
        .into_iter()
        .map(|store| (normalize(store.as_ref()), store.as_ref().to_path_buf()))
        .collect();
    paths.sort_by(|left, right| left.0.cmp(&right.0));
    paths.dedup_by(|later, earlier| later.0 == earlier.0);
    paths.into_iter().map(|(_, store)| store).collect()
}

fn normalize(store: &Path) -> PathBuf {
    let absolute = std::path::absolute(store).unwrap_or_else(|_| store.to_path_buf());
    match (absolute.parent(), absolute.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map(|parent| parent.join(name))
            .unwrap_or(absolute),
        _ => absolute,
    }
}

#[derive(Debug)]
enum Guard {
    Flock(FlockGuard),
    Pid(PidLock),
}

/// Locks held for one invocation; dropping it releases them.
#[derive(Debug)]
pub struct StoreLock {
    guards: Vec<Guard>,
}

impl StoreLock {
    /// Locks every store in `stores` (see [`lock_order`]),
    /// waiting up to `options.timeout` for each. `--lock pid` is always
    /// exclusive.
    pub fn acquire(
        stores: &[PathBuf],
        mode: LockMode,
        options: &StoreLockOptions,
    ) -> Result<Self, LinkError> {
        // `None`: a timeout too long to reach, so wait for good.
        let deadline = Instant::now().checked_add(options.timeout);
        let mut guards = Vec::new();
        for store in stores {
            guards.push(match options.kind {
                StoreLockKind::None => continue,
                StoreLockKind::Flock => {
                    Guard::Flock(acquire_flock(store, mode, deadline, options)?)
                }
                StoreLockKind::Pid => Guard::Pid(acquire_pid(store, deadline, options)?),
            });
        }
        Ok(Self { guards })
    }

    /// The lock files and directories held.
    pub fn paths(&self) -> Vec<&Path> {
        self.guards
            .iter()
            .map(|guard| match guard {
                Guard::Flock(guard) => guard.lock.path(),
                Guard::Pid(lock) => lock.path(),
            })
            .collect()
    }
}

/// Waits from 1 ms, doubling up to 50 ms, until `deadline`.
struct Backoff {
    delay: Duration,
    deadline: Option<Instant>,
}

impl Backoff {
    fn new(deadline: Option<Instant>) -> Self {
        Self {
            delay: Duration::from_millis(1),
            deadline,
        }
    }

    /// Sleeps before the next attempt; false once the deadline has passed.
    fn wait(&mut self) -> bool {
        let left = match self.deadline {
            Some(deadline) => match deadline.checked_duration_since(Instant::now()) {
                Some(left) if !left.is_zero() => left,
                _ => return false,
            },
            None => self.delay,
        };
        std::thread::sleep(self.delay.min(left));
        self.delay = (self.delay * 2).min(Duration::from_millis(50));
        true
    }
}

fn locked_error(path: &Path, kind: &str, holder: &str, options: &StoreLockOptions) -> LinkError {
    LinkError::Lock(format!(
        "store is locked: {} is held by {holder}; gave up waiting for the {kind} lock after {} ms (--lock-timeout)",
        path.display(),
        options.timeout.as_millis()
    ))
}

/// `<db>.lock.owner`: the record of the exclusive `flock` holder.
pub fn owner_file_path(store: &Path) -> PathBuf {
    let mut name = lock_file_path(store).into_os_string();
    name.push(".owner");
    PathBuf::from(name)
}

/// Replaces the record at `path` at once, so that no reader sees half of it.
/// Best effort: the record only serves diagnostics.
fn write_record(path: &Path, record: &str) {
    let mut staging = path.as_os_str().to_owned();
    staging.push(format!(".{}.tmp", unique_token()));
    let staging = PathBuf::from(staging);
    if fs::write(&staging, record)
        .and_then(|()| fs::rename(&staging, path))
        .is_err()
    {
        let _ = fs::remove_file(&staging);
    }
}

/// An exclusive or shared `flock` on `<db>.lock`. An exclusive holder keeps
/// its owner record in `<db>.lock.owner` and removes it on release.
#[derive(Debug)]
struct FlockGuard {
    lock: FileLock,
    owner: PathBuf,
}

impl Drop for FlockGuard {
    fn drop(&mut self) {
        // Before the lock itself is released, with the fields.
        if self.lock.mode() == LockMode::Exclusive {
            let _ = fs::remove_file(&self.owner);
        }
    }
}

fn acquire_flock(
    store: &Path,
    mode: LockMode,
    deadline: Option<Instant>,
    options: &StoreLockOptions,
) -> Result<FlockGuard, LinkError> {
    let path = lock_file_path(store);
    let kind = match mode {
        LockMode::Shared => "shared flock",
        LockMode::Exclusive => "exclusive flock",
    };
    let mut backoff = Backoff::new(deadline);
    let mut reported = false;
    loop {
        if let Some(lock) = FileLock::try_acquire(&path, mode)? {
            let owner = owner_file_path(store);
            match mode {
                LockMode::Exclusive => {
                    write_record(&owner, &LockOwner::current(&unique_token()).to_record())
                }
                // No exclusive holder exists, so a record is a crashed one's.
                LockMode::Shared => {
                    let _ = fs::remove_file(&owner);
                }
            }
            let guard = FlockGuard { lock, owner };
            if options.trace {
                diagnostics::stderr!("[TRACE] Acquired {kind} lock {}", path.display());
            }
            return Ok(guard);
        }
        if options.trace && !reported {
            reported = true;
            diagnostics::stderr!(
                "[TRACE] Waiting for {kind} lock {}: held by {}",
                path.display(),
                flock_holder(store)
            );
        }
        if !backoff.wait() {
            return Err(locked_error(&path, kind, &flock_holder(store), options));
        }
    }
}

/// Who holds the `<db>.lock` of `store`: shared readers, or the writer its
/// record names.
fn flock_holder(store: &Path) -> String {
    if let Ok(Some(_probe)) = FileLock::try_acquire(lock_file_path(store), LockMode::Shared) {
        return "shared readers".into();
    }
    let text = fs::read_to_string(owner_file_path(store)).unwrap_or_default();
    match LockOwner::parse(&text) {
        Some(owner) => format!("an exclusive writer: {}", owner.describe()),
        None => "an exclusive writer that left no owner record".into(),
    }
}

fn acquire_pid(
    store: &Path,
    deadline: Option<Instant>,
    options: &StoreLockOptions,
) -> Result<PidLock, LinkError> {
    let dir = lock_dir_path(store);
    let mut backoff = Backoff::new(deadline);
    let mut reported = false;
    loop {
        let holder = match pid_lock::try_acquire(&dir, options.trace)? {
            Attempt::Acquired(lock) => {
                if options.trace {
                    diagnostics::stderr!("[TRACE] Acquired pid lock {}", dir.display());
                }
                return Ok(lock);
            }
            Attempt::Held(owner) => describe_pid_holder(&dir, owner.as_ref()),
        };
        if options.trace && !reported {
            reported = true;
            diagnostics::stderr!(
                "[TRACE] Waiting for pid lock {}: held by {holder}",
                dir.display()
            );
        }
        if !backoff.wait() {
            return Err(locked_error(&dir, "pid", &holder, options));
        }
    }
}

fn describe_pid_holder(dir: &Path, owner: Option<&LockOwner>) -> String {
    match owner {
        Some(owner) => owner.describe(),
        None => format!(
            "a directory without an owner record (removed once {} is 10 s old)",
            dir.display()
        ),
    }
}

/// `--lock-status`: one line per store and lock kind, without taking or
/// creating any lock that is not already there.
pub fn lock_status(stores: &[PathBuf]) -> String {
    let mut report = String::new();
    for store in stores {
        let path = lock_file_path(store);
        let state = if !path.exists() {
            "unlocked".to_string()
        } else {
            match FileLock::try_acquire(&path, LockMode::Exclusive) {
                Ok(Some(_probe)) => "unlocked".to_string(),
                Ok(None) => format!("locked by {}", flock_holder(store)),
                Err(error) => format!("unknown ({error})"),
            }
        };
        report.push_str(&format!("flock {}: {state}\n", path.display()));
        let dir = lock_dir_path(store);
        let state = if fs::symlink_metadata(&dir).is_err() {
            "unlocked".to_string()
        } else {
            let owner = pid_lock::read_owner(&dir);
            let mut state = format!("locked by {}", describe_pid_holder(&dir, owner.as_ref()));
            if let Some(reason) = pid_lock::stale_reason(&dir, owner.as_ref()) {
                state.push_str(&format!("; stale, the next writer breaks it: {reason}"));
            }
            state
        };
        report.push_str(&format!("pid {}: {state}\n", dir.display()));
    }
    report
}
