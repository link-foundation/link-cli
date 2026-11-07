//! An optional worker that compacts a journal while commits continue.
use super::{error, Engine, LinkReference};
use crate::{LinkError, ReplaceableTransitionLogStore};
use std::{
    sync::{Arc, Condvar, Mutex, PoisonError},
    thread::JoinHandle,
    time::Duration,
};

/// Calls `compact_journal_after` every `period` on its own thread. Each pass that
/// compacts holds the commit lock while the journal is replaced, so commits wait
/// for it. The first failure ends the worker and `stop` returns it; a failed
/// replacement also fences the engine until reopen, as with a manual compaction.
/// Dropping the handle stops the worker and discards any failure.
pub struct BackgroundCompaction {
    stop: Arc<(Mutex<bool>, Condvar)>,
    worker: Option<JoinHandle<Result<(), LinkError>>>,
}

impl BackgroundCompaction {
    /// Refuses a zero `commits` or `period` and a log that cannot be replaced
    /// before starting a thread.
    pub fn start<T, L>(
        engine: Engine<T, L>,
        commits: u64,
        period: Duration,
    ) -> Result<Self, LinkError>
    where
        T: LinkReference + Send + 'static,
        L: ReplaceableTransitionLogStore + Send + 'static,
    {
        if commits == 0 || period.is_zero() {
            return Err(error(
                "compaction needs a commit count and a period above zero",
            ));
        }
        // Nothing is due yet at this distance; this only checks the selection.
        engine.compact_journal_after(u64::MAX)?;
        let stop = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = stop.clone();
        let worker = std::thread::spawn(move || {
            let (stopped, wake) = &*signal;
            let mut guard = stopped.lock().unwrap_or_else(PoisonError::into_inner);
            loop {
                guard = wake
                    .wait_timeout_while(guard, period, |stopped| !*stopped)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0;
                if *guard {
                    return Ok(());
                }
                engine.compact_journal_after(commits)?;
            }
        });
        Ok(Self {
            stop,
            worker: Some(worker),
        })
    }

    fn join(&mut self) -> Result<(), LinkError> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        *self.stop.0.lock().unwrap_or_else(PoisonError::into_inner) = true;
        self.stop.1.notify_all();
        worker
            .join()
            .unwrap_or_else(|_| Err(error("the compaction worker panicked")))
    }

    /// Waits for a running pass, then returns the failure that ended the worker.
    pub fn stop(mut self) -> Result<(), LinkError> {
        self.join()
    }
}

impl Drop for BackgroundCompaction {
    fn drop(&mut self) {
        let _ = self.join();
    }
}
