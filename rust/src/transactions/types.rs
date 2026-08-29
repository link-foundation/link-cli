//! Value types and serialization helpers for the transactions layer.
//!
//! Every type here is generic over the doublets address type `T` so the
//! transactions layer can be reused with `usize`- or `u64`-addressed
//! stores. The `u32` specialisations ([`DoubletLink`], [`Transition`])
//! are what the `clink` CLI itself uses.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};
use doublets::data::LinkReference;

use crate::error::LinkError;
use crate::link::GenericLink;

/// The kind of write operation recorded by a [`Transition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransitionKind {
    Create,
    Update,
    Delete,
}

impl TransitionKind {
    pub fn as_u8(self) -> u8 {
        match self {
            TransitionKind::Create => 0,
            TransitionKind::Update => 1,
            TransitionKind::Delete => 2,
        }
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(TransitionKind::Create),
            1 => Some(TransitionKind::Update),
            2 => Some(TransitionKind::Delete),
            _ => None,
        }
    }
}

/// Sync flushes data-store side-effects before `commit` returns.
///
/// Async durably persists the transitions then applies the data-store
/// side-effects on a background-friendly path (already-applied
/// side-effects are the common case for in-process inner stores).
///
/// The Rust port runs both modes synchronously on the calling thread
/// for predictability; the distinction is preserved for parity with C#
/// and for future expansion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CommitMode {
    #[default]
    Sync,
    Async,
}

/// Retention policy for the transitions log.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LogRetentionPolicy {
    /// Keep every transition forever (default).
    #[default]
    Infinite,
    /// Drop the oldest applied transitions once the live log exceeds
    /// `max_transitions`. Never drops un-applied transitions (R7).
    Sized { max_transitions: u64 },
    /// Archive the oldest `chunk_size` applied transitions to a
    /// rolling file in `archive_directory` once the live log reaches
    /// `chunk_size`.
    Chunked {
        chunk_size: u64,
        archive_directory: PathBuf,
    },
}

impl LogRetentionPolicy {
    /// Parses a CLI spec: `infinite`, `sized:<n>`, `chunked:<n>:<dir>`.
    pub fn parse(spec: &str) -> Result<Self> {
        let trimmed = spec.trim();
        if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("infinite") {
            return Ok(Self::Infinite);
        }

        let lowered = trimmed.to_ascii_lowercase();
        if lowered.starts_with("sized:") {
            let rest = &trimmed["sized:".len()..];
            let max: u64 = rest
                .parse()
                .map_err(|_| anyhow!("invalid sized retention spec '{spec}'"))?;
            return Ok(Self::Sized {
                max_transitions: max,
            });
        }
        if lowered.starts_with("chunked:") {
            let rest = &trimmed["chunked:".len()..];
            let (size_text, dir) = rest
                .split_once(':')
                .ok_or_else(|| anyhow!("invalid chunked retention spec '{spec}'"))?;
            let chunk_size: u64 = size_text
                .parse()
                .map_err(|_| anyhow!("invalid chunked size in '{spec}'"))?;
            if chunk_size == 0 {
                bail!("invalid chunked size in '{spec}'");
            }
            if dir.is_empty() {
                bail!("invalid chunked retention spec '{spec}'");
            }
            return Ok(Self::Chunked {
                chunk_size,
                archive_directory: PathBuf::from(dir),
            });
        }
        bail!("unknown retention spec '{spec}'");
    }
}

/// A single doublet link state captured by a transition (mirror of the
/// C# `Platform.Data.Doublets.Link<TLinkAddress>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct GenericDoubletLink<T> {
    pub index: T,
    pub source: T,
    pub target: T,
}

impl<T> GenericDoubletLink<T> {
    pub const fn new(index: T, source: T, target: T) -> Self {
        Self {
            index,
            source,
            target,
        }
    }
}

impl<T: LinkReference> GenericDoubletLink<T> {
    /// The all-zero doublet used for the missing side of a create/delete.
    pub fn empty() -> Self {
        let zero = T::from_byte(0);
        Self::new(zero, zero, zero)
    }

    pub fn from_link(link: &GenericLink<T>) -> Self {
        Self::new(link.index, link.source, link.target)
    }

    fn serialize(&self) -> String {
        format!("{},{},{}", self.index, self.source, self.target)
    }

    fn parse(text: &str) -> Result<Self, LinkError> {
        let parts: Vec<&str> = text.split(',').collect();
        if parts.len() != 3 {
            return Err(LinkError::InvalidFormat(format!(
                "expected 'index,source,target' in transition, got '{text}'"
            )));
        }
        Ok(Self::new(
            parse_address(parts[0])?,
            parse_address(parts[1])?,
            parse_address(parts[2])?,
        ))
    }
}

