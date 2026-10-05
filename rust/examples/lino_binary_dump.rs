//! Prints the binary LiNo encoding of a document under several option sets.
//!
//! ```text
//! cargo run --example lino_binary_dump -- "() ((1 1))"
//! VERBOSE=1 cargo run --example lino_binary_dump -- "(a b c)"
//! ```

use link_cli::protocol::{encode_document, parse_document, ArityRange, BinaryLinoOptions};

fn main() {
    let text = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "() ((1 1))".into());
    let document = parse_document(&text).expect("valid LiNo");
    println!("document: {text}");
    for external_references in [false, true] {
        for arity in [
            ArityRange::DOUBLETS,
            ArityRange::between(2, 3),
            ArityRange::at_least(1),
        ] {
            for packed_widths in [false, true] {
                let options = BinaryLinoOptions {
                    external_references,
                    arity,
                    packed_widths,
                };
                let packet = encode_document(&document, options).expect("encodable");
                let bytes = packet.to_bytes().expect("serializable");
                let hex = bytes
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                println!(
                    "external={external_references:<5} arity={arity:<4} packed={packed_widths:<5} {:>4} bytes: {hex}",
                    bytes.len()
                );
                if std::env::var_os("VERBOSE").is_some() {
                    for section in &packet.sections {
                        println!(
                            "  gap {} arity {} width {}: {:?}",
                            section.gap, section.arity, section.width, section.links
                        );
                    }
                }
            }
        }
    }
}
