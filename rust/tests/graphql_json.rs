//! The JSON reader and writer of the GraphQL server, at their edges. The C#
//! port has the same cases in `GraphqlJsonTests.cs`.

use link_cli::graphql::Json;

fn refused(text: &str, message: &str) {
    let error = Json::parse(text).unwrap_err().to_string();
    assert!(error.contains(message), "{text}: {error}");
}

fn round_trip(text: &str, written: &str) {
    assert_eq!(Json::parse(text).unwrap().write(), written, "{text}");
}

#[test]
fn malformed_text_is_refused() {
    for (text, message) in [
        ("", "Invalid JSON at byte 0"),
        ("1 2", "Unexpected text after JSON at byte 2"),
        ("nul", "Invalid JSON at byte 0"),
        ("tru", "Invalid JSON at byte 0"),
        ("fals", "Invalid JSON at byte 0"),
        ("?", "Invalid JSON at byte 0"),
        ("[1", "Invalid JSON array at byte 2"),
        ("[1 2]", "Invalid JSON array at byte 3"),
        ("{1:2}", "Invalid JSON object at byte 1"),
        ("{\"a\" 1}", "Invalid JSON object at byte 5"),
        ("{\"a\":1", "Invalid JSON object at byte 6"),
        ("{\"a\":1 \"b\":2}", "Invalid JSON object at byte 7"),
        ("{\"a\":1,}", "Invalid JSON object at byte 7"),
        ("1.2.3", "Invalid JSON number at byte 0"),
        ("-", "Invalid JSON number at byte 0"),
        ("\"a", "Unterminated JSON string"),
        ("\"a\\", "Unterminated JSON string"),
        ("\"\\q\"", "Invalid JSON escape"),
        ("\"\\u12\"", "Invalid JSON escape"),
        ("\"\\uzzzz\"", "Invalid JSON escape"),
        ("\"\\ud83d\\uzzzz\"", "Invalid JSON escape"),
    ] {
        refused(text, message);
    }
}

#[test]
fn nesting_has_a_limit() {
    let within = format!("{}{}", "[".repeat(128), "]".repeat(128));
    assert!(Json::parse(&within).is_ok());
    let beyond = format!("{}{}", "[".repeat(130), "]".repeat(130));
    refused(&beyond, "nested too deeply");
    let objects = format!("{}1{}", "{\"a\":".repeat(130), "}".repeat(130));
    refused(&objects, "nested too deeply");
}

#[test]
fn values_and_escapes_survive_a_round_trip() {
    round_trip(" [ ] ", "[]");
    round_trip(" { } ", "{}");
    round_trip("[null,true,false]", "[null,true,false]");
    round_trip(r#"{"b":1,"a":[2]}"#, r#"{"b":1,"a":[2]}"#);
    round_trip("1.5", "1.5");
    round_trip("-2", "-2");
    round_trip(
        r#""\"\\\/\n\r\t\b\f\u0041""#,
        r#""\"\\/\n\r\t\u0008\u000cA""#,
    );
    round_trip(r#""\u0001""#, r#""\u0001""#);
    round_trip(r#""\ud83d\ude00""#, "\"😀\"");
    // A lone surrogate has no character of its own.
    round_trip(r#""\ud83d""#, "\"\u{FFFD}\"");
    round_trip(r#""\ude00""#, "\"\u{FFFD}\"");
}

#[test]
fn numbers_read_as_what_they_are() {
    let whole = Json::Float(3.0);
    assert_eq!(whole.as_i64(), Some(3));
    assert_eq!(whole.as_f64(), Some(3.0));
    assert_eq!(Json::Float(3.5).as_i64(), None);
    assert_eq!(Json::Int(3).as_f64(), Some(3.0));
    assert_eq!(Json::Null.as_f64(), None);
    assert_eq!(Json::Null.as_i64(), None);
    assert_eq!(Json::Int(1).as_str(), None);
    assert!(Json::Int(1).get("a").is_none());
    assert_eq!(Json::Int(1).as_list(), vec![&Json::Int(1)]);
    assert!(Json::Null.as_list().is_empty());
    assert_eq!(Json::Float(1.5).write(), "1.5");
    assert_eq!(Json::Float(f64::NAN).write(), "null");
    assert_eq!(Json::Float(f64::INFINITY).write(), "null");
}

#[test]
fn control_characters_are_escaped_when_written() {
    assert_eq!(
        Json::str("\u{0}\u{1f}\"\\\n").write(),
        r#""\u0000\u001f\"\\\n""#
    );
    assert_eq!(
        Json::parse("99999999999999999999").unwrap().as_f64(),
        Some(1e20)
    );
}
