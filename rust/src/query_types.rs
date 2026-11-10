//! The intermediate shapes a query passes through: the [`Pattern`] a
//! restriction or substitution parses into, and the [`ResolvedLink`] a pattern
//! becomes once its variables are bound.

use crate::link::GenericLink;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Pattern {
    pub index: String,
    pub source: Option<Box<Pattern>>,
    pub target: Option<Box<Pattern>>,
}

impl Pattern {
    pub fn new(index: String, source: Option<Pattern>, target: Option<Pattern>) -> Self {
        Self {
            index,
            source: source.map(Box::new),
            target: target.map(Box::new),
        }
    }

    pub fn is_leaf(&self) -> bool {
        self.source.is_none() && self.target.is_none()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GenericResolvedLink<T> {
    pub index: T,
    pub source: T,
    pub target: T,
    pub name: Option<String>,
}

impl<T: Copy> GenericResolvedLink<T> {
    pub fn new(index: T, source: T, target: T, name: Option<String>) -> Self {
        Self {
            index,
            source,
            target,
            name,
        }
    }

    pub fn to_link(&self) -> GenericLink<T> {
        GenericLink::new(self.index, self.source, self.target)
    }
}

/// The existing CLI specialization.
pub type ResolvedLink = GenericResolvedLink<u32>;
