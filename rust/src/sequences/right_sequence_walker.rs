use anyhow::{bail, Result};
use std::collections::HashSet;

use crate::link_storage::LinkStorage;
use crate::sequences::{DefaultStack, TargetMatcher};

#[derive(Clone, Copy, Debug)]
pub struct RightSequenceWalker {
    unicode_symbol_criterion_matcher: TargetMatcher,
}

impl RightSequenceWalker {
    pub fn new(unicode_symbol_criterion_matcher: TargetMatcher) -> Self {
        Self {
            unicode_symbol_criterion_matcher,
        }
    }

    /// Compatibility wrapper for valid graphs. Use [`Self::try_walk`] when
    /// decoding persisted or caller-controlled sequence links.
    ///
    /// # Panics
    /// Panics if the graph contains a missing node or a cycle.
    pub fn walk(&self, links: &LinkStorage, sequence: u32) -> Vec<u32> {
        self.try_walk(links, sequence)
            .expect("Invalid sequence graph; use try_walk for fallible decoding")
    }

    /// Walk left-to-right, rejecting missing nodes and active-path cycles.
    /// Shared subtrees are expanded for each occurrence. Traversal uses an
    /// explicit stack and O(depth) path metadata rather than recursive calls.
    pub fn try_walk(&self, links: &LinkStorage, sequence: u32) -> Result<Vec<u32>> {
        let mut output = Vec::new();
        let mut stack = DefaultStack::new();
        let mut path = HashSet::new();
        stack.push((sequence, false));

        while let Some((element, exit)) = stack.pop() {
            if exit {
                path.remove(&element);
                continue;
            }
            let link = links
                .get(element)
                .ok_or_else(|| anyhow::anyhow!("Sequence link {element} does not exist."))?;
            if link.target == self.unicode_symbol_criterion_matcher.target() {
                output.push(element);
                continue;
            }
            if !path.insert(element) {
                bail!("Unicode sequence cycle at link {element}.");
            }
            stack.push((element, true));
            stack.push((link.target, false));
            stack.push((link.source, false));
        }

        Ok(output)
    }
}
