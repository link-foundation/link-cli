//! The transport around the binary links notation: text framing, protocol
//! detection, connections, limits and errors (issues #104 and #105).

use link_cli::protocol::{
    format_document, parse_document, read_any_document, BinaryError, BinaryLinoOptions,
    BinaryLinoProtocol, DecodeLimits, LinoConnection, LinoProtocol, MessageFormat, ProtocolError,
    ProtocolLimits, TextLinoProtocol, DEFAULT_MAX_TEXT_BYTES,
};
use std::io::{self, Cursor};

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
fn connections_decorate_any_byte_stream() {
    let document = parse_document("() ((1 1))").unwrap();
    let mut connection = LinoConnection::new(Cursor::new(Vec::new()), TextLinoProtocol::new());
    assert_eq!(connection.protocol().max_text_bytes, DEFAULT_MAX_TEXT_BYTES);
    // The cursor sits after what was sent, so no reply follows.
    assert_eq!(
        connection.request(&document).unwrap_err().to_string(),
        "malformed message: connection closed before the reply"
    );
    assert_eq!(connection.into_inner().into_inner(), b"() ((1 1))\n.\n");
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

    // Without the terminator line, with or without the last newline.
    for truncated in ["() ((1 1))\n", "() ((1 1))"] {
        let mut truncated = Cursor::new(truncated.as_bytes());
        assert_eq!(
            TextLinoProtocol::new()
                .read_document(&mut truncated)
                .unwrap_err()
                .to_string(),
            "malformed message: stream ended before the '.' terminator line"
        );
    }

    let tiny = TextLinoProtocol { max_text_bytes: 8 };
    let mut long = Cursor::new(format!("{}\n.\n", "a".repeat(100)).into_bytes());
    assert_eq!(
        tiny.read_document(&mut long).unwrap_err().to_string(),
        "limit exceeded: text message longer than 8 bytes"
    );

    let mut invalid_utf8 = Cursor::new(vec![0xC3, 0x28, b'\n', b'.', b'\n']);
    assert_eq!(
        TextLinoProtocol::new()
            .read_document(&mut invalid_utf8)
            .unwrap_err()
            .to_string(),
        "malformed message: text message is not valid UTF-8"
    );
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

    let limits = ProtocolLimits::default();
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
fn notation_errors_keep_their_kind_and_message() {
    let reset = BinaryError::Io(io::ErrorKind::ConnectionReset.into());
    let io = ProtocolError::from(reset);
    assert!(
        matches!(io, ProtocolError::Io(ref error) if error.kind() == io::ErrorKind::ConnectionReset)
    );
    assert!(matches!(
        ProtocolError::from(BinaryError::Malformed("m".into())),
        ProtocolError::Malformed(detail) if detail == "m"
    ));
    assert!(matches!(
        ProtocolError::from(BinaryError::InvalidLino("i".into())),
        ProtocolError::InvalidLino(detail) if detail == "i"
    ));
    assert!(matches!(
        ProtocolError::from(BinaryError::LimitExceeded("l".into())),
        ProtocolError::LimitExceeded(detail) if detail == "l"
    ));
    assert!(matches!(
        ProtocolError::from(BinaryError::Unencodable("u".into())),
        ProtocolError::Unencodable(detail) if detail == "u"
    ));
}

#[test]
fn protocol_limits_split_binary_and_text_budgets() {
    let limits = ProtocolLimits::default();
    assert_eq!(limits.binary, DecodeLimits::default());
    assert_eq!(limits.max_text_bytes, DEFAULT_MAX_TEXT_BYTES);
    assert_eq!(
        TextLinoProtocol::default().max_text_bytes,
        DEFAULT_MAX_TEXT_BYTES
    );
    let unlimited = ProtocolLimits::unlimited();
    assert_eq!(unlimited.binary, DecodeLimits::unlimited());
    assert_eq!(unlimited.max_text_bytes, usize::MAX);

    // The text limit of a reply protocol comes from the server limits.
    let tiny = ProtocolLimits {
        max_text_bytes: 8,
        ..ProtocolLimits::default()
    };
    let mut long = Cursor::new(format!("{}\n.\n", "a".repeat(100)).into_bytes());
    assert_eq!(
        read_any_document(&mut long, &tiny).unwrap_err().to_string(),
        "limit exceeded: text message longer than 8 bytes"
    );
}

#[test]
fn binary_protocol_refuses_to_write_what_its_peer_would_reject() {
    // links-notation 0.23 checks the limits when encoding too, so a sender
    // finds out before the receiver drops the connection.
    // The text parser stops at the same depth, so build the model directly.
    let mut link = links_notation::LiNo::Ref("x".to_string());
    for _ in 0..70 {
        link = links_notation::LiNo::Link {
            id: None,
            values: vec![links_notation::LiNo::Ref("y".to_string()), link],
        };
    }
    let document = vec![link];
    assert_eq!(
        BinaryLinoProtocol::new()
            .encode(&document)
            .unwrap_err()
            .to_string(),
        "cannot encode: nesting deeper than 64"
    );
    let trusted = BinaryLinoProtocol {
        limits: DecodeLimits::unlimited(),
        ..BinaryLinoProtocol::new()
    };
    assert_eq!(
        trusted.decode(&trusted.encode(&document).unwrap()).unwrap(),
        document
    );
}
