//! Link - A doublet (source, target) pair with an index
//!
//! This module provides the core link data structure that represents
//! a link in the doublet storage.
//!
//! The structure is generic over the link *address* type ([`GenericLink`])
//! so that the storage and transactions layers can be reused with any
//! address width supported by `doublets` (`u32`, `u64`, `usize`, ...).
//! [`Link`] is the `u32` specialisation used by the `clink` CLI itself.

use doublets::data::LinkReference;

/// A doublet `(source, target)` pair together with its own address.
///
/// Generic over the address type `T` so external consumers can use the
/// same storage/transaction stack with `usize`-addressed doublets stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct GenericLink<T> {
    pub index: T,
    pub source: T,
    pub target: T,
}

impl<T> GenericLink<T> {
    /// Creates a new link with the given index, source, and target
    pub const fn new(index: T, source: T, target: T) -> Self {
        Self {
            index,
            source,
            target,
        }
    }
}

impl<T: LinkReference> GenericLink<T> {
    /// The null link (all addresses zero).
    pub fn null() -> Self {
        let zero = T::from_byte(0);
        Self::new(zero, zero, zero)
    }

    /// Returns true if this link is null (all zeros)
    pub fn is_null(&self) -> bool {
        let zero = T::from_byte(0);
        self.index == zero && self.source == zero && self.target == zero
    }

    /// Returns true if this is a full point (self-referential link)
    pub fn is_full_point(&self) -> bool {
        self.index == self.source && self.source == self.target
    }

    /// Returns true if this link references itself from at least one side.
    pub fn is_partial_point(&self) -> bool {
        self.index == self.source || self.index == self.target
    }

    /// Formats the link for display
    pub fn format(&self) -> String {
        format!("({} {} {})", self.index, self.source, self.target)
    }
}

/// The `u32`-addressed link used by the `clink` CLI and its decorators.
pub type Link = GenericLink<u32>;

/// Link type from the upstream `doublets` crate used as the Rust basis.
pub type DoubletsLink = doublets::Link<u32>;

impl<T: LinkReference> From<doublets::Link<T>> for GenericLink<T> {
    fn from(link: doublets::Link<T>) -> Self {
        Self::new(link.index, link.source, link.target)
    }
}

impl<T: LinkReference> From<GenericLink<T>> for doublets::Link<T> {
    fn from(link: GenericLink<T>) -> Self {
        Self::new(link.index, link.source, link.target)
    }
}
