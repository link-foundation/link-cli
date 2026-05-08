use anyhow::Result;
use link_cli::sequences::{
    AddressToRawNumberConverter, BalancedVariantConverter, CachingConverterDecorator,
    CharToUnicodeSymbolConverter, RawNumberToAddressConverter, RightSequenceWalker,
    StringToUnicodeSequenceConverter, TargetMatcher, UnicodeSequenceToStringConverter,
    UnicodeSymbolToCharConverter,
};
use link_cli::{external_reference, HybridReference, LinkStorage, PinnedTypes};
use std::cell::Cell;
use tempfile::NamedTempFile;

fn with_links(test: impl FnOnce(&mut LinkStorage) -> Result<()>) -> Result<()> {
    let temp_file = NamedTempFile::new()?;
    let db_path = temp_file.path().to_str().unwrap();
    let mut links = LinkStorage::new(db_path, false)?;
    test(&mut links)
}

fn allocate_unicode_types(links: &mut LinkStorage) -> Result<(u32, u32)> {
    let mut pinned_types = PinnedTypes::new(links);
    let _type_type = pinned_types.next_type()?;
    let unicode_symbol_type = pinned_types.next_type()?;
    let unicode_sequence_type = pinned_types.next_type()?;
    Ok((unicode_symbol_type, unicode_sequence_type))
}

#[test]
fn raw_number_converters_match_hybrid_external_reference_encoding() {
    let address_to_number = AddressToRawNumberConverter::new();
    let number_to_address = RawNumberToAddressConverter::new();

    assert_eq!(u32::MAX, external_reference(1));
    assert_eq!(u32::MAX, address_to_number.convert(1));
    assert_eq!(1, number_to_address.convert(u32::MAX));

    let zero = HybridReference::external(0);
    assert!(zero.is_external());
    assert_eq!(Some(0), zero.absolute_value());
}

#[test]
fn target_and_char_symbol_converters_create_and_decode_symbols() -> Result<()> {
    with_links(|links| {
        let (unicode_symbol_type, _) = allocate_unicode_types(links)?;
        let symbol_matcher = TargetMatcher::new(unicode_symbol_type);
        let char_to_symbol = CharToUnicodeSymbolConverter::new(
            AddressToRawNumberConverter::new(),
            unicode_symbol_type,
        );
        let symbol_to_char =
            UnicodeSymbolToCharConverter::new(RawNumberToAddressConverter::new(), symbol_matcher);

        let symbol = char_to_symbol.convert(links, 'A' as u16);

        assert!(symbol_matcher.is_matched(links, symbol));
        assert_eq!('A' as u16, symbol_to_char.convert(links, symbol)?);
        Ok(())
    })
}

#[test]
fn balanced_variant_and_right_sequence_walker_preserve_symbol_order() -> Result<()> {
    with_links(|links| {
        let (unicode_symbol_type, _) = allocate_unicode_types(links)?;
        let symbol_matcher = TargetMatcher::new(unicode_symbol_type);
        let char_to_symbol = CharToUnicodeSymbolConverter::new(
            AddressToRawNumberConverter::new(),
            unicode_symbol_type,
        );
        let symbol_to_char =
            UnicodeSymbolToCharConverter::new(RawNumberToAddressConverter::new(), symbol_matcher);
        let symbols = "ABCDE"
            .encode_utf16()
            .map(|code_unit| char_to_symbol.convert(links, code_unit))
            .collect::<Vec<_>>();

        let root = BalancedVariantConverter::new().convert(links, &symbols);
        let walked = RightSequenceWalker::new(symbol_matcher).walk(links, root);
        let code_units = walked
            .into_iter()
            .map(|symbol| symbol_to_char.convert(links, symbol))
            .collect::<Result<Vec<_>>>()?;

        assert_eq!("ABCDE", String::from_utf16(&code_units)?);
        Ok(())
    })
}

#[test]
fn string_and_unicode_sequence_converters_round_trip_utf16_text() -> Result<()> {
    with_links(|links| {
        let (unicode_symbol_type, unicode_sequence_type) = allocate_unicode_types(links)?;
        let symbol_matcher = TargetMatcher::new(unicode_symbol_type);
        let sequence_matcher = TargetMatcher::new(unicode_sequence_type);
        let char_to_symbol = CharToUnicodeSymbolConverter::new(
            AddressToRawNumberConverter::new(),
            unicode_symbol_type,
        );
        let symbol_to_char =
            UnicodeSymbolToCharConverter::new(RawNumberToAddressConverter::new(), symbol_matcher);
        let string_to_sequence = StringToUnicodeSequenceConverter::new(
            char_to_symbol,
            BalancedVariantConverter::new(),
            unicode_sequence_type,
        );
        let sequence_to_string = UnicodeSequenceToStringConverter::new(
            sequence_matcher,
            RightSequenceWalker::new(symbol_matcher),
            symbol_to_char,
            unicode_sequence_type,
        );
        let input = "A😀B世界";

        let sequence = string_to_sequence.convert(links, input);

        assert!(sequence_matcher.is_matched(links, sequence));
        assert_eq!(input, sequence_to_string.convert(links, sequence)?);
        Ok(())
    })
}

#[test]
fn caching_converter_decorator_reuses_cached_values() -> Result<()> {
    let calls = Cell::new(0);
    let mut cache = CachingConverterDecorator::<String, usize>::new();

    let first = cache.convert_with("Unicode".to_string(), |input| {
        calls.set(calls.get() + 1);
        Ok::<_, anyhow::Error>(input.encode_utf16().count())
    })?;
    let second = cache.convert_with("Unicode".to_string(), |input| {
        calls.set(calls.get() + 1);
        Ok::<_, anyhow::Error>(input.len())
    })?;

    assert_eq!(7, first);
    assert_eq!(first, second);
    assert_eq!(1, calls.get());
    assert_eq!(1, cache.len());
    Ok(())
}
