//! Optional serializable point-read validation; scans conservatively cover all rows.
use std::collections::BTreeSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SerializableValidation {
    /// Abort after any intervening database revision; no per-read bookkeeping.
    #[default]
    Revision,
    /// Validate point dependencies and writes; any scan still checks the revision.
    ReadSet,
}

pub(super) struct ReadSet<T> {
    pub keys: BTreeSet<T>,
    pub scanned: bool,
}
impl<T> Default for ReadSet<T> {
    fn default() -> Self {
        Self {
            keys: BTreeSet::new(),
            scanned: false,
        }
    }
}
