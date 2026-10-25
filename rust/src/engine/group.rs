//! Opt-in coordination of independent sessions at one durability boundary.
use super::{error, Engine, EngineTransaction, MemoryLog, State};
use crate::{LinkError, TransitionLogStore};
use doublets::data::LinkReference;
use std::sync::Arc;

/// One independent session's result, in the caller's input order.
#[derive(Debug, PartialEq, Eq)]
pub enum GroupCommitOutcome {
    /// Acknowledged after the shared flush (or no journal I/O was necessary).
    Committed(u64),
    /// Validation failed before this session appended any physical record.
    Aborted(String),
    /// No acknowledgement: reopening must discover which prefix persisted.
    Indeterminate(String),
}

impl<T: LinkReference, L: TransitionLogStore> Engine<T, L> {
    /// Consume independent sessions and share one flush across accepted writes.
    /// Validation follows input order and retains the configured isolation rules.
    /// Each write session retains its own revision; this is not an atomic batch.
    /// An uncertain append/flush fences the engine and retains its old visible
    /// state. All provisionally accepted members then return `Indeterminate`,
    /// including read-only members that may depend on a tentative revision.
    ///
    /// Foreign sessions cause an outer error before I/O; consumed sessions are
    /// rolled back on that path. A log that syncs on every append still performs
    /// those syncs; select buffered append with durable flush to amortize them.
    /// Private preparation retains history/map metadata and selected logical
    /// revision records until publication. Ordinary commits keep their own path.
    pub fn commit_group(
        &self,
        mut sessions: Vec<EngineTransaction<T, L>>,
    ) -> Result<Vec<GroupCommitOutcome>, LinkError> {
        if sessions
            .iter()
            .any(|tx| !Arc::ptr_eq(&self.state, &tx.engine.state))
        {
            return Err(error("group sessions must belong to the same engine"));
        }
        let mut state = self.lock()?;
        let mut options = state.options.clone();
        options.trace = false;
        let mut working = State {
            log: MemoryLog::default(),
            current: state.current.clone(),
            history: state.history.clone(),
            changed: state.changed.clone(),
            write_owners: state.write_owners.clone(),
            next: state.next,
            committed_next: state.committed_next,
            active: state.active,
            poisoned: false,
            options,
        };
        let mut outcomes = Vec::with_capacity(sessions.len());
        for tx in &mut sessions {
            let base = tx.base.clone();
            let outcome = match tx.commit_locked(&mut working) {
                Ok(revision) => GroupCommitOutcome::Committed(revision),
                Err(cause) => GroupCommitOutcome::Aborted(cause.to_string()),
            };
            tx.base = base;
            outcomes.push(outcome);
        }
        // Only ownership/admission cleanup is real until durability succeeds.
        // Read-committed sessions outside this group must retain the old view
        // if the physical log fails after writing an unknown prefix.
        state.active = working.active;
        state.write_owners = working.write_owners;
        let persisted = (|| {
            for record in &working.log.entries {
                state.log.append_log_entry(record)?;
            }
            if !working.log.entries.is_empty() {
                state.log.flush_log()?;
            }
            Ok::<(), LinkError>(())
        })();
        if let Err(cause) = persisted {
            state.poisoned = true;
            if state.active == 0 {
                state.changed.clear();
            }
            let message = format!("indeterminate group commit: {cause}; reopen the engine");
            for outcome in &mut outcomes {
                if matches!(outcome, GroupCommitOutcome::Committed(_)) {
                    *outcome = GroupCommitOutcome::Indeterminate(message.clone());
                }
            }
            return Ok(outcomes);
        }
        state.current = working.current;
        state.history = working.history;
        state.changed = working.changed;
        state.committed_next = working.committed_next;
        if state.options.trace && !working.log.entries.is_empty() {
            crate::diagnostics::stderr!(
                "[Engine] group committed revision={} records={}",
                state.current.revision,
                working.log.entries.len()
            );
        }
        Ok(outcomes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        engine::{ConcurrencyControl, EngineOptions},
        LinksStorage,
    };
    struct Unavailable;
    impl TransitionLogStore for Unavailable {
        fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
            Ok(vec![])
        }
        fn append_log_entry(&mut self, _: &str) -> Result<(), LinkError> {
            Err(error("append unavailable"))
        }
        fn flush_log(&mut self) -> Result<(), LinkError> {
            Ok(())
        }
    }
    #[test]
    fn uncertain_and_already_fenced_groups_release_admission_and_point_ownership() {
        for concurrency in [
            ConcurrencyControl::Exclusive,
            ConcurrencyControl::Pessimistic,
        ] {
            let db = Engine::<u32, _>::open(
                Unavailable,
                EngineOptions {
                    concurrency,
                    ..Default::default()
                },
            )
            .unwrap();
            let mut first = db.begin().unwrap();
            first.create_link(0, 0).unwrap();
            assert!(matches!(
                db.commit_group(vec![first]).unwrap()[0],
                GroupCommitOutcome::Indeterminate(_)
            ));
            let state = db.state.lock().unwrap();
            assert_eq!(state.active, 0);
            assert!(state.write_owners.is_empty());
            assert!(state.changed.is_empty());
            drop(state);
            // A group rejected at admission still consumes/rolls back its open
            // members. Use a fresh engine to retain an owner before fencing it.
            let db = Engine::<u32, _>::open(
                Unavailable,
                EngineOptions {
                    concurrency,
                    ..Default::default()
                },
            )
            .unwrap();
            let mut held = db.begin().unwrap();
            held.create_link(0, 0).unwrap();
            assert!(db.with_log::<()>(|_| Err(error("unknown"))).is_err());
            assert!(db.commit_group(vec![held]).is_err());
            let state = db.state.lock().unwrap();
            assert_eq!(state.active, 0);
            assert!(state.write_owners.is_empty());
        }
    }
}
