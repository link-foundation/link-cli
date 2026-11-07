//! Optional savepoints inside one staged session.
//!
//! A savepoint copies the private write set, so it costs O(staged writes) when
//! taken and nothing for a session that never takes one. Rolling back restores
//! rows only. Observed reads stay validated, because later work may depend on
//! them; point-write ownership stays until the session ends, and allocated
//! addresses stay retired, matching the gaps a complete rollback leaves.
use super::{error, EngineTransaction};
use crate::{LinkError, TransitionLogStore};
use doublets::data::LinkReference;
use std::sync::atomic::{AtomicU64, Ordering};

// Process-wide, so a token from another session can never select a savepoint.
static NEXT_SAVEPOINT: AtomicU64 = AtomicU64::new(1);

/// Names one position in the write set of the session that created it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Savepoint(pub u64);

impl<T: LinkReference, L: TransitionLogStore> EngineTransaction<T, L> {
    fn savepoint_position(&self, savepoint: Savepoint) -> Result<usize, LinkError> {
        self.check_open()?;
        self.savepoints
            .iter()
            .position(|(id, _)| *id == savepoint.0)
            .ok_or_else(|| error("unknown savepoint"))
    }
    /// Remember the current write set. Savepoints nest.
    pub fn savepoint(&mut self) -> Result<Savepoint, LinkError> {
        self.check_open()?;
        let id = NEXT_SAVEPOINT.fetch_add(1, Ordering::Relaxed);
        self.savepoints.push((id, self.writes.clone()));
        Ok(Savepoint(id))
    }
    /// Discard writes made after this savepoint and every later savepoint. The
    /// savepoint itself stays usable, as in PostgreSQL and SQLite.
    pub fn rollback_to_savepoint(&mut self, savepoint: Savepoint) -> Result<(), LinkError> {
        let position = self.savepoint_position(savepoint)?;
        self.savepoints.truncate(position + 1);
        self.writes = self.savepoints[position].1.clone();
        Ok(())
    }
    /// Forget this savepoint and every later one, keeping all writes.
    pub fn release_savepoint(&mut self, savepoint: Savepoint) -> Result<(), LinkError> {
        let position = self.savepoint_position(savepoint)?;
        self.savepoints.truncate(position);
        Ok(())
    }
}
