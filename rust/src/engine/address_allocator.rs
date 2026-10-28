//! Selected permanent ranges over one linearizable atomic journal key.
//! A grant is never recycled; author lineage fencing remains caller-owned.
use super::cluster::{AtomicJournalStore, JournalVersion};
use crate::LinkError;
use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct AddressAllocatorOptions {
    pub max_entries: usize,
    pub max_bytes: usize,
    pub require_durable_flush: bool,
}
impl Default for AddressAllocatorOptions {
    fn default() -> Self {
        Self {
            max_entries: 4096,
            max_bytes: 1_048_576,
            require_durable_flush: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddressReservation {
    pub author: String,
    pub token: String,
    pub count: u64,
    pub first: u64,
    pub last: u64,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddressAllocationSnapshot {
    pub generation: u64,
    /// None means the immutable domain is exhausted.
    pub next: Option<u64>,
    pub reservations: Vec<AddressReservation>,
}
struct Loaded {
    expected: JournalVersion,
    next: u128,
    reservations: Vec<AddressReservation>,
}
fn invalid() -> LinkError {
    LinkError::InvalidFormat("invalid address reservation state or request".into())
}
fn identity(value: &str) -> Result<(), LinkError> {
    if value.len() != 32
        || value
            .bytes()
            .any(|b| !b.is_ascii_digit() && !(b'a'..=b'f').contains(&b))
        || value.bytes().all(|b| b == b'0')
    {
        return Err(invalid());
    }
    Ok(())
}
fn number(value: &str) -> Result<u64, LinkError> {
    if value.is_empty()
        || value.len() > 20
        || !value.bytes().all(|b| b.is_ascii_digit())
        || (value.len() > 1 && value.starts_with('0'))
    {
        return Err(invalid());
    }
    value.parse().map_err(|_| invalid())
}

/// O(retained journal) per new grant. Every fresh writer in the domain must use
/// the same coordinator key; initialize above all previously issued identities.
/// No background work, receipt deletion, blind retry or provider disposal.
pub struct AddressAllocator<S> {
    store: S,
    key: String,
    first: u64,
    last: u64,
    header: String,
    options: AddressAllocatorOptions,
}
impl<S: AtomicJournalStore> AddressAllocator<S> {
    pub fn new(
        store: S,
        key: &str,
        first: u64,
        last: u64,
        options: AddressAllocatorOptions,
    ) -> Result<Self, LinkError> {
        if key.is_empty()
            || key.len() > 128
            || key
                .bytes()
                .any(|b| !b.is_ascii_alphanumeric() && !b"_.-".contains(&b))
            || first == 0
            || first > last
            || !(2..=4096).contains(&options.max_entries)
            || !(1..=1_048_576).contains(&options.max_bytes)
            || (options.require_durable_flush && !store.supports_durable_flush())
        {
            return Err(invalid());
        }
        Ok(Self {
            store,
            key: key.into(),
            first,
            last,
            header: serde_json::to_string(&[
                "address-reservations1",
                &first.to_string(),
                &last.to_string(),
            ])
            .unwrap(),
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
    fn load(&mut self) -> Result<Loaded, LinkError> {
        let mut values = self.store.read(std::slice::from_ref(&self.key))?;
        if values.len() != 1 {
            return Err(invalid());
        }
        let expected = values.remove(0);
        self.admit(&expected.entries)?;
        let mut next = u128::from(self.first);
        let mut reservations = Vec::new();
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
                if entry.len() != 6
                    || entry[0] != "reserve-addresses1"
                    || serde_json::to_string(&entry).unwrap() != *text
                {
                    return Err(invalid());
                }
                identity(&entry[1])?;
                identity(&entry[2])?;
                let count = number(&entry[3])?;
                let first = number(&entry[4])?;
                let last = number(&entry[5])?;
                if count == 0
                    || u128::from(first) != next
                    || last > self.last
                    || last < first
                    || last - first != count - 1
                    || !tokens.insert(entry[2].clone())
                {
                    return Err(invalid());
                }
                reservations.push(AddressReservation {
                    author: entry[1].clone(),
                    token: entry[2].clone(),
                    count,
                    first,
                    last,
                });
                next = u128::from(last) + 1;
            }
        }
        Ok(Loaded {
            expected,
            next,
            reservations,
        })
    }
    pub fn snapshot(&mut self) -> Result<AddressAllocationSnapshot, LinkError> {
        let state = self.load()?;
        Ok(AddressAllocationSnapshot {
            generation: state.expected.generation,
            next: (state.next <= u128::from(self.last)).then_some(state.next as u64),
            reservations: state.reservations,
        })
    }
    /// Retry the original author/token/count after conflict or uncertain I/O.
    /// Retained matching requests publish nothing, even after later grants.
    pub fn reserve(
        &mut self,
        author: &str,
        token: &str,
        count: u64,
    ) -> Result<AddressReservation, LinkError> {
        identity(author)?;
        identity(token)?;
        if count == 0 {
            return Err(invalid());
        }
        let state = self.load()?;
        if let Some(retained) = state.reservations.iter().find(|v| v.token == token) {
            return if retained.author == author && retained.count == count {
                Ok(retained.clone())
            } else {
                Err(invalid())
            };
        }
        let end = state.next + u128::from(count) - 1;
        if end > u128::from(self.last) || state.expected.generation == u64::MAX {
            return Err(invalid());
        }
        let result = AddressReservation {
            author: author.into(),
            token: token.into(),
            count,
            first: state.next as u64,
            last: end as u64,
        };
        let mut entries = state.expected.entries.clone();
        if entries.is_empty() {
            entries.push(self.header.clone());
        }
        entries.push(
            serde_json::to_string(&[
                "reserve-addresses1",
                author,
                token,
                &count.to_string(),
                &result.first.to_string(),
                &result.last.to_string(),
            ])
            .unwrap(),
        );
        self.admit(&entries)?;
        let committed = self.store.compare_exchange(std::slice::from_ref(&self.key), &[state.expected], &[Some(entries)])
            .map_err(|cause| LinkError::StorageError(format!("indeterminate address reservation; retry the original author/token/count: {cause}")))?;
        if !committed {
            return Err(LinkError::Transaction(
                "conflict: address reservations changed; retry the original request".into(),
            ));
        }
        Ok(result)
    }
}
