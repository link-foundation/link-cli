//! Malformed, hostile and interrupted binary links notation input (issues
//! #104 and #105): every rejection names its exact cause, decoding limits
//! bound hostile packets and stream failures surface as I/O errors.
//!
//! Split from `protocol_packet_tests.rs` for issue #172 to keep that file
//! below the 900-line warning threshold of `rust/scripts/check-file-size.rs`.

use link_cli::protocol::{
    parse_document, BinaryError, BinaryLinoProtocol, DecodeLimits, LinksPacket, ProtocolError,
    Reference,
};
use std::io::{self, Cursor, Read};

fn hex(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn unhex(text: &str) -> Vec<u8> {
    text.split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

fn expect_error(bytes: &[u8], limits: DecodeLimits) -> ProtocolError {
    let protocol = BinaryLinoProtocol {
        limits,
        ..BinaryLinoProtocol::default()
    };
    protocol.decode(bytes).expect_err("must be rejected")
}

/// Asserts that `bytes` is rejected as malformed because of `detail`.
fn assert_malformed(bytes: &[u8], detail: &str) {
    match expect_error(bytes, DecodeLimits::default()) {
        ProtocolError::Malformed(message) => {
            assert_eq!(message, detail, "{}", hex(bytes))
        }
        other => panic!("{}: {other}", hex(bytes)),
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn malformed_packets_are_rejected() {
    // Each case names the exact problem, so none passes for another reason.
    for (bytes, detail) in [
        ("", "empty input"),
        ("20 00", "unsupported binary header byte 0x20"),
        ("10 01 06 00", "link 6 refers to Internal(6), which is not an earlier link"),
        ("10 01 07 00", "link 6 refers to Internal(7), which is not an earlier link"),
        ("10 02 00", "unexpected end of packet"),
        ("10 00 00", "trailing bytes after the packet"),
        // A tenth byte above 1, then an eleventh byte.
        (
            "10 ff ff ff ff ff ff ff ff ff 7f",
            "LEB128 value overflows 64 bits",
        ),
        (
            "10 80 80 80 80 80 80 80 80 80 81",
            "LEB128 value overflows 64 bits",
        ),
        ("10 01 01 01", "the root link is not a list"),
        // The chain ends in marker 2, not null.
        ("10 02 00 02 04 06", "broken element chain"),
        ("10 01 04 02", "broken element chain"),
        ("10 02 02 00 04 06", "marker 2 used as a value"),
        (
            "10 03 01 00 06 00 04 07",
            "marker 1 cannot start a typed value",
        ),
        // (Number) alone.
        (
            "12 02 14 05 01 20 02 02 06 00 04 07",
            "a number needs exactly one value",
        ),
        ("10 03 05 00 06 00 04 07", "an identified link needs an id"),
        // (Number 3): marker 3 is no number.
        ("10 03 02 03 06 00 04 07", "expected a unary number"),
        // (Number 6) where link 6 is (5 5), not a unary number.
        (
            "10 05 05 05 06 00 02 07 08 00 04 09",
            "expected a unary number",
        ),
        // The id is ().
        (
            "10 04 00 00 05 06 07 00 04 08",
            "a link id must be a reference",
        ),
        (
            "16 00",
            "the explicit layout keeps the header width bits clear",
        ),
        ("12 01 04 05 00", "arity must be at least 1"),
        // A link of 6 in a section of arity 1..2.
        (
            "12 01 18 01 01 05 00",
            "link length outside the section arity 1..2",
        ),
        (
            "12 01 f8 ff ff ff ff ff ff ff ff 01 ff ff ff ff ff ff ff ff ff 01 00",
            "arity range overflows 64 bits",
        ),
        // The gap, then the count.
        (
            "12 01 24 ff ff ff ff ff ff ff ff ff 01 01",
            "addresses overflow 64 bits",
        ),
        (
            "12 01 20 ff ff ff ff ff ff ff ff ff 01",
            "addresses overflow 64 bits",
        ),
        (
            "12 01 24 06 01 01 01",
            "a LiNo packet stores its links contiguously from address 6, found link 7 where 6 belongs",
        ),
        // A hole.
        (
            "12 02 24 05 01 24 01 01 01 01 04 06",
            "a LiNo packet stores its links contiguously from address 6, found link 8 where 7 belongs",
        ),
    ] {
        assert_malformed(&unhex(bytes), detail);
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_code_points_are_rejected() {
    // (String (0x110000)) is not a valid code point.
    let links = [
        vec![Reference::External(0x11_0000), Reference::NULL],
        vec![Reference::Internal(3), Reference::Internal(6)],
        vec![Reference::Internal(7), Reference::NULL],
        vec![Reference::Internal(4), Reference::Internal(8)],
    ];
    let links = (6..).zip(links).collect::<Vec<_>>();
    assert_malformed(
        &LinksPacket::pack(true, &links, false)
            .unwrap()
            .to_bytes()
            .unwrap(),
        "invalid code point 1114112",
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn hostile_packets_hit_limits() {
    let limited = |bytes: &[u8], limits: DecodeLimits| {
        assert!(
            matches!(expect_error(bytes, limits), ProtocolError::LimitExceeded(_)),
            "{}",
            hex(bytes)
        );
    };
    let few_links = DecodeLimits {
        max_links: 8,
        ..DecodeLimits::default()
    };
    limited(&[0x10, 0x09], few_links);
    limited(&[0x12, 0x09], few_links); // more sections than links allowed
    limited(&[0x12, 0x02, 0x20, 0x05, 0x20, 0x05], few_links); // 10 in all
    let few_references = DecodeLimits {
        max_references: 5,
        ..DecodeLimits::default()
    };
    limited(&[0x10, 0x03, 1, 1, 1, 1, 1, 1], few_references);
    limited(&[0x12, 0x01, 0x18, 0x00, 0x01, 0x05], few_references); // one link of 6

    // A doubling chain `d(k) = (d(k-1) d(k-1))` expands to 2^k nodes.
    let mut links = vec![vec![Reference::NULL, Reference::NULL]];
    for address in 6..60u64 {
        links.push(vec![
            Reference::Internal(address),
            Reference::Internal(address),
        ]);
    }
    let last = 5 + links.len() as u64;
    links.push(vec![Reference::Internal(last), Reference::NULL]);
    links.push(vec![Reference::Internal(4), Reference::Internal(last + 1)]);
    let links = (6..).zip(links).collect::<Vec<_>>();
    let bomb = LinksPacket::pack(false, &links, false).unwrap();
    let small_budget = DecodeLimits {
        max_nodes: 1 << 12,
        ..DecodeLimits::default()
    };
    let limit = |bytes: &[u8], limits| expect_error(bytes, limits).to_string();
    assert_eq!(
        limit(&bomb.to_bytes().unwrap(), small_budget),
        "limit exceeded: too many LiNo nodes"
    );

    let deep = format!("{}x{}", "(y ".repeat(40), ")".repeat(40));
    let bytes = BinaryLinoProtocol::new()
        .encode(&parse_document(&deep).unwrap())
        .unwrap();
    let shallow = DecodeLimits {
        max_depth: 10,
        ..DecodeLimits::default()
    };
    assert_eq!(
        limit(&bytes, shallow),
        "limit exceeded: nesting deeper than 10"
    );

    let three_links = BinaryLinoProtocol::new()
        .encode(&parse_document("a b c").unwrap())
        .unwrap();
    let two_nodes = DecodeLimits {
        max_nodes: 2,
        ..DecodeLimits::default()
    };
    assert_eq!(
        limit(&three_links, two_nodes),
        "limit exceeded: chain too long"
    );
    assert!(DecodeLimits::unlimited().max_links == u64::MAX);
}

/// Fails once with `kind`, then reads `bytes`.
struct Hiccup {
    kind: Option<io::ErrorKind>,
    bytes: Cursor<&'static [u8]>,
}

impl Hiccup {
    fn new(kind: io::ErrorKind, bytes: &'static [u8]) -> Self {
        Self {
            kind: Some(kind),
            bytes: Cursor::new(bytes),
        }
    }
}

impl Read for Hiccup {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        match self.kind.take() {
            Some(kind) => Err(kind.into()),
            None => self.bytes.read(buffer),
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn stream_failures_are_io_errors_and_interruptions_are_retried() {
    let limits = DecodeLimits::default();
    let read = |reader: &mut dyn Read| LinksPacket::read_from(reader, &limits);

    let packet = read(&mut Hiccup::new(io::ErrorKind::Interrupted, &[0x10, 0x00]));
    assert_eq!(packet.unwrap().unwrap().links().count(), 0);
    // Before the header, then inside the packet.
    let reset = io::ErrorKind::ConnectionReset;
    assert!(matches!(
        read(&mut Hiccup::new(reset, &[])),
        Err(BinaryError::Io(_))
    ));
    assert!(matches!(
        read(&mut [0x10u8].chain(Hiccup::new(reset, &[]))),
        Err(BinaryError::Io(_))
    ));
}
