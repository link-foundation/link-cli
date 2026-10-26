//! Shared small-address text/archive fixtures; never reserve a huge address.
use anyhow::Result;
use link_cli::{
    import_lino_text,
    protocol::{import_store, LinksPacket, Reference},
    NamedTypeLinks,
};

pub fn run(links: &mut impl NamedTypeLinks, operation: &str) -> Result<()> {
    match operation {
        "import-explicit" => import_lino_text(links, "(8: 1 8)"),
        "import-nested" => import_lino_text(links, "(8: (1 8) 1)"),
        "import-named" => import_lino_text(links, "('imported 雪': 1 0)"),
        "import-archive" => {
            let mut bytes = LinksPacket::pack(
                false,
                &[(8, vec![Reference::Internal(1), Reference::Internal(8)])],
                true,
            )?
            .to_bytes()?;
            let mut name = vec![Reference::External(8)];
            name.extend("imported 雪".chars().map(|c| Reference::External(c as u64)));
            bytes.extend(LinksPacket::pack(true, &[(1, name)], true)?.to_bytes()?);
            import_store(links, &mut bytes.as_slice())
        }
        _ => unreachable!("unknown finite import fixture: {operation}"),
    }
}
