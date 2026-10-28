//! Selected exclusive native directory, independent of SQL or a coordinator.
mod device;
mod directory;
#[cfg(test)]
mod fault_tests;
mod files;
#[cfg(test)]
mod open_tests;
#[cfg(test)]
mod process_tests;
mod read_only;
#[cfg(test)]
mod read_only_tests;
#[cfg(test)]
mod test_support;
use super::{invalid, LsmStoreOptions, PhysicalLsmJournalStore};
use crate::engine::{
    cluster::{AtomicJournalStore, JournalVersion},
    maintenance::{JournalMaintenanceStore, JournalNamespaceEntry, JournalScanOptions},
    paged::CheckpointPagedLog,
};
use crate::{LinkError, ReplaceableTransitionLogStore, TransitionLogStore};
use device::Device;
use files::{Files, Namespace};
use std::{
    path::Path,
    sync::{Arc, Mutex},
};

/// Fixed mirror modes and finite resource bounds, checked before filesystem I/O.
#[derive(Clone, Copy, Debug)]
pub struct FileLsmOptions {
    pub store: LsmStoreOptions,
    pub max_pages: u64,
    pub mirrored_root: bool,
    pub mirrored_banks: bool,
    pub mirrored_tables: bool,
}
impl Default for FileLsmOptions {
    fn default() -> Self {
        Self {
            store: LsmStoreOptions::default(),
            max_pages: 16_384,
            mirrored_root: false,
            mirrored_banks: false,
            mirrored_tables: false,
        }
    }
}
type Store = PhysicalLsmJournalStore<CheckpointPagedLog<Device, Device>, Namespace>;
/// Owns `.lsm-writer` before bounded discovery and all child leases. Complete
/// read-only physical/semantic validation precedes constructor repair effects.
/// Close attempts all children before removing the outer fence. An uncertain
/// raw close is cached rather than retried. Never steals crashed ownership.
/// Windows supports file-content sync but advertises no durable namespace.
pub struct FileLsmJournalStore {
    inner: Option<Store>,
    files: Arc<Mutex<Files>>,
}
struct ReadOnlyWal(Vec<String>);
impl TransitionLogStore for ReadOnlyWal {
    fn supports_durable_flush(&self) -> bool {
        false
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.0.clone())
    }
    fn append_log_entry(&mut self, _: &str) -> Result<(), LinkError> {
        Err(invalid("read-only native LSM WAL"))
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        Err(invalid("read-only native LSM WAL"))
    }
}
impl ReplaceableTransitionLogStore for ReadOnlyWal {
    fn supports_replacement(&self) -> bool {
        true
    }
    fn replace_log_entries(&mut self, _: &[String]) -> Result<bool, LinkError> {
        Err(invalid("read-only native LSM WAL"))
    }
}
impl FileLsmJournalStore {
    pub fn open(directory: impl AsRef<Path>, options: FileLsmOptions) -> Result<Self, LinkError> {
        options.store.limits.validate()?;
        if !(1..=65_536).contains(&options.max_pages)
            || !(2..=65_536).contains(&options.store.max_namespace_entries)
        {
            return Err(invalid("native LSM admission bounds"));
        }
        let files = Arc::new(Mutex::new(Files::new(directory.as_ref(), options)?));
        let build = (|| {
            let records = {
                let mut files = files
                    .lock()
                    .map_err(|_| invalid("native LSM discovery mutex poisoned"))?;
                let names = files.list(options.store.max_namespace_entries)?;
                let (mut root, mut banks) = files.wal(true)?;
                let records =
                    CheckpointPagedLog::inspect(&mut root, &mut banks, options.max_pages)?;
                if records.is_empty() && !names.is_empty() {
                    return Err(invalid(
                        "uninitialized native LSM namespace contains tables",
                    ));
                }
                records
            };
            if !records.is_empty() {
                let mut validation = PhysicalLsmJournalStore::open(
                    ReadOnlyWal(records),
                    Namespace {
                        files: files.clone(),
                        read_only: true,
                    },
                    options.store,
                )?;
                validation.close()?;
            }
            let (root, banks) = {
                let mut files = files
                    .lock()
                    .map_err(|_| invalid("native LSM discovery mutex poisoned"))?;
                files.close_readers()?;
                files.wal(false)?
            };
            let wal = CheckpointPagedLog::with_max_pages(root, banks, options.max_pages)?;
            PhysicalLsmJournalStore::open(
                wal,
                Namespace {
                    files: files.clone(),
                    read_only: false,
                },
                options.store,
            )
        })();
        match build {
            Ok(inner) => Ok(Self {
                inner: Some(inner),
                files,
            }),
            Err(cause) => {
                let cleanup = files
                    .lock()
                    .map_err(|_| invalid("native LSM cleanup mutex poisoned"))?
                    .close(Vec::new());
                match cleanup {
                    Ok(()) => Err(cause),
                    Err(cleanup) => Err(invalid(&format!("{cause}; native cleanup: {cleanup}"))),
                }
            }
        }
    }
    fn store(&mut self) -> Result<&mut Store, LinkError> {
        self.inner
            .as_mut()
            .ok_or_else(|| invalid("closed native LSM store"))
    }
    pub fn flush(&mut self) -> Result<(), LinkError> {
        self.store()?.flush()
    }
    pub fn compact(&mut self) -> Result<(), LinkError> {
        self.store()?.compact()
    }
    /// Does not flush implicitly; successful CAS already acknowledges the WAL.
    pub fn close(&mut self) -> Result<(), LinkError> {
        let mut errors = Vec::new();
        if let Some(mut store) = self.inner.take() {
            if let Err(error) = store.close() {
                errors.push(error.to_string());
            }
        }
        self.files
            .lock()
            .map_err(|_| invalid("native LSM cleanup mutex poisoned"))?
            .close(errors)
    }
}
impl Drop for FileLsmJournalStore {
    fn drop(&mut self) {
        let _ = self.close();
    }
}
impl AtomicJournalStore for FileLsmJournalStore {
    fn supports_durable_flush(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(AtomicJournalStore::supports_durable_flush)
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        self.store()?.read(keys)
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        self.store()?.compare_exchange(keys, expected, updates)
    }
}
impl JournalMaintenanceStore for FileLsmJournalStore {
    fn scan_namespace(
        &mut self,
        options: JournalScanOptions,
    ) -> Result<Vec<JournalNamespaceEntry>, LinkError> {
        self.store()?.scan_namespace(options)
    }
    fn compare_delete(&mut self, key: &str, expected: u64) -> Result<bool, LinkError> {
        self.store()?.compare_delete(key, expected)
    }
}
