//! An optional log decorator that remembers when each revision was written, so
//! that a restore can name a time. See `docs/ENGINE-BACKUP.md`.
use super::error;
use crate::{LinkError, ReplaceableTransitionLogStore, TransitionLogStore};
use std::time::{SystemTime, UNIX_EPOCH};

fn system_clock() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

fn revision_of(record: &str) -> Result<u64, LinkError> {
    record
        .strip_prefix("engine")
        .and_then(|rest| rest.split('|').nth(1))
        .and_then(|revision| revision.parse().ok())
        .ok_or_else(|| error("cannot timestamp a record without a revision"))
}

/// What is still true of a timeline: `(revision, written)` oldest first. A revision
/// written again after a crash lost it replaces what the lost one and its
/// successors claimed.
fn surviving<M: TransitionLogStore>(timeline: &mut M) -> Result<Vec<(u64, u64)>, LinkError> {
    let mut entries: Vec<(u64, u64)> = Vec::new();
    for entry in timeline.read_log_entries()? {
        let parsed = entry
            .split_once('|')
            .and_then(|(revision, written)| Some((revision.parse().ok()?, written.parse().ok()?)))
            .ok_or_else(|| error("malformed timeline record"))?;
        while entries.last().is_some_and(|last| last.0 >= parsed.0) {
            entries.pop();
        }
        entries.push(parsed);
    }
    Ok(entries)
}

/// Keeps `revision|unix milliseconds` in a second log for every record appended to
/// `inner`. Records and their format are unchanged, so an engine or a backup never
/// needs to know this decorator exists, and the cost is one more append and flush.
///
/// The record is written and flushed first, so a crash can lose a timestamp but
/// never leave one for a record that is not there. A commit without a timestamp
/// makes a restore pick an earlier revision. Time never runs backwards in a
/// timeline: a clock that steps back repeats the latest time instead.
pub struct TimedLog<L, M> {
    inner: L,
    timeline: M,
    clock: Box<dyn FnMut() -> u64 + Send>,
    latest: u64,
}

impl<L: TransitionLogStore, M: TransitionLogStore> TimedLog<L, M> {
    /// Timestamps from the system clock.
    pub fn new(inner: L, timeline: M) -> Result<Self, LinkError> {
        Self::with_clock(inner, timeline, system_clock)
    }

    /// Timestamps from `clock`, in the milliseconds `revision_at` will be asked for.
    pub fn with_clock(
        inner: L,
        mut timeline: M,
        clock: impl FnMut() -> u64 + Send + 'static,
    ) -> Result<Self, LinkError> {
        let latest = surviving(&mut timeline)?.last().map_or(0, |entry| entry.1);
        Ok(Self {
            inner,
            timeline,
            clock: Box::new(clock),
            latest,
        })
    }
}

impl<L: TransitionLogStore, M: TransitionLogStore> TransitionLogStore for TimedLog<L, M> {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        let revision = revision_of(entry)?;
        self.inner.append_log_entry(entry)?;
        self.latest = self.latest.max((self.clock)());
        self.timeline
            .append_log_entry(&format!("{revision}|{}", self.latest))
    }

    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.inner.read_log_entries()
    }

    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.inner.flush_log()?;
        self.timeline.flush_log()
    }

    fn supports_durable_flush(&self) -> bool {
        self.inner.supports_durable_flush() && self.timeline.supports_durable_flush()
    }
}

/// Compaction does not touch the timeline: its checkpoint repeats a revision the
/// timeline already holds, and revisions it dropped are then refused by a restore.
impl<L: ReplaceableTransitionLogStore, M: TransitionLogStore> ReplaceableTransitionLogStore
    for TimedLog<L, M>
{
    fn supports_replacement(&self) -> bool {
        self.inner.supports_replacement()
    }

    fn replace_log_entries(&mut self, entries: &[String]) -> Result<bool, LinkError> {
        self.inner.replace_log_entries(entries)
    }
}

/// The latest revision written by `time`, for `restore_backup`. Refuses a time
/// before the first timestamp, because the state then is not known.
pub fn revision_at<M: TransitionLogStore>(timeline: &mut M, time: u64) -> Result<u64, LinkError> {
    surviving(timeline)?
        .iter()
        .rev()
        .find(|entry| entry.1 <= time)
        .map(|entry| entry.0)
        .ok_or_else(|| error("no revision was written by that time"))
}
