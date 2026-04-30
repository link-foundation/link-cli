use crate::link_storage::LinkStorage;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TargetMatcher {
    target: u32,
}

impl TargetMatcher {
    pub fn new(target: u32) -> Self {
        Self { target }
    }

    pub fn target(&self) -> u32 {
        self.target
    }

    pub fn is_matched(&self, links: &LinkStorage, link: u32) -> bool {
        links
            .get(link)
            .is_some_and(|candidate| candidate.target == self.target)
    }
}
