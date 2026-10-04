//! Binary and text LiNo protocol codecs (issue #105).

use link_cli::external_reference;
use link_cli::protocol::packet::{
    address_tier, decode_external, encode_external, external_capacity, internal_capacity,
};
use link_cli::protocol::{
    decode_document, encode_document, format_document, format_reference, parse_document,
    read_any_document, BinaryLinoOptions, BinaryLinoProtocol, DecodeLimits, LinksPacket,
    LinoProtocol, MessageFormat, ProtocolError, Reference, TextLinoProtocol,
};
use std::io::Cursor;

const CORPUS: &[&str] = &[
    "() ((1 1))",
    "((1: 1 1)) ((1: 1 2))",
    "((1 1)) ()",
    "((1: 1 1)) ()",
    "(($i: $s $t)) (($i: $s $t))",
    "((($index: $source $target)) (($index: $target $source)))",
    "(a b c d)",
    "(name: 'with space' \"it's\")",
    "((a))",
    "(((a)))",
    "(a (b c) ((d)))",
    "1\n2\n3",
    "hello",
    "😀 привет 世界",
    "0",
    "007",
    "9223372036854775807",
    "9223372036854775808",
    "18446744073709551615",
    "18446744073709551616",
    "'multi\nline'",
    ".dot",
    "'a''b\"c`d'",
    "(1: (2: 3 4) 5)",
    "() ()",
    "(* *)",
    "(type: type type)",
];

fn all_options() -> Vec<BinaryLinoOptions> {
    let mut options = Vec::new();
    for external_references in [false, true] {
        for sequences in [false, true] {
            for progressive_widths in [false, true] {
                options.push(BinaryLinoOptions {
                    external_references,
                    sequences,
                    progressive_widths,
                });
            }
        }
    }
    options
}

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

fn encode_hex(text: &str, external_references: bool, sequences: bool) -> String {
    let binary = BinaryLinoProtocol::with_options(BinaryLinoOptions {
        external_references,
        sequences,
        progressive_widths: false,
    });
    hex(&binary.encode(&parse_document(text).unwrap()).unwrap())
}

/// Golden vectors shared with the C# test suite: both implementations must
/// produce exactly these bytes.
const GOLDEN: &[(&str, bool, bool, &str)] = &[
    (
        "() ((1 1))",
        false,
        false,
        "10 07 02 01 06 06 07 00 04 08 00 09 0a 00 04 0b",
    ),
    (
        "() ((1 1))",
        false,
        true,
        "12 02 03 02 01 06 06 01 07 02 00 08 01 09",
    ),
    (
        "() ((1 1))",
        true,
        false,
        "11 06 ff ff 06 00 04 07 00 08 09 00 04 0a",
    ),
    (
        "() ((1 1))",
        true,
        true,
        "13 01 03 ff ff 01 06 02 00 07 01 08",
    ),
    ("hi", true, false, "11 05 97 00 98 06 03 07 08 00 04 09"),
    ("hi", true, true, "13 00 02 03 03 98 97 01 06"),
    ("", false, false, "10 00"),
    ("", true, true, "13 00 00"),
];

#[test]
fn golden_vectors_are_stable() {
    for &(text, external_references, sequences, expected) in GOLDEN {
        assert_eq!(
            encode_hex(text, external_references, sequences),
            expected,
            "{text:?} external={external_references} sequences={sequences}"
        );
        let binary = BinaryLinoProtocol::new();
        assert_eq!(
            binary.decode(&unhex(expected)).unwrap(),
            parse_document(text).unwrap()
        );
    }
}

#[test]
fn every_option_set_round_trips_the_corpus() {
    for text in CORPUS {
        let document = parse_document(text).unwrap();
        for options in all_options() {
            let binary = BinaryLinoProtocol::with_options(options);
            let bytes = binary.encode(&document).unwrap();
            assert!((0x10..=0x1F).contains(&bytes[0]));
            assert_eq!(
                binary.decode(&bytes).unwrap(),
                document,
                "{text:?} with {options:?}"
            );
        }
    }
}

#[test]
fn canonical_text_round_trips_the_corpus() {
    for text in CORPUS {
        let document = parse_document(text).unwrap();
        let canonical = format_document(&document);
        assert_eq!(
            parse_document(&canonical).unwrap(),
            document,
            "{text:?} → {canonical:?}"
        );
    }
    assert_eq!(
        format_document(&parse_document("(()((1 1)))").unwrap()),
        "() ((1 1))"
    );
    assert_eq!(
        format_document(&parse_document("((1: 1 1)) ((1: 1 2))").unwrap()),
        "((1: 1 1)) ((1: 1 2))"
    );
}

