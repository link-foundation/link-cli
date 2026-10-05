//! Binary and text LiNo protocol codecs (issue #105).

mod common;

use common::binary_options;
use link_cli::external_reference;
use link_cli::protocol::packet::{
    address_tier, decode_external, encode_external, external_capacity, internal_capacity,
};
use link_cli::protocol::{
    decode_document, encode_document, format_document, format_reference, parse_document,
    read_any_document, ArityRange, BinaryLinoOptions, BinaryLinoProtocol, DecodeLimits,
    LinksPacket, LinoProtocol, MessageFormat, ProtocolError, Reference, Section, TextLinoProtocol,
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

fn references_option(field: &str) -> bool {
    match field {
        "plain" => false,
        "external" => true,
        other => panic!("unknown references option {other:?}"),
    }
}

fn widths_option(field: &str) -> bool {
    match field {
        "uniform" => false,
        "packed" => true,
        other => panic!("unknown widths option {other:?}"),
    }
}

/// `address:reference reference…` separated by `;`, `#v` being external.
fn parse_links(text: &str) -> Vec<(u64, Vec<Reference>)> {
    text.split(';')
        .map(|link| {
            let (address, references) = link.split_once(':').unwrap();
            let references = references
                .split(' ')
                .map(|reference| match reference.strip_prefix('#') {
                    Some(value) => Reference::External(value.parse().unwrap()),
                    None => Reference::Internal(reference.parse().unwrap()),
                })
                .collect();
            (address.parse().unwrap(), references)
        })
        .collect()
}

/// The golden vectors shared with the C# test suite: both implementations
/// must write and read exactly these bytes.
fn golden_vectors() -> impl Iterator<Item = Vec<&'static str>> {
    include_str!("../../docs/protocol/binary-links-notation-vectors.txt")
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.split('\t').collect())
}

#[test]
fn golden_documents_are_stable() {
    let mut checked = 0;
    for fields in golden_vectors().filter(|fields| fields[0] == "document") {
        let [_, text, references, arity, widths, expected] = fields[..] else {
            panic!("bad document vector {fields:?}");
        };
        let text = text.replace("\\n", "\n");
        let options = BinaryLinoOptions {
            external_references: references_option(references),
            arity: arity.parse().unwrap(),
            packed_widths: widths_option(widths),
        };
        let document = parse_document(&text).unwrap();
        let binary = BinaryLinoProtocol::with_options(options);
        assert_eq!(
            hex(&binary.encode(&document).unwrap()),
            expected,
            "{text:?} with {options:?}"
        );
        assert_eq!(binary.decode(&unhex(expected)).unwrap(), document);
        checked += 1;
    }
    assert!(checked >= 80, "only {checked} document vectors");
}

#[test]
fn golden_packets_are_stable() {
    let mut checked = 0;
    for fields in golden_vectors().filter(|fields| fields[0] == "links") {
        let [_, links, references, widths, expected] = fields[..] else {
            panic!("bad links vector {fields:?}");
        };
        let links = parse_links(links);
        let packet =
            LinksPacket::pack(references_option(references), &links, widths_option(widths))
                .unwrap();
        assert_eq!(hex(&packet.to_bytes().unwrap()), expected, "{links:?}");
        let read = LinksPacket::from_bytes(&unhex(expected), &DecodeLimits::default()).unwrap();
        assert_eq!(read, packet);
        let read_links = read
            .links()
            .map(|(address, link)| (address, link.to_vec()))
            .collect::<Vec<_>>();
        assert_eq!(read_links, links);
        checked += 1;
    }
    assert!(checked >= 10, "only {checked} links vectors");
}

