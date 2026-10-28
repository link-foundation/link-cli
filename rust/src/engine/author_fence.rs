//! Explicit author takeover. Every guarded data CAS includes the fence key.
use super::cluster::{validate_keys, AtomicJournalStore, JournalVersion};
use crate::LinkError;
use std::collections::BTreeSet;
#[derive(Clone, Debug)]
pub struct AuthorFenceOptions {
    pub max_entries: usize,
    pub max_bytes: usize,
    pub require_durable_flush: bool,
}
impl Default for AuthorFenceOptions {
    fn default() -> Self {
        Self {
            max_entries: 4096,
            max_bytes: 1_048_576,
            require_durable_flush: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorClaim {
    pub author: String,
    pub token: String,
    pub epoch: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AuthorFenceSnapshot {
    pub generation: u64,
    pub epoch: u64,
    pub claims: Vec<AuthorClaim>,
}
fn invalid() -> LinkError {
    LinkError::InvalidFormat("invalid author fence state or request".into())
}
fn fenced() -> LinkError {
    LinkError::Transaction("author owner is fenced by a later takeover".into())
}
fn identity(value: &str) -> Result<(), LinkError> {
    if value.len() != 32
        || value.bytes().all(|b| b == b'0')
        || value
            .bytes()
            .any(|b| !b.is_ascii_digit() && !(b'a'..=b'f').contains(&b))
    {
        return Err(invalid());
    }
    Ok(())
}
struct Loaded {
    expected: JournalVersion,
    claims: Vec<AuthorClaim>,
    values: Vec<JournalVersion>,
}
/// Caller-owned coherent provider; no expiry, election or background work.
/// Retain every claim; finite admission eventually refuses new takeovers.
pub struct AuthorFence<S> {
    store: S,
    key: String,
    author: String,
    header: String,
    options: AuthorFenceOptions,
}
impl<S: AtomicJournalStore> AuthorFence<S> {
    pub fn new(
        store: S,
        key: &str,
        author: &str,
        options: AuthorFenceOptions,
    ) -> Result<Self, LinkError> {
        identity(author)?;
        validate_keys(&[key.into()])?;
        if !(2..=4096).contains(&options.max_entries)
            || !(1..=1_048_576).contains(&options.max_bytes)
            || (options.require_durable_flush && !store.supports_durable_flush())
        {
            return Err(invalid());
        }
        Ok(Self {
            store,
            key: key.into(),
            author: author.into(),
            header: serde_json::to_string(&["author-fence1", author]).unwrap(),
            options,
        })
    }
    pub fn into_store(self) -> S {
        self.store
    }
    fn admit(&self, entries: &[String]) -> Result<(), LinkError> {
        if entries.len() > self.options.max_entries {
            return Err(invalid());
        }
        let mut bytes = 0;
        for entry in entries {
            if entry.len() > 256 {
                return Err(invalid());
            }
            bytes += entry.len();
            if bytes > self.options.max_bytes {
                return Err(invalid());
            }
        }
        Ok(())
    }
    fn read(&mut self, keys: &[String]) -> Result<Loaded, LinkError> {
        let mut selected = vec![self.key.clone()];
        selected.extend_from_slice(keys);
        let mut values = self.store.read(&selected)?;
        if values.len() != selected.len() {
            return Err(invalid());
        }
        let expected = values.remove(0);
        self.admit(&expected.entries)?;
        let mut claims = Vec::new();
        let mut tokens = BTreeSet::new();
        if expected.generation == 0 {
            if !expected.entries.is_empty() {
                return Err(invalid());
            }
        } else {
            if expected.entries.first() != Some(&self.header) {
                return Err(invalid());
            }
            for text in expected.entries.iter().skip(1) {
                let entry: Vec<String> = serde_json::from_str(text).map_err(|_| invalid())?;
                if entry.len() != 3
                    || entry[0] != "claim-author1"
                    || serde_json::to_string(&entry).unwrap() != *text
                {
                    return Err(invalid());
                }
                identity(&entry[1])?;
                let epoch = claims.len() as u64 + 1;
                if entry[2] != epoch.to_string() || !tokens.insert(entry[1].clone()) {
                    return Err(invalid());
                }
                claims.push(AuthorClaim {
                    author: self.author.clone(),
                    token: entry[1].clone(),
                    epoch,
                });
            }
        }
        Ok(Loaded {
            expected,
            claims,
            values,
        })
    }
    pub fn snapshot(&mut self) -> Result<AuthorFenceSnapshot, LinkError> {
        let state = self.read(&[])?;
        Ok(AuthorFenceSnapshot {
            generation: state.expected.generation,
            epoch: state.claims.len() as u64,
            claims: state.claims,
        })
    }
    /// One CAS; retry the original predecessor/token after uncertain I/O.
    /// Superseded retained tokens never retake ownership.
    pub fn claim(&mut self, expected_epoch: u64, token: &str) -> Result<AuthorClaim, LinkError> {
        identity(token)?;
        let state = self.read(&[])?;
        if let Some(retained) = state.claims.iter().find(|value| value.token == token) {
            if retained.epoch != state.claims.len() as u64
                || Some(retained.epoch) != expected_epoch.checked_add(1)
            {
                return Err(fenced());
            }
            return Ok(retained.clone());
        }
        if expected_epoch != state.claims.len() as u64 {
            return Err(LinkError::Transaction(
                "conflict: author epoch changed".into(),
            ));
        }
        if state.expected.generation == u64::MAX {
            return Err(invalid());
        }
        let result = AuthorClaim {
            author: self.author.clone(),
            token: token.into(),
            epoch: expected_epoch + 1,
        };
        let mut entries = state.expected.entries.clone();
        if entries.is_empty() {
            entries.push(self.header.clone());
        }
        entries.push(
            serde_json::to_string(&["claim-author1", token, &result.epoch.to_string()]).unwrap(),
        );
        self.admit(&entries)?;
        let committed = self
            .store
            .compare_exchange(
                std::slice::from_ref(&self.key),
                &[state.expected],
                &[Some(entries)],
            )
            .map_err(|cause| {
                LinkError::StorageError(format!(
                    "indeterminate author takeover; retry original predecessor/token: {cause}"
                ))
            })?;
        if !committed {
            return Err(LinkError::Transaction(
                "conflict: author epoch changed".into(),
            ));
        }
        Ok(result)
    }
    pub fn bind(self, claim: AuthorClaim) -> Result<GuardedJournalStore<S>, LinkError> {
        identity(&claim.token)?;
        if claim.author != self.author || claim.epoch == 0 {
            return Err(invalid());
        }
        Ok(GuardedJournalStore { owner: self, claim })
    }
}
/// All writers to selected data keys must use this guard. One reserved fence
/// key reduces the maximum data group to 31. Reading may linearize immediately
/// before takeover; publication always compares ownership atomically.
pub struct GuardedJournalStore<S> {
    owner: AuthorFence<S>,
    claim: AuthorClaim,
}
impl<S: AtomicJournalStore> GuardedJournalStore<S> {
    pub fn into_store(self) -> S {
        self.owner.store
    }
    fn keys(&self, keys: &[String]) -> Result<(), LinkError> {
        validate_keys(keys)?;
        if keys.len() > 31 || keys.contains(&self.owner.key) {
            return Err(invalid());
        }
        Ok(())
    }
    fn current(&mut self, keys: &[String]) -> Result<Loaded, LinkError> {
        let state = self.owner.read(keys)?;
        if state.claims.last() != Some(&self.claim) {
            return Err(fenced());
        }
        Ok(state)
    }
}
impl<S: AtomicJournalStore> AtomicJournalStore for GuardedJournalStore<S> {
    fn supports_durable_flush(&self) -> bool {
        self.owner.store.supports_durable_flush()
    }
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        self.keys(keys)?;
        Ok(self.current(keys)?.values)
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        self.keys(keys)?;
        if expected.len() != keys.len() || updates.len() != keys.len() {
            return Err(invalid());
        }
        let state = self.current(&[])?;
        let mut selected = vec![self.owner.key.clone()];
        selected.extend_from_slice(keys);
        let mut versions = vec![state.expected];
        versions.extend_from_slice(expected);
        let mut writes = vec![None];
        writes.extend_from_slice(updates);
        self.owner
            .store
            .compare_exchange(&selected, &versions, &writes)
    }
}
