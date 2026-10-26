//! Optional bounded namespace maintenance; ordinary atomic stores need not implement it.
use super::cluster::{AtomicJournalStore, JournalVersion};
use crate::LinkError;

#[derive(Clone, Copy, Debug)]
pub struct JournalScanOptions {
    pub max_keys: usize,
    /// Decoded UTF-8 key bytes, eight generation bytes, and record bytes plus
    /// one separator per record. Container overhead also depends on record count.
    pub max_bytes: usize,
    pub page_keys: usize,
    /// Omit record payloads; returned versions contain empty entries.
    pub keys_only: bool,
}
impl Default for JournalScanOptions {
    fn default() -> Self {
        Self {
            max_keys: 4096,
            max_bytes: 8_388_608,
            page_keys: 128,
            keys_only: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JournalNamespaceEntry {
    pub key: String,
    pub version: JournalVersion,
}
/// Scans return the COMPLETE namespace at one linearizable point or an error,
/// never partial success. Every generation must be nonzero and never reused,
/// including after deletion. A delete error has an unknown outcome: do not retry
/// blindly. This capability supplies no object-liveness or reclamation policy.
pub trait JournalMaintenanceStore: AtomicJournalStore {
    fn scan_namespace(
        &mut self,
        options: JournalScanOptions,
    ) -> Result<Vec<JournalNamespaceEntry>, LinkError>;
    fn compare_delete(&mut self, key: &str, expected_generation: u64) -> Result<bool, LinkError>;
}
