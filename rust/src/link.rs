//! Link - A doublet (source, target) pair with an index
//!
//! This module provides the core Link data structure that represents
//! a link in the doublet storage.

/// Link represents a doublet (source, target) pair with an index
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Link {
    pub index: u32,
    pub source: u32,
    pub target: u32,
}

impl Link {
    /// Creates a new link with the given index, source, and target
    pub fn new(index: u32, source: u32, target: u32) -> Self {
        Self {
            index,
            source,
            target,
        }
    }

    /// Returns true if this link is null (all zeros)
    pub fn is_null(&self) -> bool {
        self.index == 0 && self.source == 0 && self.target == 0
    }

    /// Returns true if this is a full point (self-referential link)
    pub fn is_full_point(&self) -> bool {
        self.index == self.source && self.source == self.target
    }

    /// Formats the link for display
    pub fn format(&self) -> String {
        format!("({} {} {})", self.index, self.source, self.target)
    }
}