impl<T: LinkReference> From<GenericLink<T>> for GenericDoubletLink<T> {
    fn from(link: GenericLink<T>) -> Self {
        Self::new(link.index, link.source, link.target)
    }
}

impl<T: LinkReference> From<GenericDoubletLink<T>> for GenericLink<T> {
    fn from(link: GenericDoubletLink<T>) -> Self {
        Self::new(link.index, link.source, link.target)
    }
}

/// The `u32`-addressed doublet used by the `clink` CLI.
pub type DoubletLink = GenericDoubletLink<u32>;

/// Reversible write captured by the transactions layer. Holds both
/// `before` and `after` link states so the operation can be undone or
/// replayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GenericTransition<T> {
    pub transaction_id: u128,
    pub sequence: i64,
    pub timestamp_ms: i64,
    pub kind: TransitionKind,
    pub before: GenericDoubletLink<T>,
    pub after: GenericDoubletLink<T>,
}

/// The `u32`-addressed transition used by the `clink` CLI.
pub type Transition = GenericTransition<u32>;

impl<T: LinkReference> GenericTransition<T> {
    pub const SCHEMA_VERSION: &'static str = "v1";

    /// Encodes the transition as a single line stored as one entry of
    /// the transitions log.
    ///
    /// Addresses are written in decimal, so a log written by a
    /// `u32`-addressed store reads back unchanged in a `u64`- or
    /// `usize`-addressed one.
    pub fn serialize(&self) -> String {
        format!(
            "{schema}|{tx:032x}|{seq}|{ms}|{kind}|{before}|{after}",
            schema = Self::SCHEMA_VERSION,
            tx = self.transaction_id,
            seq = self.sequence,
            ms = self.timestamp_ms,
            kind = self.kind.as_u8(),
            before = self.before.serialize(),
            after = self.after.serialize(),
        )
    }

    /// Parses a serialized transition.
    ///
    /// Returns [`LinkError::InvalidFormat`] for anything that is not a
    /// well-formed entry — including the partial line a crash can leave
    /// at the end of an append-only log — and
    /// [`LinkError::AddressOutOfRange`] for a structurally valid entry
    /// whose addresses do not fit into `T`. The two are distinct on
    /// purpose: recovery skips torn entries but must not silently drop
    /// a log written by a wider address type.
    pub fn parse(text: &str) -> Result<Self, LinkError> {
        let invalid = || LinkError::InvalidFormat(format!("malformed transition entry '{text}'"));
        if text.is_empty() {
            return Err(invalid());
        }
        let parts: Vec<&str> = text.split('|').collect();
        if parts.len() < 7 || parts[0] != Self::SCHEMA_VERSION {
            return Err(invalid());
        }
        let transaction_id = u128::from_str_radix(parts[1], 16).map_err(|_| invalid())?;
        let sequence: i64 = parts[2].parse().map_err(|_| invalid())?;
        let timestamp_ms: i64 = parts[3].parse().map_err(|_| invalid())?;
        let kind_value: u8 = parts[4].parse().map_err(|_| invalid())?;
        let kind = TransitionKind::from_u8(kind_value).ok_or_else(invalid)?;
        let before =
            GenericDoubletLink::parse(parts[5]).map_err(|error| keep_range(error, &invalid))?;
        let after =
            GenericDoubletLink::parse(parts[6]).map_err(|error| keep_range(error, &invalid))?;
        Ok(Self {
            transaction_id,
            sequence,
            timestamp_ms,
            kind,
            before,
            after,
        })
    }

    /// Lenient variant of [`GenericTransition::parse`].
    pub fn try_parse(text: &str) -> Option<Self> {
        Self::parse(text).ok()
    }
}

/// Keeps [`LinkError::AddressOutOfRange`] distinguishable while turning
/// every other doublet parse failure into the caller's format error.
fn keep_range(error: LinkError, invalid: &dyn Fn() -> LinkError) -> LinkError {
    match error {
        LinkError::AddressOutOfRange(value) => LinkError::AddressOutOfRange(value),
        _ => invalid(),
    }
}

/// Parses a decimal link address into any `doublets` address type.
fn parse_address<T: LinkReference>(text: &str) -> Result<T, LinkError> {
    let value: u128 = text
        .parse()
        .map_err(|_| LinkError::InvalidFormat(format!("invalid link address '{text}'")))?;
    T::try_from(value).map_err(|_| LinkError::AddressOutOfRange(value))
}

/// Sidecar-store name prefixes used by the recovery protocol.
pub const COMMIT_MARKER_PREFIX: &str = "__transactions:commit:";
pub const ROLLBACK_MARKER_PREFIX: &str = "__transactions:rollback:";
pub const APPLIED_MARKER_PREFIX: &str = "__transactions:applied:";
pub const TRANSITION_NAME_PREFIX: &str = "__transactions:transition:";
