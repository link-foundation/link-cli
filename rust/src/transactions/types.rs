//! Value types and serialization helpers for the transactions layer.

use std::path::PathBuf;

use anyhow::{anyhow, bail, Result};

use crate::link::Link;

/// The kind of write operation recorded by a [`Transition`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TransitionKind {
    Create,
    Update,
    Delete,
}

impl TransitionKind {
    pub(crate) fn as_u8(self) -> u8 {
        match self {
            TransitionKind::Create => 0,
            TransitionKind::Update => 1,
            TransitionKind::Delete => 2,
        }
    }

    pub(crate) fn from_u8(value: u8) -> Option<Self> {
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
/// C# `Platform.Data.Doublets.Link<uint>`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct DoubletLink {
    pub index: u32,
    pub source: u32,
    pub target: u32,
}

impl DoubletLink {
    pub const fn new(index: u32, source: u32, target: u32) -> Self {
        Self {
            index,
            source,
            target,
        }
    }

    pub const fn empty() -> Self {
        Self::new(0, 0, 0)
    }

    pub fn from_link(link: &Link) -> Self {
        Self::new(link.index, link.source, link.target)
    }
}

/// Reversible write captured by the transactions layer. Holds both
/// `before` and `after` link states so the operation can be undone or
/// replayed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Transition {
    pub transaction_id: u128,
    pub sequence: i64,
    pub timestamp_ms: i64,
    pub kind: TransitionKind,
    pub before: DoubletLink,
    pub after: DoubletLink,
}

impl Transition {
    pub(crate) const SCHEMA_VERSION: &'static str = "v1";

    /// Encodes the transition as a single line stored as the *name*
    /// of one link in the log doublets store.
    pub fn serialize(&self) -> String {
        format!(
            "{schema}|{tx:032x}|{seq}|{ms}|{kind}|{bi},{bs},{bt}|{ai},{as_},{at}",
            schema = Self::SCHEMA_VERSION,
            tx = self.transaction_id,
            seq = self.sequence,
            ms = self.timestamp_ms,
            kind = self.kind.as_u8(),
            bi = self.before.index,
            bs = self.before.source,
            bt = self.before.target,
            ai = self.after.index,
            as_ = self.after.source,
            at = self.after.target,
        )
    }

    /// Parses a serialized transition.
    pub fn try_parse(text: &str) -> Option<Self> {
        if text.is_empty() {
            return None;
        }
        let parts: Vec<&str> = text.split('|').collect();
        if parts.len() < 7 {
            return None;
        }
        if parts[0] != Self::SCHEMA_VERSION {
            return None;
        }
        let tx = u128::from_str_radix(parts[1], 16).ok()?;
        let seq: i64 = parts[2].parse().ok()?;
        let ms: i64 = parts[3].parse().ok()?;
        let kind_value: u8 = parts[4].parse().ok()?;
        let kind = TransitionKind::from_u8(kind_value)?;
        let before = parse_doublet(parts[5])?;
        let after = parse_doublet(parts[6])?;
        Some(Self {
            transaction_id: tx,
            sequence: seq,
            timestamp_ms: ms,
            kind,
            before,
            after,
        })
    }
}

fn parse_doublet(text: &str) -> Option<DoubletLink> {
    let parts: Vec<&str> = text.split(',').collect();
    if parts.len() != 3 {
        return None;
    }
    Some(DoubletLink {
        index: parts[0].parse().ok()?,
        source: parts[1].parse().ok()?,
        target: parts[2].parse().ok()?,
    })
}

/// Sidecar-store name prefixes used by the recovery protocol.
pub(crate) const COMMIT_MARKER_PREFIX: &str = "__transactions:commit:";
pub(crate) const ROLLBACK_MARKER_PREFIX: &str = "__transactions:rollback:";
pub(crate) const APPLIED_MARKER_PREFIX: &str = "__transactions:applied:";
pub(crate) const TRANSITION_NAME_PREFIX: &str = "__transactions:transition:";
