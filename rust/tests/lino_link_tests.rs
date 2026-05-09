//! Tests for the LinoLink module

use link_cli::LinoLink;

#[test]
fn test_lino_link_new() {
    let link = LinoLink::new(Some("test".to_string()));
    assert_eq!(link.id, Some("test".to_string()));
    assert!(link.values.is_none());
}

#[test]
fn test_lino_link_with_values() {
    let child1 = LinoLink::new(Some("1".to_string()));
    let child2 = LinoLink::new(Some("2".to_string()));
    let link = LinoLink::with_values(Some("parent".to_string()), vec![child1, child2]);
    assert_eq!(link.id, Some("parent".to_string()));
    assert_eq!(link.values_count(), 2);
}

#[test]
fn test_lino_link_is_variable() {
    let var = LinoLink::new(Some("$var".to_string()));
    assert!(var.is_variable());

    let non_var = LinoLink::new(Some("test".to_string()));
    assert!(!non_var.is_variable());
}

#[test]
fn test_lino_link_is_wildcard() {
    let wildcard = LinoLink::new(Some("*".to_string()));
    assert!(wildcard.is_wildcard());

    let non_wildcard = LinoLink::new(Some("test".to_string()));
    assert!(!non_wildcard.is_wildcard());
}

#[test]
fn test_lino_link_is_numeric() {
    let numeric = LinoLink::new(Some("123".to_string()));
    assert!(numeric.is_numeric());

    let non_numeric = LinoLink::new(Some("test".to_string()));
    assert!(!non_numeric.is_numeric());
}
