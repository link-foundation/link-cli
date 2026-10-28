use anyhow::Result;
use link_cli::{LinkStorage, UnicodeStringStorage};

pub fn cyclic_decoding(operation: &str) -> Result<()> {
    for shape in ["source-self", "target-self", "source-pair", "target-pair"] {
        let mut links = LinkStorage::in_memory(false);
        let mut storage = UnicodeStringStorage::new(&mut links)?;
        let symbol = storage.char_to_unicode_symbol_converter();
        let symbol = symbol.convert(storage.links_mut(), 'x' as u16);
        let a = storage.links_mut().create(0, 0);
        let b = storage.links_mut().create(0, 0);
        match shape {
            "source-self" => {
                storage.links_mut().update(a, a, symbol)?;
            }
            "target-self" => {
                storage.links_mut().update(a, symbol, a)?;
            }
            "source-pair" => {
                storage.links_mut().update(a, b, symbol)?;
                storage.links_mut().update(b, a, symbol)?;
            }
            "target-pair" => {
                storage.links_mut().update(a, symbol, b)?;
                storage.links_mut().update(b, symbol, a)?;
            }
            _ => unreachable!(),
        }
        let sequence_type = storage.unicode_sequence_type();
        let string_type = storage.string_type();
        let sequence = storage.links_mut().get_or_create(a, sequence_type);
        let value = storage.links_mut().get_or_create(string_type, sequence);
        for _ in 0..2 {
            let error = match operation {
                "string" => storage.get_string(value).unwrap_err(),
                "code-units" => storage.unicode_sequence_code_units(value).unwrap_err(),
                _ => unreachable!(),
            };
            assert!(error.to_string().contains("cycle"), "{shape}: {error}");
        }
        let row = storage.links_mut().create(0, 0);
        let name_type = storage.name_type();
        let name = storage.links_mut().get_or_create(name_type, value);
        storage.links_mut().get_or_create(row, name);
        storage
            .links_mut()
            .get_or_create(link_cli::external_reference(900), name);
        assert!(storage.get_name(row).is_err());
        assert!(storage.external_names().is_err());
        let healthy = storage.create_string("A😀雪A")?;
        assert_eq!(storage.get_string(healthy)?, "A😀雪A");
        assert_eq!(
            storage.unicode_sequence_code_units(healthy)?,
            "A😀雪A".encode_utf16().collect::<Vec<_>>()
        );
    }
    Ok(())
}
