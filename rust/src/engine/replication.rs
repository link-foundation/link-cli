//! Fixed-primary log synchronization. Replica acknowledgements are copies,
//! not consensus: no elections, automatic failover, or cross-shard atomicity.
use super::error;
use crate::{LinkError, TransitionLogStore};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Synchronization {
    /// Caller schedules synchronize(); the primary log is the persistent backlog.
    Asynchronous,
    /// Number of acknowledgements including the primary, from fixed membership.
    Synchronous { acknowledgements: usize },
}

pub struct ReplicatedLog<P, R> {
    primary: P,
    replicas: Vec<R>,
    policy: Synchronization,
    trace: bool,
}
impl<P: TransitionLogStore, R: TransitionLogStore> ReplicatedLog<P, R> {
    pub fn new(primary: P, replicas: Vec<R>, policy: Synchronization) -> Result<Self, LinkError> {
        if let Synchronization::Synchronous { acknowledgements } = policy {
            if acknowledgements == 0 || acknowledgements > replicas.len() + 1 {
                return Err(error("acknowledgements must fit the fixed membership"));
            }
        }
        Ok(Self {
            primary,
            replicas,
            policy,
            trace: false,
        })
    }
    pub fn set_trace(&mut self, enabled: bool) {
        self.trace = enabled;
    }
    pub fn into_parts(self) -> (P, Vec<R>) {
        (self.primary, self.replicas)
    }
    /// Catch up each replica, checking that its complete log is an exact prefix.
    /// Retries never append duplicates. Divergent replicas require operator repair.
    pub fn synchronize(&mut self) -> Result<usize, LinkError> {
        self.primary.flush_log()?;
        self.synchronize_flushed()
    }
    // flush_log already established this boundary; explicit catch-up must do
    // it itself. Neither path acknowledges an unflushed primary prefix.
    fn synchronize_flushed(&mut self) -> Result<usize, LinkError> {
        let entries = self.primary.read_log_entries()?;
        let mut acknowledgements = 1;
        for (index, replica) in self.replicas.iter_mut().enumerate() {
            let result = (|| {
                let previous = replica.read_log_entries()?;
                if previous.len() > entries.len() || !entries.starts_with(&previous) {
                    return Err(error("replica log diverges from primary"));
                }
                for entry in &entries[previous.len()..] {
                    replica.append_log_entry(entry)?;
                }
                replica.flush_log()
            })();
            match result {
                Ok(()) => acknowledgements += 1,
                Err(cause) => {
                    if self.trace {
                        crate::diagnostics::stderr!(
                            "[Replication] replica={index} unavailable: {cause}"
                        );
                    }
                }
            }
        }
        if self.trace {
            crate::diagnostics::stderr!(
                "[Replication] acknowledgements={acknowledgements}/{}",
                self.replicas.len() + 1
            );
        }
        Ok(acknowledgements)
    }
}
impl<P: TransitionLogStore, R: TransitionLogStore> TransitionLogStore for ReplicatedLog<P, R> {
    fn supports_durable_flush(&self) -> bool {
        self.primary.supports_durable_flush()
            && match self.policy {
                Synchronization::Asynchronous => true,
                Synchronization::Synchronous { acknowledgements } => {
                    acknowledgements == 1
                        || self
                            .replicas
                            .iter()
                            .all(TransitionLogStore::supports_durable_flush)
                }
            }
    }
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        self.primary.append_log_entry(entry)
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.primary.read_log_entries()
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.primary.flush_log()?;
        if let Synchronization::Synchronous { acknowledgements } = self.policy {
            let actual = self.synchronize_flushed()?;
            if actual < acknowledgements {
                return Err(error(&format!("indeterminate replication outcome: {actual}/{acknowledgements} acknowledgements")));
            }
        }
        Ok(())
    }
}
