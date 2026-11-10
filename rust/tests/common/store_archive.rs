//! Shared finite archive vectors for native, remote and actual-WASM contracts.
#![allow(dead_code)] // Each native test binary uses a different helper subset.
use link_cli::{
    protocol::{export_store, LinksPacket, Reference},
    Link, NamedTypeLinks,
};

/// The archive of [`fill`], byte for byte; the C# `StoreArchiveTests`
/// assert the same bytes.
///
/// - Links packet, 13 bytes: `12` explicit layout, `02` sections; `20 01`
///   one doublet of 1-byte references, `01 01`; `24 01 02` a section after a
///   gap of one address (the hole at 2) with two doublets, `03 03 01 03`.
/// - Names packet, 17 bytes: `13` explicit layout with external references,
///   `02` sections; `21 02` two links of two 2-byte references,
///   `ff ff ff 9f` (1, `a`) and `ff fd ff 17` (3, `é`); `30 01` one link of
///   three 1-byte references, `fc 9f 9e` (4, `a`, `b`).
pub const ARCHIVE: &str = "12 02 20 01 24 01 02 01 01 03 03 01 03 \
                       13 02 21 02 30 01 ff ff 9f ff fd ff 17 ff fc 9f 9e";

/// Links `1: 1 1` named `a`, `3: 3 3` named `é` and `4: 1 3` named `ab`,
/// with a hole at 2.
pub fn fill(store: &mut impl NamedTypeLinks) {
    for _ in 0..3 {
        let id = store.create(0, 0);
        store.update(id, id, id).unwrap();
    }
    assert_eq!(store.create(1, 3), 4);
    store.delete(2).unwrap();
    store.set_name(1, "a").unwrap();
    store.set_name(3, "é").unwrap();
    store.set_name(4, "ab").unwrap();
}

pub fn export(store: &mut impl NamedTypeLinks) -> Vec<u8> {
    let mut bytes = Vec::new();
    export_store(store, &mut bytes).unwrap();
    bytes
}

pub fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn bytes(hex: &str) -> Vec<u8> {
    hex.split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

pub fn names(store: &mut impl NamedTypeLinks) -> Vec<(u32, Option<String>)> {
    (1..=4)
        .map(|id| (id, store.get_name(id).unwrap()))
        .collect()
}

pub fn links(store: &mut impl NamedTypeLinks) -> Vec<Link> {
    let mut links = store.all_links();
    links.sort_by_key(|link| link.index);
    links
}

pub fn packet(external_references: bool, links: &[(u64, Vec<Reference>)]) -> Vec<u8> {
    LinksPacket::pack(external_references, links, true)
        .unwrap()
        .to_bytes()
        .unwrap()
}

pub fn doublet(address: u64, source: u64, target: u64) -> (u64, Vec<Reference>) {
    (
        address,
        vec![Reference::Internal(source), Reference::Internal(target)],
    )
}

pub fn no_names() -> Vec<u8> {
    packet(true, &[])
}
