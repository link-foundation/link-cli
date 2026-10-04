//! Prints the binary LiNo encoding of a document under every option set.
//!
//! ```text
//! cargo run --example lino_binary_dump -- "() ((1 1))"
//! ```

use link_cli::protocol::{encode_document, parse_document, BinaryLinoOptions};

fn main() {
    let text = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "() ((1 1))".into());
    let document = parse_document(&text).expect("valid LiNo");
    println!("document: {text}");
    for external_references in [false, true] {
        for sequences in [false, true] {
            let options = BinaryLinoOptions {
                external_references,
                sequences,
                progressive_widths: false,
            };
            let packet = encode_document(&document, options).expect("encodable");
            let bytes = packet.to_bytes().expect("serializable");
            let hex = bytes
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<Vec<_>>()
                .join(" ");
            println!(
                "external_references={external_references:<5} sequences={sequences:<5} {:>4} bytes: {hex}",
                bytes.len()
            );
            if std::env::var_os("VERBOSE").is_some() {
                println!("  doublets:  {:?}", packet.doublets);
                println!("  sequences: {:?}", packet.sequences);
            }
        }
    }
}
