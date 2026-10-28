use link_cli::{external_reference, LinkStorage, UnicodeStringStorage};

fn main() {
    let mut links = LinkStorage::in_memory(false);
    let mut storage = UnicodeStringStorage::new(&mut links).unwrap();
    let symbol = storage.char_to_unicode_symbol_converter();
    let symbol = symbol.convert(storage.links_mut(), 'Z' as u16);
    let root = storage.links_mut().get_or_create(external_reference(123456), symbol);
    let sequence_type = storage.unicode_sequence_type();
    let string_type = storage.string_type();
    let sequence = storage.links_mut().get_or_create(root, sequence_type);
    let value = storage.links_mut().get_or_create(string_type, sequence);
    let string = storage.get_string(value);
    let units = storage.unicode_sequence_code_units(value);
    eprintln!("missing child: string={string:?}; code_units={units:?}");
    assert!(string.is_err() && units.is_err(), "missing nodes must not silently discard data");
}
