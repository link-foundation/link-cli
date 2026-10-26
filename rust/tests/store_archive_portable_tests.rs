//! Portable whole-store archive compatibility and malformed-input contracts.
use link_cli::{
    protocol::{import_store, Reference},
    NamedTypeLinks, NamedTypesDecorator,
};
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/storage.rs"]
mod test_store;
#[cfg(target_arch = "wasm32")]
use crate::test_store;
#[cfg(not(target_arch = "wasm32"))]
#[path = "common/store_archive.rs"]
mod archive;
#[cfg(target_arch = "wasm32")]
use crate::archive_fixture as archive;
use archive::*;

fn store() -> (test_store::NamedFiles, NamedTypesDecorator) {
    test_store::named().unwrap()
}

fn import(bytes: &[u8]) -> anyhow::Result<(test_store::NamedFiles, NamedTypesDecorator)> {
    let (files, mut target) = store();
    import_store(&mut target, &mut &bytes[..])?;
    Ok((files, target))
}

fn import_error(bytes: &[u8]) -> String {
    match import(bytes) {
        Ok(_) => panic!("{} should not import", hex(bytes)),
        Err(error) => error.to_string(),
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_store_exports_to_the_golden_archive() {
    let (_source_files, mut source) = store();
    fill(&mut source);

    assert_eq!(hex(&export(&mut source)), hex(&bytes(ARCHIVE)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_imported_archive_restores_links_holes_and_names() {
    let (_source_files, mut source) = store();
    fill(&mut source);
    let (_target_files, mut target) = store();

    import_store(&mut target, &mut &bytes(ARCHIVE)[..]).unwrap();

    assert_eq!(links(&mut target), links(&mut source));
    assert!(!target.exists(2), "the hole at 2 stays a hole");
    assert_eq!(names(&mut target), names(&mut source));
    assert_eq!(hex(&export(&mut target)), hex(&bytes(ARCHIVE)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_empty_store_is_two_empty_packets() {
    let (_empty_files, mut empty) = store();

    assert_eq!(hex(&export(&mut empty)), "10 00 11 00");
    let (_imported_files, mut imported) = import(&bytes("10 00 11 00")).unwrap();
    assert!(imported.all_links().is_empty());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_archive_imports_into_a_store_that_already_has_the_links() {
    let (_target_files, mut target) = store();
    fill(&mut target);

    import_store(&mut target, &mut &bytes(ARCHIVE)[..]).unwrap();

    assert_eq!(hex(&export(&mut target)), hex(&bytes(ARCHIVE)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_truncated_archive_is_rejected() {
    assert!(import_error(&[]).contains("ends before its links"));
    assert!(import_error(&bytes("10 00")).contains("ends before its names"));
    assert!(import_error(&bytes("10 00 11")).contains("malformed"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn trailing_bytes_are_rejected() {
    assert!(import_error(&bytes("10 00 11 00 10")).contains("trailing bytes"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn a_link_must_be_a_doublet_of_link_addresses() {
    let triplet = (
        1,
        vec![
            Reference::Internal(1),
            Reference::Internal(1),
            Reference::Internal(1),
        ],
    );
    let external = (1, vec![Reference::External(1), Reference::Internal(1)]);
    for link in [triplet, external] {
        let mut archive = packet(true, &[link]);
        archive.extend(no_names());
        assert!(import_error(&archive).contains("is not a doublet of link addresses"));
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn addresses_must_fit_a_32_bit_store() {
    let too_large = u64::from(u32::MAX) + 1;
    for link in [doublet(too_large, 1, 1), doublet(1, too_large, 1)] {
        let mut archive = packet(false, &[link]);
        archive.extend(no_names());
        assert!(import_error(&archive).contains("does not fit a 32-bit store"));
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn names_must_be_an_address_and_code_points() {
    let cases = [
        (vec![Reference::Internal(1)], "only external values"),
        (
            vec![Reference::External(1), Reference::External(0xD800)],
            "invalid code point 55296",
        ),
        (
            vec![Reference::External(1), Reference::External(1 << 40)],
            "invalid code point",
        ),
        (
            vec![Reference::External(1 << 40)],
            "does not fit a 32-bit store",
        ),
    ];
    for (name, message) in cases {
        let mut archive = packet(false, &[doublet(1, 1, 1)]);
        archive.extend(packet(true, &[(1, name)]));
        let error = import_error(&archive);
        assert!(error.contains(message), "{error}");
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn an_empty_name_is_kept() {
    let mut archive = packet(false, &[doublet(1, 1, 1)]);
    archive.extend(packet(true, &[(1, vec![Reference::External(1)])]));

    let (_imported_files, mut imported) = import(&archive).unwrap();

    assert_eq!(imported.get_name(1).unwrap().as_deref(), Some(""));
    assert_eq!(hex(&export(&mut imported)), hex(&archive));
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn file_helpers_report_unsupported_filesystem_without_changing_the_store() {
    use link_cli::protocol::{export_store_file, import_store_file};
    let (_files, mut store) = store();
    fill(&mut store);
    let before = (links(&mut store), names(&mut store));
    let path = std::path::Path::new("archive-file-helpers/store.bin");
    for (operation, result) in [
        ("write", export_store_file(&mut store, path)),
        ("read", import_store_file(&mut store, path)),
    ] {
        let error = result.unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "Failed to {operation} the store archive: {}",
                path.display()
            )
        );
        assert_eq!(
            error
                .root_cause()
                .downcast_ref::<std::io::Error>()
                .unwrap()
                .kind(),
            std::io::ErrorKind::Unsupported
        );
        assert_eq!((links(&mut store), names(&mut store)), before);
    }
    let archive = export(&mut store);
    assert_eq!(archive, bytes(ARCHIVE));
    import_store(&mut store, &mut &archive[..]).unwrap();
    assert_eq!((links(&mut store), names(&mut store)), before);
}