#[test]
fn every_option_set_round_trips_the_corpus() {
    for text in CORPUS {
        let document = parse_document(text).unwrap();
        for options in binary_options() {
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

fn section_widths(packet: &LinksPacket) -> Vec<u8> {
    packet
        .sections
        .iter()
        .map(|section| section.width)
        .collect()
}

#[test]
fn uniform_widths_grow_past_256_addresses_and_packed_widths_stay_small() {
    let document = parse_document(&many_links(3)).unwrap();
    let small = encode_document(&document, BinaryLinoOptions::default()).unwrap();
    assert_eq!(section_widths(&small), [1]);

    let document = parse_document(&many_links(400)).unwrap();
    let uniform = encode_document(&document, BinaryLinoOptions::default()).unwrap();
    assert_eq!(section_widths(&uniform), [2]);
    let uniform_bytes = uniform.to_bytes().unwrap();
    let header = 1 + 2; // header byte + two-byte LEB128 count
    assert_eq!(
        uniform_bytes.len() as u64,
        header + uniform.link_count() * 2 * 2
    );

    let packed = encode_document(
        &document,
        BinaryLinoOptions::default().with_packed_widths(true),
    )
    .unwrap();
    assert_eq!(section_widths(&packed)[0], 1);
    assert!(section_widths(&packed).contains(&2));
    let packed_bytes = packed.to_bytes().unwrap();
    assert!(packed_bytes.len() < uniform_bytes.len());
    let limits = DecodeLimits::default();
    for bytes in [uniform_bytes, packed_bytes] {
        let packet = LinksPacket::from_bytes(&bytes, &limits).unwrap();
        assert_eq!(decode_document(&packet, &limits).unwrap(), document);
    }
}

#[test]
fn packed_widths_never_take_more_bytes_than_uniform_ones() {
    let mut corpus = CORPUS
        .iter()
        .map(|text| text.to_string())
        .collect::<Vec<_>>();
    corpus.push(many_links(200));
    corpus.push("(1000 1)".into());
    for text in &corpus {
        let document = parse_document(text).unwrap();
        for options in binary_options()
            .into_iter()
            .filter(|options| options.packed_widths)
        {
            let packed = encode_document(&document, options).unwrap();
            let uniform = encode_document(&document, options.with_packed_widths(false)).unwrap();
            assert!(
                packed.to_bytes().unwrap().len() <= uniform.to_bytes().unwrap().len(),
                "{text:?} with {options:?}"
            );
        }
    }
}

#[test]
fn large_external_values_widen_only_their_section() {
    let options = BinaryLinoOptions::default()
        .with_external_references(true)
        .with_packed_widths(true);
    let packet = encode_document(&parse_document("(100 1)").unwrap(), options).unwrap();
    assert_eq!(section_widths(&packet), [1]);
    let packet = encode_document(&parse_document("(70000 1)").unwrap(), options).unwrap();
    assert_eq!(section_widths(&packet), [4, 1]);
    // Beyond 63 bits the number falls back to in-band unary links.
    let packet =
        encode_document(&parse_document("18446744073709551615").unwrap(), options).unwrap();
    assert!(packet.link_count() > 60);
}

#[test]
fn arity_options_choose_doublets_triplets_or_any_length() {
    let document = parse_document("(a b c)\n(d e)").unwrap();
    let arities = |arity: ArityRange| {
        let options = BinaryLinoOptions::default()
            .with_external_references(true)
            .with_arity(arity);
        let packet = encode_document(&document, options).unwrap();
        assert_eq!(
            BinaryLinoProtocol::new()
                .decode(&packet.to_bytes().unwrap())
                .unwrap(),
            document
        );
        packet
            .links()
            .map(|(_, link)| link.len())
            .collect::<Vec<_>>()
    };
    assert!(arities(ArityRange::DOUBLETS)
        .iter()
        .all(|&length| length == 2));
    // `(a b c)` is one triplet; the strings `(String code point)` stay doublets.
    let triplets = arities(ArityRange::between(2, 3));
    assert_eq!(triplets.iter().filter(|&&length| length == 3).count(), 1);
    assert!(triplets.iter().all(|&length| length == 2 || length == 3));
    assert_eq!(arities(ArityRange::at_least(1)), triplets);
    // Only any length turns a one-item list, here the root, into one link.
    let single = parse_document("a").unwrap();
    let options = BinaryLinoOptions::default().with_external_references(true);
    let lengths = |options: BinaryLinoOptions| {
        let packet = encode_document(&single, options).unwrap();
        packet
            .links()
            .map(|(_, link)| link.len())
            .collect::<Vec<_>>()
    };
    assert_eq!(lengths(options), [2, 2, 2, 2]);
    assert_eq!(
        lengths(options.with_arity(ArityRange::at_least(1))),
        [2, 2, 1]
    );
    // Long lists only become single links when the range allows their length.
    let long = parse_document("(a b c d e f g h)").unwrap();
    for (arity, single_link) in [
        (ArityRange::between(2, 3), false),
        (ArityRange::between(2, 8), true),
        (ArityRange::at_least(2), true),
    ] {
        let options = BinaryLinoOptions::default()
            .with_external_references(true)
            .with_arity(arity);
        let packet = encode_document(&long, options).unwrap();
        let longest = packet.links().map(|(_, link)| link.len()).max().unwrap();
        assert_eq!(longest == 8, single_link, "{arity}");
    }
}

#[test]
fn arities_without_doublets_are_unencodable() {
    for arity in [
        ArityRange::exactly(3),
        ArityRange::exactly(1),
        ArityRange::at_least(3),
    ] {
        assert!(matches!(
            encode_document(
                &parse_document("(a b)").unwrap(),
                BinaryLinoOptions::default().with_arity(arity)
            ),
            Err(ProtocolError::Unencodable(_))
        ));
    }
}

#[test]
fn arity_ranges_parse_and_display() {
    for (text, range) in [
        ("2", ArityRange::DOUBLETS),
        ("2..3", ArityRange::between(2, 3)),
        ("1..", ArityRange::at_least(1)),
        ("3..3", ArityRange::exactly(3)),
    ] {
        assert_eq!(text.parse::<ArityRange>(), Ok(range));
    }
    assert_eq!(ArityRange::between(3, 3).to_string(), "3");
    assert_eq!(ArityRange::between(2, 3).to_string(), "2..3");
    assert_eq!(ArityRange::at_least(1).to_string(), "1..");
    assert_eq!(ArityRange::default(), ArityRange::DOUBLETS);
    for invalid in ["", "0", "0..2", "3..2", "a", "1..b", "..3", "-1", " 2"] {
        assert!(invalid.parse::<ArityRange>().is_err(), "{invalid:?}");
    }
    assert!(ArityRange::at_least(2).contains(1_000));
    assert!(!ArityRange::between(2, 3).contains(4));
    assert!(ArityRange::exactly(2).is_fixed() && !ArityRange::at_least(2).is_fixed());
}

#[test]
fn reply_options_follow_the_received_packet() {
    let document = parse_document("(a b c) 70000").unwrap();
    let options = BinaryLinoOptions::default()
        .with_external_references(true)
        .with_arity(ArityRange::between(2, 3))
        .with_packed_widths(true);
    let packet = encode_document(&document, options).unwrap();
    assert_eq!(BinaryLinoOptions::of_packet(&packet), options);
    let packet = encode_document(&document, BinaryLinoOptions::default()).unwrap();
    assert_eq!(
        BinaryLinoOptions::of_packet(&packet),
        BinaryLinoOptions::default()
    );
}

#[test]
fn hand_built_packets_decode() {
    // One doublet (One One) is 2^1 in unary, so `(Number 7)` is the number 2.
    let links = [(1, 1), (2, 6), (7, 0), (4, 8)]
        .map(|(source, target)| vec![Reference::Internal(source), Reference::Internal(target)]);
    let packet = LinksPacket {
        external_references: false,
        sections: vec![Section {
            gap: 5,
            arity: ArityRange::DOUBLETS,
            width: 1,
            links: links.to_vec(),
        }],
    };
    let bytes = packet.to_bytes().unwrap();
    assert_eq!(hex(&bytes), "10 04 01 01 02 06 07 00 04 08");
    let protocol = BinaryLinoProtocol::new();
    assert_eq!(format_document(&protocol.decode(&bytes).unwrap()), "2");

    // The same links as a variable section of width 2 use the explicit layout.
    let packet = LinksPacket {
        sections: vec![Section {
            arity: ArityRange::at_least(1),
            width: 2,
            ..packet.sections[0].clone()
        }],
        ..packet
    };
    let bytes = packet.to_bytes().unwrap();
    assert_eq!(
        hex(&bytes),
        "12 01 1d 05 00 04 01 01 00 01 00 01 02 00 06 00 01 07 00 00 00 01 04 00 08 00"
    );
    assert_eq!(format_document(&protocol.decode(&bytes).unwrap()), "2");
}

#[test]
fn packing_rejects_links_it_cannot_lay_out() {
    let unencodable = |links: &[(u64, Vec<Reference>)], external_references: bool| {
        assert!(
            matches!(
                LinksPacket::pack(external_references, links, true),
                Err(ProtocolError::Unencodable(_))
            ),
            "{links:?}"
        );
    };
    let link = |references: &[u64]| {
        references
            .iter()
            .copied()
            .map(Reference::Internal)
            .collect()
    };
    unencodable(&[(0, link(&[1]))], false); // address 0 is null
    unencodable(&[(2, link(&[1])), (1, link(&[1]))], false); // descending
    unencodable(&[(2, link(&[1])), (2, link(&[1]))], false); // repeated
    unencodable(&[(1, Vec::new())], false); // no references
    unencodable(&[(1, vec![Reference::External(1)])], false);
    unencodable(&[(1, vec![Reference::External(1 << 63)])], true);
    unencodable(&[(1, link(&[1 << 63]))], true); // the top bit marks externals
    assert!(LinksPacket::pack(false, &[(1, link(&[u64::MAX]))], true).is_ok());
}

#[test]
fn writing_rejects_sections_that_do_not_hold_their_links() {
    let section = |arity: ArityRange, width: u8, links: Vec<Vec<Reference>>| LinksPacket {
        external_references: false,
        sections: vec![Section {
            gap: 0,
            arity,
            width,
            links,
        }],
    };
    let unencodable = |packet: LinksPacket| {
        assert!(
            matches!(packet.to_bytes(), Err(ProtocolError::Unencodable(_))),
            "{packet:?}"
        );
    };
    let pair = vec![Reference::Internal(1), Reference::Internal(1)];
    unencodable(section(ArityRange::exactly(3), 1, vec![pair.clone()]));
    unencodable(section(ArityRange::exactly(0), 1, Vec::new()));
    unencodable(section(ArityRange::between(3, 2), 1, Vec::new()));
    unencodable(section(ArityRange::exactly(1 << 62), 1, Vec::new()));
    unencodable(section(ArityRange::DOUBLETS, 3, vec![pair.clone()]));
    unencodable(section(
        ArityRange::DOUBLETS,
        1,
        vec![vec![Reference::Internal(256), Reference::NULL]],
    ));
    let mut gaps = section(ArityRange::DOUBLETS, 1, vec![pair]);
    gaps.sections[0].gap = u64::MAX;
    unencodable(gaps);
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
    malformed(&[0x16, 0x00]); // the explicit layout keeps the width bits clear
    malformed(&[0x12, 0x01, 0x04, 0x05, 0x00]); // arity 0
    malformed(&[0x12, 0x01, 0x18, 0x01, 0x01, 0x05, 0x00]); // length 6 in arity 1..2
    malformed(&[
        0x12, 0x01, 0xf8, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x0f, 0xff, 0xff, 0xff,
        0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, 0x00,
    ]); // arity overflows
    malformed(&[
        0x12, 0x02, 0x04, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x01, 0x01, 0x20,
        0x01,
    ]); // addresses overflow
    malformed(&[0x12, 0x01, 0x24, 0x06, 0x01, 0x01, 0x01]); // LiNo links start at 6, not 7
    malformed(&[
        0x12, 0x02, 0x24, 0x05, 0x01, 0x24, 0x01, 0x01, 0x01, 0x01, 0x04, 0x06,
    ]); // a hole
        // (String (0x110000)) is not a valid code point.
    let links = [
        vec![Reference::External(0x11_0000), Reference::NULL],
        vec![Reference::Internal(3), Reference::Internal(6)],
        vec![Reference::Internal(7), Reference::NULL],
        vec![Reference::Internal(4), Reference::Internal(8)],
    ];
    let links = (6..).zip(links).collect::<Vec<_>>();
    malformed(
        &LinksPacket::pack(true, &links, false)
            .unwrap()
            .to_bytes()
            .unwrap(),
    );
}

#[test]
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
    limited(&bomb.to_bytes().unwrap(), small_budget);

    let deep = format!("{}x{}", "(y ".repeat(40), ")".repeat(40));
    let bytes = BinaryLinoProtocol::new()
        .encode(&parse_document(&deep).unwrap())
        .unwrap();
    let shallow = DecodeLimits {
        max_depth: 10,
        ..DecodeLimits::default()
    };
    limited(&bytes, shallow);
    assert!(DecodeLimits::unlimited().max_links == u64::MAX);
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
    let options = BinaryLinoOptions::default().with_external_references(true);
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