#[test]
fn references_are_quoted_only_when_needed() {
    assert_eq!(format_reference("plain"), "plain");
    assert_eq!(format_reference("$x"), "$x");
    assert_eq!(format_reference(""), "''");
    assert_eq!(format_reference("a b"), "'a b'");
    assert_eq!(format_reference("it's"), "\"it's\"");
    assert_eq!(format_reference("'\""), "`'\"`");
    assert_eq!(format_reference("'\"`"), "\"\"\"'\"`\"\"\"");
    for reference in [
        "",
        "a b",
        "a:b",
        "(x)",
        "it's",
        "'\"",
        "'\"`",
        "tab\there",
        "a\nb",
        "'x",
        "x'",
        "'",
        "''",
        "'''",
        "\"'`x`'\"",
        "a''''b\"\"`",
        "`'\"\"\"'''``",
        "(a) ''",
    ] {
        let document = parse_document(&format_reference(reference)).unwrap();
        assert_eq!(format_document(&document), format_reference(reference));
        assert_eq!(
            document,
            vec![links_notation::LiNo::Ref(reference.to_string())]
        );
    }
}

#[test]
fn width_tiers_follow_the_number_of_links() {
    assert_eq!(address_tier(0, false), 1);
    assert_eq!(address_tier(255, false), 1);
    assert_eq!(address_tier(256, false), 2);
    assert_eq!(address_tier(65_535, false), 2);
    assert_eq!(address_tier(65_536, false), 4);
    assert_eq!(address_tier(u64::from(u32::MAX), false), 4);
    assert_eq!(address_tier(u64::from(u32::MAX) + 1, false), 8);
    // External references take the top bit, halving every range.
    assert_eq!(address_tier(127, true), 1);
    assert_eq!(address_tier(128, true), 2);
    assert_eq!(address_tier(32_767, true), 2);
    assert_eq!(address_tier(32_768, true), 4);
    assert_eq!(internal_capacity(8, false), u64::MAX);
    assert_eq!(internal_capacity(8, true), i64::MAX as u64);
    assert_eq!(external_capacity(1), 127);
}

#[test]
fn external_references_match_platform_data_hybrid() {
    for width in [1u8, 2, 4, 8] {
        for value in [0, 1, 2, 100, external_capacity(width)] {
            let raw = encode_external(value, width);
            assert_eq!(
                decode_external(raw, width),
                Some(value),
                "{value} @ {width}"
            );
        }
        assert_eq!(decode_external(internal_capacity(width, true), width), None);
    }
    assert_eq!(encode_external(1, 1), 0xFF);
    assert_eq!(encode_external(0, 1), 0x80);
    for value in [0u32, 1, 5, 1000, i32::MAX as u32] {
        assert_eq!(
            encode_external(u64::from(value), 4),
            u64::from(external_reference(value))
        );
    }
}

