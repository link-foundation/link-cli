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

    pub fn walk(&self, links: &LinkStorage, sequence: u32) -> Vec<u32> {
        let mut output = Vec::new();
        let mut stack = DefaultStack::new();
        stack.push(sequence);

        while let Some(element) = stack.pop() {
            if self
                .unicode_symbol_criterion_matcher
                .is_matched(links, element)
            {
                output.push(element);
                continue;
            }

            if let Some(link) = links.get(element) {
                stack.push(link.target);
                stack.push(link.source);
            }
        }

        output
    }
}
