//! Pinned type allocation compatible with the C# `PinnedTypes` helper.

use anyhow::{bail, Result};

use crate::link_storage::LinkStorage;

/// Creates or validates the reserved type links at deterministic addresses.
pub struct PinnedTypes<'a> {
    links: &'a mut LinkStorage,
    current: u32,
    initial_source: u32,
}

impl<'a> PinnedTypes<'a> {
    pub fn new(links: &'a mut LinkStorage) -> Self {
        Self {
            links,
            current: 1,
            initial_source: 1,
        }
    }

    pub fn next_type(&mut self) -> Result<u32> {
        let address = self.current;
        if let Some(link) = self.links.get(address) {
            if link.index != address || link.source != self.initial_source || link.target != address
            {
                bail!(
                    "Unexpected link found at address {address}. Expected: ({address}: {} {address}), Found: ({}: {} {}).",
                    self.initial_source,
                    link.index,
                    link.source,
                    link.target
                );
            }
        } else {
            let created = self.links.get_or_create(self.initial_source, address);
            if created != address {
                bail!(
                    "Pinned type address {address} could not be created deterministically; got {created}."
                );
            }
        }

        self.current += 1;
        Ok(address)
    }
}
