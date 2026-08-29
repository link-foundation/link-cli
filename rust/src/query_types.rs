//! The intermediate shapes a query passes through: the [`Pattern`] a
//! restriction or substitution parses into, and the [`ResolvedLink`] a pattern
//! becomes once its variables are bound.

use crate::link::Link;

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
pub struct ResolvedLink {
    pub index: u32,
    pub source: u32,
    pub target: u32,
    pub name: Option<String>,
}

impl ResolvedLink {
    pub fn new(index: u32, source: u32, target: u32, name: Option<String>) -> Self {
        Self {
            index,
            source,
            target,
            name,
        }
    }

    pub fn to_link(&self) -> Link {
        Link::new(self.index, self.source, self.target)
    }
}
