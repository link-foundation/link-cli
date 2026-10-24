//! Compare indexed and scan algorithms through mutations and upstream decorators.
use anyhow::Result;
use link_cli::doublets::{self, Doublets};
use link_cli::{link_storage_constants, Link, LinkStorage, NamedTypeLinks, NamedTypesDecorator};
#[cfg(not(target_arch = "wasm32"))]
use tempfile::tempdir;

fn snapshot(storage: &LinkStorage, query: &[u32]) -> Vec<Link> {
    let mut links = Vec::new();
    storage.each_by(query, |link| {
        links.push(Link::from(link));
        doublets::data::Flow::Continue
    });
    links
}

fn assert_parity(plain: &LinkStorage, indexed: &LinkStorage) {
    assert_eq!(plain.all(), indexed.all());
    let any = link_storage_constants().any;
    for source in [0, 1, 2, 3, 4, 5, 99, any, u32::MAX] {
        for target in [0, 1, 2, 3, 4, 5, 99, any, u32::MAX] {
            assert_eq!(plain.search(source, target), indexed.search(source, target));
            for index in [None, Some(1), Some(99)] {
                for s in [None, Some(source)] {
                    for t in [None, Some(target)] {
                        let ordered = |storage: &LinkStorage| {
                            let mut links: Vec<_> =
                                storage.query(index, s, t).into_iter().copied().collect();
                            links.sort_by_key(|link| link.index);
                            links
                        };
                        assert_eq!(ordered(plain), ordered(indexed));
                    }
                }
            }
            assert_eq!(
                snapshot(plain, &[any, source, target]),
                snapshot(indexed, &[any, source, target])
            );
        }
        assert_eq!(
            snapshot(plain, &[any, source]),
            snapshot(indexed, &[any, source])
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn indexes_follow_raw_writes_duplicates_and_reused_addresses() -> Result<()> {
    let mut plain = LinkStorage::in_memory(false);
    plain.create(1, 1);
    plain.create(1, 1);
    let mut indexed = LinkStorage::in_memory(false).with_indexed_lookups();
    indexed.create(1, 1);
    indexed.create(1, 1);
    for step in 0..48 {
        match step % 4 {
            0 => {
                assert_eq!(plain.create(2, 3), indexed.create(2, 3));
            }
            1 => {
                plain.update_raw(1, 3, 4)?;
                indexed.update_raw(1, 3, 4)?;
            }
            2 => {
                plain.delete_raw(1)?;
                indexed.delete_raw(1)?;
            }
            _ => {
                assert_eq!(plain.create(1, 1), indexed.create(1, 1));
            }
        }
        assert_parity(&plain, &indexed);
    }
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn indexes_follow_uniqueness_merges_and_cascading_deletes() -> Result<()> {
    let mut plain = LinkStorage::in_memory(false);
    let mut indexed = LinkStorage::in_memory(false).with_indexed_lookups();
    for store in [&mut plain, &mut indexed] {
        store.create(1, 1);
        store.create(2, 2);
        store.create(1, 2);
        store.create(3, 2);
    }
    assert_parity(&plain, &indexed);
    plain.update(3, 1, 1)?;
    indexed.update(3, 1, 1)?;
    assert_parity(&plain, &indexed);
    plain.delete(2)?;
    indexed.delete(2)?;
    assert_parity(&plain, &indexed);
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg(not(target_arch = "wasm32"))]
fn enabling_after_open_and_reloading_rebuilds_indexes() -> Result<()> {
    let dir = tempdir()?;
    let path = dir.path().join("data.links");
    let mut writer = LinkStorage::new(&path, false)?;
    let id = writer.create(1, 2);
    writer.save()?;
    let mut reader = LinkStorage::new(&path, false)?.with_indexed_lookups();
    assert_parity(&writer, &reader);
    writer.update_raw(id, 3, 4)?;
    writer.save()?;
    reader.reload_from_disk()?;
    assert_parity(&writer, &reader);
    assert_eq!(None, reader.search(1, 2));
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn indexed_names_match_baseline_after_rename_reassignment_and_deletion() -> Result<()> {
    let make = || {
        NamedTypesDecorator::from_link_storages(
            LinkStorage::in_memory(false),
            LinkStorage::in_memory(false),
        )
    };
    let mut plain = make();
    let mut indexed = make().with_indexed_lookups();
    for store in [&mut plain, &mut indexed] {
        for name in [
            "Type",
            "Name",
            "String",
            "UnicodeSymbol",
            "UnicodeSequence",
            "EmptyString",
            "雪😀",
            "punctuation:()\nline",
        ] {
            let id = NamedTypeLinks::get_or_create_named(store, name)?;
            assert_eq!(Some(id), NamedTypeLinks::get_by_name(store, name)?);
        }
        NamedTypeLinks::set_name(store, 7, "renamed")?;
        NamedTypeLinks::set_name(store, 8, "renamed")?;
        store.delete(8)?;
    }
    assert_eq!(
        NamedTypeLinks::lino_lines(&mut plain)?,
        NamedTypeLinks::lino_lines(&mut indexed)?
    );
    for name in ["Type", "Name", "雪😀", "renamed", "missing"] {
        assert_eq!(
            NamedTypeLinks::get_by_name(&mut plain, name)?,
            NamedTypeLinks::get_by_name(&mut indexed, name)?
        );
    }
    assert_parity(plain.names_links(), indexed.names_links());
    Ok(())
}
