//! Helpers of `union_and_remap_links_tests.rs` and
//! `permissions_integration_tests.rs`.

#![allow(dead_code)] // Every test crate that includes it uses only part of it.

use anyhow::Result;
use link_cli::NamedTypeLinks;

/// A new link `(source target)`, made as a client makes one: created, then
/// updated, so the store resolves it as any update.
pub fn make(links: &mut dyn NamedTypeLinks, source: u32, target: u32) -> Result<u32> {
    let link = links.try_create(0, 0)?;
    links.update(link, source, target)?;
    Ok(link)
}

/// Every link of `links` as `(index, source, target)`, by index.
pub fn all(links: &mut dyn NamedTypeLinks) -> Vec<(u32, u32, u32)> {
    all_of(links, None, None)
}

/// The links of `links` from `source` to `target`, `None` matching any.
pub fn all_of(
    links: &mut dyn NamedTypeLinks,
    source: Option<u32>,
    target: Option<u32>,
) -> Vec<(u32, u32, u32)> {
    let mut all: Vec<(u32, u32, u32)> = links
        .query_links(None, source, target)
        .into_iter()
        .map(|link| (link.index, link.source, link.target))
        .collect();
    all.sort_unstable();
    all
}
