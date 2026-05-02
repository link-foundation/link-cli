use crate::link::Link;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Pattern {
    pub(crate) index: String,
    pub(crate) source: Option<Box<Pattern>>,
    pub(crate) target: Option<Box<Pattern>>,
}

impl Pattern {
    pub(crate) fn new(index: String, source: Option<Pattern>, target: Option<Pattern>) -> Self {
        Self {
            index,
            source: source.map(Box::new),
            target: target.map(Box::new),
        }
    }

    pub(crate) fn is_leaf(&self) -> bool {
        self.source.is_none() && self.target.is_none()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ResolvedLink {
    pub(crate) index: u32,
    pub(crate) source: u32,
    pub(crate) target: u32,
    pub(crate) name: Option<String>,
}

impl ResolvedLink {
    pub(crate) fn new(index: u32, source: u32, target: u32, name: Option<String>) -> Self {
        Self {
            index,
            source,
            target,
            name,
        }
    }

    pub(crate) fn to_link(&self) -> Link {
        Link::new(self.index, self.source, self.target)
    }
}