fn many_links(count: usize) -> String {
    // Distinct pairs of distinct names keep every link unique.
    (0..count)
        .map(|index| format!("(n{index} m{index})"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn uniform_width_grows_past_256_addresses() {
    let document = parse_document(&many_links(3)).unwrap();
    let small = encode_document(&document, BinaryLinoOptions::default()).unwrap();
    assert!(small.last_address().unwrap() <= 255);
    assert_eq!(small.min_width, 1);

    let document = parse_document(&many_links(400)).unwrap();
    let uniform = encode_document(&document, BinaryLinoOptions::default()).unwrap();
    assert!(uniform.last_address().unwrap() > 255);
    assert_eq!(uniform.min_width, 2);
    let uniform_bytes = uniform.to_bytes().unwrap();
    let header = 1 + 2; // header byte + two-byte LEB128 count
    assert_eq!(uniform_bytes.len(), header + uniform.doublets.len() * 4);

    let progressive = encode_document(
        &document,
        BinaryLinoOptions::default().with_progressive_widths(true),
    )
    .unwrap();
    assert_eq!(progressive.min_width, 1);
    let progressive_bytes = progressive.to_bytes().unwrap();
    assert!(progressive_bytes.len() < uniform_bytes.len());
    let limits = DecodeLimits::default();
    for bytes in [uniform_bytes, progressive_bytes] {
        let packet = LinksPacket::from_bytes(&bytes, &limits).unwrap();
        assert_eq!(decode_document(&packet, &limits).unwrap(), document);
    }
}

#[test]
fn large_external_values_raise_the_minimum_width() {
    let options = BinaryLinoOptions::default()
        .with_external_references(true)
        .with_progressive_widths(true);
    let packet = encode_document(&parse_document("(1000 1)").unwrap(), options).unwrap();
    assert_eq!(packet.min_width, 2);
    let packet = encode_document(&parse_document("(100 1)").unwrap(), options).unwrap();
    assert_eq!(packet.min_width, 1);
    // Beyond 63 bits the number falls back to in-band unary links.
    let packet =
        encode_document(&parse_document("18446744073709551615").unwrap(), options).unwrap();
    assert!(packet.doublets.len() > 60);
}

#[test]
fn hand_built_packets_decode() {
    // One doublet (One One) is 2^1 in unary, so `(Number 7)` is the number 2.
    let packet = LinksPacket {
        min_width: 1,
        doublets: vec![
            (Reference::Internal(1), Reference::Internal(1)),
            (Reference::Internal(2), Reference::Internal(6)),
            (Reference::Internal(7), Reference::NULL),
            (Reference::Internal(4), Reference::Internal(8)),
        ],
        ..LinksPacket::default()
    };
    let bytes = packet.to_bytes().unwrap();
    assert_eq!(hex(&bytes), "10 04 01 01 02 06 07 00 04 08");
    let protocol = BinaryLinoProtocol::new();
    assert_eq!(format_document(&protocol.decode(&bytes).unwrap()), "2");
}

fn expect_error(bytes: &[u8], limits: DecodeLimits) -> ProtocolError {
    let protocol = BinaryLinoProtocol {
        limits,
        ..BinaryLinoProtocol::default()
    };
    protocol.decode(bytes).expect_err("must be rejected")
}

#[test]
fn malformed_packets_are_rejected() {
    let limits = DecodeLimits::default();
    let malformed = |bytes: &[u8]| {
        assert!(
            matches!(expect_error(bytes, limits), ProtocolError::Malformed(_)),
            "{}",
            hex(bytes)
        );
    };
    malformed(&[]);
    malformed(&[0x20, 0x00]); // unknown version nibble
    malformed(&[0x10, 0x01, 0x06, 0x00]); // refers to itself
    malformed(&[0x10, 0x01, 0x07, 0x00]); // refers forward
    malformed(&[0x10, 0x02, 0x00]); // truncated
    malformed(&[0x10, 0x00, 0x00]); // trailing byte
    malformed(&[
        0x10, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x7f,
    ]); // LEB128 overflow
    malformed(&[0x10, 0x01, 0x01, 0x01]); // root is not a list
    malformed(&[0x10, 0x02, 0x00, 0x02, 0x04, 0x06]); // marker 2 used as a value
    malformed(&[0x10, 0x01, 0x04, 0x02]); // a bare marker in a list chain
                                          // (String (0x110000)) is not a valid code point.
    let invalid = LinksPacket {
        external_references: true,
        min_width: 4,
        doublets: vec![
            (Reference::External(0x11_0000), Reference::NULL),
            (Reference::Internal(3), Reference::Internal(6)),
            (Reference::Internal(7), Reference::NULL),
            (Reference::Internal(4), Reference::Internal(8)),
        ],
        ..LinksPacket::default()
    };
    malformed(&invalid.to_bytes().unwrap());
}

#[test]
fn hostile_packets_hit_limits() {
    let limits = DecodeLimits {
        max_links: 8,
        ..DecodeLimits::default()
    };
    assert!(matches!(
        expect_error(&[0x10, 0x09], limits),
        ProtocolError::LimitExceeded(_)
    ));

    // A doubling chain `d(k) = (d(k-1) d(k-1))` expands to 2^k nodes.
    let mut doublets = vec![(Reference::NULL, Reference::NULL)];
    for address in 6..60u64 {
        doublets.push((Reference::Internal(address), Reference::Internal(address)));
    }
    let last = 5 + doublets.len() as u64;
    doublets.push((Reference::Internal(last), Reference::NULL));
    doublets.push((Reference::Internal(4), Reference::Internal(last + 1)));
    let bomb = LinksPacket {
        min_width: 1,
        doublets,
        ..LinksPacket::default()
    };
    let small_budget = DecodeLimits {
        max_nodes: 1 << 12,
        ..DecodeLimits::default()
    };
    assert!(matches!(
        expect_error(&bomb.to_bytes().unwrap(), small_budget),
        ProtocolError::LimitExceeded(_)
    ));

    let deep = format!("{}x{}", "(y ".repeat(40), ")".repeat(40));
    let bytes = BinaryLinoProtocol::new()
        .encode(&parse_document(&deep).unwrap())
        .unwrap();
    let shallow = DecodeLimits {
        max_depth: 10,
        ..DecodeLimits::default()
    };
    assert!(matches!(
        expect_error(&bytes, shallow),
        ProtocolError::LimitExceeded(_)
    ));
}

#[test]
fn text_messages_are_dot_stuffed() {
    let document = parse_document(".dot\n'first\n.second'\n(a b)").unwrap();
    let mut wire = Vec::new();
    TextLinoProtocol::new()
        .write_document(&mut wire, &document)
        .unwrap();
    let wire_text = String::from_utf8(wire.clone()).unwrap();
    assert_eq!(wire_text, "..dot\n'first\n..second'\na b\n.\n");
    let mut reader = Cursor::new(wire);
    let received = TextLinoProtocol::new()
        .read_document(&mut reader)
        .unwrap()
        .unwrap();
    assert_eq!(received, document);
    assert!(TextLinoProtocol::new()
        .read_document(&mut reader)
        .unwrap()
        .is_none());
}

#[test]
fn text_messages_accept_crlf_and_reject_truncation() {
    let mut reader = Cursor::new(b"() ((1 1))\r\n.\r\n".to_vec());
    let document = TextLinoProtocol::new()
        .read_document(&mut reader)
        .unwrap()
        .unwrap();
    assert_eq!(format_document(&document), "() ((1 1))");

    let mut empty = Cursor::new(b".\n".to_vec());
    assert_eq!(
        TextLinoProtocol::new().read_document(&mut empty).unwrap(),
        Some(Vec::new())
    );

    let mut truncated = Cursor::new(b"() ((1 1))\n".to_vec());
    assert!(matches!(
        TextLinoProtocol::new().read_document(&mut truncated),
        Err(ProtocolError::Malformed(_))
    ));

    let tiny = TextLinoProtocol {
        limits: DecodeLimits {
            max_text_bytes: 8,
            ..DecodeLimits::default()
        },
    };
    let mut long = Cursor::new(format!("{}\n.\n", "a".repeat(100)).into_bytes());
    assert!(matches!(
        tiny.read_document(&mut long),
        Err(ProtocolError::LimitExceeded(_))
    ));
}

#[test]
fn protocols_are_detected_per_message() {
    let document = parse_document("() ((1 1))").unwrap();
    let options = BinaryLinoOptions::default()
        .with_external_references(true)
        .with_sequences(true);
    let mut wire = Vec::new();
    TextLinoProtocol::new()
        .write_document(&mut wire, &document)
        .unwrap();
    BinaryLinoProtocol::with_options(options)
        .write_document(&mut wire, &document)
        .unwrap();
    TextLinoProtocol::new()
        .write_document(&mut wire, &document)
        .unwrap();

    let limits = DecodeLimits::default();
    let mut reader = Cursor::new(wire);
    let formats = std::iter::from_fn(|| read_any_document(&mut reader, &limits).unwrap())
        .map(|(received, format)| {
            assert_eq!(received, document);
            format
        })
        .collect::<Vec<_>>();
    assert_eq!(
        formats,
        vec![
            MessageFormat::Text,
            MessageFormat::Binary(options),
            MessageFormat::Text
        ]
    );
}

#[test]
fn every_short_reference_over_delimiters_round_trips() {
    let alphabet = ['\'', '"', '`', 'a', ' ', '(', ')', ':'];
    let mut stack = vec![String::new()];
    while let Some(reference) = stack.pop() {
        if !reference.is_empty() {
            let text = format_reference(&reference);
            let document = parse_document(&text)
                .unwrap_or_else(|error| panic!("{reference:?} as {text:?}: {error}"));
            assert_eq!(
                document,
                vec![links_notation::LiNo::Ref(reference.clone())],
                "{reference:?} as {text:?}"
            );
        }
        if reference.chars().count() < 5 {
            for character in alphabet {
                stack.push(format!("{reference}{character}"));
            }
        }
    }
}

#[test]
fn deeply_nested_text_parses_in_linear_time() {
    // links-notation before 0.21.3 took exponential time in the nesting depth
    // (link-foundation/links-notation#314), so one short message could stall
    // a server thread. Depth 40 took minutes there.
    let nested = |depth: usize| format!("{}a{}", "(".repeat(depth), ")".repeat(depth));
    let started = std::time::Instant::now();
    let mut link = parse_document(&nested(40)).unwrap().remove(0);
    let mut depth = 1;
    while let links_notation::LiNo::Link { mut values, .. } = link {
        link = values.remove(0);
        depth += 1;
    }
    assert_eq!(depth, 40);
    assert!(matches!(
        parse_document(&nested(100_000)),
        Err(ProtocolError::InvalidLino(_))
    ));
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}
