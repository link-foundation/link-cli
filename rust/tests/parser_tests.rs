//! Tests for the Parser module

use link_cli::Parser;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn test_parse_empty() {
    let parser = Parser::new();
    let result = parser.parse("").unwrap();
    assert!(result.is_empty());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn test_parse_simple_link() {
    let parser = Parser::new();
    let result = parser.parse("(1 2)").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].values_count(), 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn test_parse_nested_link() {
    let parser = Parser::new();
    let result = parser.parse("((1 2) (3 4))").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].values_count(), 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn test_parse_link_with_id() {
    let parser = Parser::new();
    let result = parser.parse("(5: 1 2)").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, Some("5".to_string()));
    assert_eq!(result[0].values_count(), 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn test_parse_variable() {
    let parser = Parser::new();
    let result = parser.parse("($var 1 2)").unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].values.as_ref().unwrap()[0].is_variable());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn test_parse_wildcard() {
    let parser = Parser::new();
    let result = parser.parse("(* 1)").unwrap();
    assert_eq!(result.len(), 1);
    assert!(result[0].values.as_ref().unwrap()[0].is_wildcard());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn test_parse_query_format() {
    let parser = Parser::new();
    // Query format: (( restriction ) ( substitution ))
    let result = parser.parse("(((1 2)) ((3 4)))").unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].values_count(), 2);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn test_parse_links_notation_backtick_unicode_identifier() {
    let parser = Parser::new();
    let result = parser
        .parse("(`ссылка с пробелом`: `источник` `цель`)")
        .unwrap();

    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, Some("ссылка с пробелом".to_string()));

    let values = result[0].values.as_ref().unwrap();
    assert_eq!(values[0].id, Some("источник".to_string()));
    assert_eq!(values[1].id, Some("цель".to_string()));
}
