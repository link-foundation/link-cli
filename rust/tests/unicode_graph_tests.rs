//! Checked decoding preserves sequence order and shared subtree occurrences.
use anyhow::Result;
use link_cli::{external_reference, LinkStorage, UnicodeStringStorage};

#[cfg(target_arch = "wasm32")]
#[path = "common/unicode_graph.rs"]
mod graph_contract;

fn wrap(storage: &mut UnicodeStringStorage<'_>, root: u32) -> u32 {
    let sequence_type = storage.unicode_sequence_type();
    let string_type = storage.string_type();
    let sequence = storage.links_mut().get_or_create(root, sequence_type);
    storage.links_mut().get_or_create(string_type, sequence)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn shared_subtrees_expand_each_occurrence_in_unicode_order() -> Result<()> {
    let mut links = LinkStorage::in_memory(false);
    let mut storage = UnicodeStringStorage::new(&mut links)?;
    let original = storage.create_string("A😀雪")?;
    let sequence = storage.links_mut().get(original).unwrap().target;
    let root = storage.links_mut().get(sequence).unwrap().source;
    let shared = storage.links_mut().get_or_create(root, root);
    let value = wrap(&mut storage, shared);
    let wrapper = storage.links_mut().get_or_create(0, value);
    let wrapper = storage.links_mut().get_or_create(0, wrapper);
    assert_eq!(storage.get_string(wrapper)?, "A😀雪A😀雪");
    assert_eq!(
        storage.unicode_sequence_code_units(wrapper)?,
        "A😀雪A😀雪".encode_utf16().collect::<Vec<_>>()
    );
    let walker = storage.sequence_walker();
    assert_eq!(
        walker.try_walk(storage.links_mut(), shared)?,
        walker.walk(storage.links_mut(), shared)
    );
    let too_many_wrappers = storage.links_mut().get_or_create(0, wrapper);
    assert!(storage.get_string(too_many_wrappers).is_err());
    assert!(storage
        .unicode_sequence_code_units(too_many_wrappers)
        .is_err());
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn deep_finite_sequence_uses_an_explicit_stack() -> Result<()> {
    let mut links = LinkStorage::in_memory(false);
    let mut storage = UnicodeStringStorage::new(&mut links)?;
    let converter = storage.char_to_unicode_symbol_converter();
    let symbol = converter.convert(storage.links_mut(), 'A' as u16);
    let mut root = symbol;
    for _ in 0..2048 {
        root = storage.links_mut().get_or_create(root, symbol);
    }
    let value = wrap(&mut storage, root);
    assert_eq!(storage.get_string(value)?, "A".repeat(2049));
    assert_eq!(storage.unicode_sequence_code_units(value)?, vec![65; 2049]);
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn missing_sequence_nodes_fail_instead_of_silently_truncating() -> Result<()> {
    let mut links = LinkStorage::in_memory(false);
    let mut storage = UnicodeStringStorage::new(&mut links)?;
    let converter = storage.char_to_unicode_symbol_converter();
    let symbol = converter.convert(storage.links_mut(), 'Z' as u16);
    for (source, target) in [(external_reference(123456), symbol), (symbol, 0)] {
        let root = storage.links_mut().get_or_create(source, target);
        let value = wrap(&mut storage, root);
        for _ in 0..2 {
            assert!(storage.get_string(value).is_err());
            assert!(storage.unicode_sequence_code_units(value).is_err());
        }
    }
    let missing = external_reference(123456);
    assert!(storage.get_string(missing).is_err());
    assert!(storage.unicode_sequence_code_units(missing).is_err());
    let following = storage.create_string("following")?;
    assert_eq!(storage.get_string(following)?, "following");
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn exposed_converters_compose_with_pinned_and_new_types() -> Result<()> {
    let mut links = LinkStorage::in_memory(false);
    let mut storage = UnicodeStringStorage::new(&mut links)?;
    let pinned = [
        storage.type_type(),
        storage.unicode_symbol_type(),
        storage.unicode_sequence_type(),
        storage.string_type(),
        storage.empty_string_type(),
        storage.name_type(),
    ];
    let types = storage.get_types();
    for address in pinned {
        assert!(types.contains(&address));
        assert!(storage.is_type(address));
    }
    assert!(!storage.is_type(external_reference(123456)));
    let custom = storage.get_or_create_type("Custom 雪")?;
    assert_eq!(storage.get_or_create_type("Custom 雪")?, custom);
    assert_eq!(storage.get_name(custom)?.as_deref(), Some("Custom 雪"));
    assert!(storage.get_types().contains(&custom));

    let raw = storage.address_to_number_converter().convert(65);
    assert_eq!(raw, external_reference(65));
    assert_eq!(storage.number_to_address_converter().convert(raw), 65);
    let to_symbol = storage.char_to_unicode_symbol_converter();
    let symbol = to_symbol.convert(storage.links_mut(), 65);
    let matcher = storage.unicode_symbol_criterion_matcher();
    assert!(matcher.is_matched(storage.links_mut(), symbol));
    let from_symbol = storage.unicode_symbol_to_char_converter();
    assert_eq!(from_symbol.convert(storage.links_mut(), symbol)?, 65);
    let root = storage
        .balanced_variant_converter()
        .convert(storage.links_mut(), &[symbol, symbol, symbol]);
    let value = wrap(&mut storage, root);
    assert_eq!(storage.get_string(value)?, "AAA");
    let to_sequence = storage.string_to_unicode_sequence_converter();
    let sequence = to_sequence.convert(storage.links_mut(), "B😀雪");
    let matcher = storage.unicode_sequence_criterion_matcher();
    assert!(matcher.is_matched(storage.links_mut(), sequence));
    let from_sequence = storage.unicode_sequence_to_string_converter();
    assert_eq!(
        from_sequence.convert(storage.links_mut(), sequence)?,
        "B😀雪"
    );
    let empty = storage.unicode_sequence_type();
    assert_eq!(to_sequence.convert(storage.links_mut(), ""), empty);
    assert_eq!(from_sequence.convert(storage.links_mut(), empty)?, "");
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_symbols_sequences_and_utf16_keep_fallible_decoding() -> Result<()> {
    let mut links = LinkStorage::in_memory(false);
    let mut storage = UnicodeStringStorage::new(&mut links)?;
    let from_sequence = storage.unicode_sequence_to_string_converter();
    let from_symbol = storage.unicode_symbol_to_char_converter();
    assert!(from_sequence.convert(storage.links_mut(), 0).is_err());
    assert!(from_symbol.convert(storage.links_mut(), 0).is_err());
    let symbol_type = storage.unicode_symbol_type();
    let invalid = storage
        .links_mut()
        .get_or_create(external_reference(65536), symbol_type);
    assert!(from_symbol.convert(storage.links_mut(), invalid).is_err());
    let value = wrap(&mut storage, invalid);
    assert!(storage.get_string(value).is_err());
    assert!(storage.unicode_sequence_code_units(value).is_err());
    let to_symbol = storage.char_to_unicode_symbol_converter();
    let surrogate = to_symbol.convert(storage.links_mut(), 0xd800);
    let value = wrap(&mut storage, surrogate);
    assert!(storage.get_string(value).is_err());
    assert_eq!(storage.unicode_sequence_code_units(value)?, [0xd800]);
    let wrong_type = storage.links_mut().get_or_create(symbol_type, symbol_type);
    let string_type = storage.string_type();
    let value = storage.links_mut().get_or_create(string_type, wrong_type);
    assert!(storage.get_string(value).is_err());
    assert!(storage.unicode_sequence_code_units(value).is_err());
    let empty = storage.create_string("")?;
    assert_eq!(storage.get_string(empty)?, "");
    assert!(storage.unicode_sequence_code_units(empty)?.is_empty());
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
#[should_panic(expected = "Invalid sequence graph; use try_walk")]
fn legacy_vec_walker_rejects_invalid_graphs_with_a_bounded_panic() {
    use link_cli::sequences::{RightSequenceWalker, TargetMatcher};
    let links = LinkStorage::in_memory(false);
    RightSequenceWalker::new(TargetMatcher::new(1)).walk(&links, 0);
}

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn actual_wasm_rejects_cyclic_strings() -> Result<()> {
    graph_contract::cyclic_decoding("string")
}
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen_test::wasm_bindgen_test]
fn actual_wasm_rejects_cyclic_code_units() -> Result<()> {
    graph_contract::cyclic_decoding("code-units")
}
