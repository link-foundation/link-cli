//! Tests for the ChangesSimplifier module

use link_cli::{simplify_changes, Link};

#[test]
fn test_simplify_empty() {
    let changes: Vec<(Link, Link)> = vec![];
    let result = simplify_changes(changes);
    assert!(result.is_empty());
}

#[test]
fn test_simplify_no_op() {
    let link = Link::new(1, 2, 3);
    let changes = vec![(link, link)];
    let result = simplify_changes(changes);
    assert_eq!(result.len(), 1);
    assert_eq!(result[0], (link, link));
}

#[test]
fn test_simplify_chain() {
    let link1 = Link::new(1, 1, 1);
    let link2 = Link::new(1, 2, 2);
    let link3 = Link::new(1, 3, 3);

    let changes = vec![(link1, link2), (link2, link3)];
    let result = simplify_changes(changes);

    assert_eq!(result.len(), 1);
    assert_eq!(result[0], (link1, link3));
}

#[test]
fn test_simplify_with_unchanged() {
    let unchanged = Link::new(2, 2, 2);
    let link1 = Link::new(1, 1, 1);
    let link2 = Link::new(1, 2, 2);

    let changes = vec![(unchanged, unchanged), (link1, link2)];
    let result = simplify_changes(changes);

    assert_eq!(result.len(), 2);
}
