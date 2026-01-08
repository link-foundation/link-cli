//! Tests for the Parser module

use link_cli::Parser;

#[test]
fn test_parse_empty() {
    let parser = Parser::new();
    let result = parser.parse("").unwrap();
    assert!(result.is_empty());
}

#[test]
fn test_parse_simple_link() {
    let parser = Parser::new();
    let result = parser.parse("(1 2)").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].values_count(), 2);
}

#[test]
fn test_parse_nested_link() {
    let parser = Parser::new();
    let result = parser.parse("((1 2) (3 4))").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].values_count(), 2);
}

#[test]
fn test_parse_link_with_id() {
    let parser = Parser::new();
    let result = parser.parse("(5: 1 2)").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, Some("5".to_string()));
    assert_eq!(result[0].values_count(), 2);
}

#[test]
fn test_parse_variable() {
    let parser = Parser::new();
    let result = parser.parse("($var 1 2)").unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].values.as_ref().unwrap()[0].is_variable());
}

#[test]
fn test_parse_wildcard() {
    let parser = Parser::new();
    let result = parser.parse("(* 1)").unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].values.as_ref().unwrap()[0].is_wildcard());
}

#[test]
fn test_parse_query_format() {
    let parser = Parser::new();
    // Query format: (( restriction ) ( substitution ))
    let result = parser.parse("(((1 2)) ((3 4)))").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].values_count(), 2);
}
