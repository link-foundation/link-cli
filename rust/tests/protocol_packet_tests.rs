//! The binary links notation as link-cli uses it (issues #104 and #105).
//!
//! The binary links notation comes from links-notation (issue #104); these
//! tests pin the behavior link-cli relies on, so an upgrade that changes it
//! fails here.

#[path = "common/options.rs"]
mod common;

use common::binary_options;
use link_cli::external_reference;
use link_cli::protocol::packet::{
    address_tier, decode_external, encode_external, external_capacity, internal_capacity,
};
use link_cli::protocol::{
    decode_document, encode_document, format_document, format_reference, parse_document,
    ArityRange, BinaryError, BinaryLinoOptions, BinaryLinoProtocol, DecodeLimits, LinksPacket,
    Reference, Section,
};

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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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
    let empty_named_link = links_notation::LiNo::Link {
        id: Some("a".to_string()),
        values: vec![],
    };
    // Only a binary message carries an id without values; links-notation reads its text back as the reference.
    assert_eq!(format_document(&[empty_named_link]), "(a:)");
    assert_eq!(
        parse_document("(a:)").unwrap(),
        vec![links_notation::LiNo::Ref("a".to_string())]
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn references_are_formatted_losslessly() {
    assert_eq!(format_reference("plain"), "plain");
    assert_eq!(format_reference("$x"), "$x");
    assert_eq!(format_reference(""), "~1{}");
    assert_eq!(format_reference("\n"), "~1{0a}");
    assert_eq!(format_reference("a b"), "'a b'");
    assert_eq!(format_reference("it's"), "\"it's\"");
    assert_eq!(format_reference("'\""), "`'\"`");
    assert_eq!(format_reference("'\"`"), "\"\"\"'\"`\"\"\"");
    assert_eq!(format_reference("a\r\nb"), "~1{610d0a62}");
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
        "a\r\nb",
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn legacy_quoted_references_match_lossless_literals() {
    for (legacy, literal) in [("''", "~1{}"), ("'a\nb'", "~1{610a62}")] {
        assert_eq!(
            parse_document(legacy).unwrap(),
            parse_document(literal).unwrap()
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn width_tiers_follow_the_number_of_links() {
    assert_eq!(address_tier(0, false).unwrap(), 1);
    assert_eq!(address_tier(255, false).unwrap(), 1);
    assert_eq!(address_tier(256, false).unwrap(), 2);
    assert_eq!(address_tier(65_535, false).unwrap(), 2);
    assert_eq!(address_tier(65_536, false).unwrap(), 4);
    assert_eq!(address_tier(u64::from(u32::MAX), false).unwrap(), 4);
    assert_eq!(address_tier(u64::from(u32::MAX) + 1, false).unwrap(), 8);
    // External references take the top bit, halving every range.
    assert_eq!(address_tier(127, true).unwrap(), 1);
    assert_eq!(address_tier(128, true).unwrap(), 2);
    assert_eq!(address_tier(32_767, true).unwrap(), 2);
    assert_eq!(address_tier(32_768, true).unwrap(), 4);
    assert_eq!(address_tier(u64::MAX, false).unwrap(), 8);
    // No width holds an internal address in the external half.
    assert!(matches!(
        address_tier(1 << 63, true),
        Err(BinaryError::Unencodable(_))
    ));
    assert_eq!(internal_capacity(8, false), u64::MAX);
    assert_eq!(internal_capacity(8, true), i64::MAX as u64);
    assert_eq!(external_capacity(1), 127);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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
            Err(BinaryError::Unencodable(_))
        ));
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn packing_rejects_links_it_cannot_lay_out() {
    let unencodable = |links: &[(u64, Vec<Reference>)], external_references: bool| {
        assert!(
            matches!(
                LinksPacket::pack(external_references, links, true),
                Err(BinaryError::Unencodable(_))
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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
            matches!(packet.to_bytes(), Err(BinaryError::Unencodable(_))),
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
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

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn deeply_nested_text_parses_in_linear_time() {
    // links-notation before 0.21.3 took exponential time in the nesting depth
    // (link-foundation/links-notation#314), so one short message could stall
    // a server thread. Depth 40 took minutes there.
    let nested = |depth: usize| format!("{}a{}", "(".repeat(depth), ")".repeat(depth));
    #[cfg(not(target_arch = "wasm32"))]
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
        Err(BinaryError::InvalidLino(_))
    ));
    // WASM uses the runner's finite test deadline; std::time::Instant
    // is unavailable on wasm32-unknown-unknown. Keep the same input checks.
    #[cfg(not(target_arch = "wasm32"))]
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}
