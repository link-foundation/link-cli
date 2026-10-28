//! Pure protocol options shared by native and WASM contract tests.
#![allow(dead_code)] // Individual test binaries use only part of this matrix.

use link_cli::protocol::{
    ArityRange, BinaryLinoOptions, BinaryLinoProtocol, LinoProtocol, TextLinoProtocol,
};

/// Every combination of the binary options the tests cover: plain and
/// external references, doublets, doublets and triplets, and any arity, with
/// uniform and packed widths.
pub fn binary_options() -> Vec<BinaryLinoOptions> {
    let mut options = Vec::new();
    for external_references in [false, true] {
        for arity in [
            ArityRange::DOUBLETS,
            ArityRange::between(2, 3),
            ArityRange::at_least(1),
        ] {
            for packed_widths in [false, true] {
                options.push(BinaryLinoOptions {
                    external_references,
                    arity,
                    packed_widths,
                });
            }
        }
    }
    options
}

pub fn protocols() -> Vec<Box<dyn LinoProtocol>> {
    let mut protocols: Vec<Box<dyn LinoProtocol>> = vec![Box::new(TextLinoProtocol::new())];
    for options in binary_options() {
        protocols.push(Box::new(BinaryLinoProtocol::with_options(options)));
    }
    protocols
}
