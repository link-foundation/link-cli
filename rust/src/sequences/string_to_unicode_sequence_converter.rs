use crate::link_storage::LinkStorage;
use crate::sequences::{BalancedVariantConverter, CharToUnicodeSymbolConverter};

#[derive(Clone, Copy, Debug)]
pub struct StringToUnicodeSequenceConverter {
    char_to_unicode_symbol_converter: CharToUnicodeSymbolConverter,
    balanced_variant_converter: BalancedVariantConverter,
    unicode_sequence_type: u32,
}

impl StringToUnicodeSequenceConverter {
    pub fn new(
        char_to_unicode_symbol_converter: CharToUnicodeSymbolConverter,
        balanced_variant_converter: BalancedVariantConverter,
        unicode_sequence_type: u32,
    ) -> Self {
        Self {
            char_to_unicode_symbol_converter,
            balanced_variant_converter,
            unicode_sequence_type,
        }
    }

    pub fn convert(&self, links: &mut LinkStorage, content: &str) -> u32 {
        let symbols = content
            .encode_utf16()
            .map(|code_unit| {
                self.char_to_unicode_symbol_converter
                    .convert(links, code_unit)
            })
            .collect::<Vec<_>>();

        if symbols.is_empty() {
            return self.unicode_sequence_type;
        }

        let sequence = self.balanced_variant_converter.convert(links, &symbols);
        links.get_or_create(sequence, self.unicode_sequence_type)
    }
}
