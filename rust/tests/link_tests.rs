//! Tests for the Link module

use link_cli::Link;

#[test]
fn test_link_creation() {
    let link = Link::new(1, 2, 3);
    assert_eq!(link.index, 1);
    assert_eq!(link.source, 2);
    assert_eq!(link.target, 3);
}

#[test]
fn test_link_is_null() {
    let null_link = Link::default();
    assert!(null_link.is_null());

    let non_null_link = Link::new(1, 2, 3);
    assert!(!non_null_link.is_null());
}

#[test]
fn test_link_is_full_point() {
    let point = Link::new(1, 1, 1);
    assert!(point.is_full_point());

    let non_point = Link::new(1, 2, 3);
    assert!(!non_point.is_full_point());
}

#[test]
fn test_link_format() {
    let link = Link::new(1, 2, 3);
    assert_eq!(link.format(), "(1 2 3)");
}
