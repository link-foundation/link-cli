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

#[test]
fn test_simplify_issue26_update_operation() {
    // This represents the scenario described in GitHub issue #26
    // Where links (1: 1 2) and (2: 2 1) are being updated to swap source and target
    // The issue was that intermediate steps were being shown instead of the final transformation
    let changes = vec![
        // Step 1: Link (1: 1 2) is first deleted (becomes null/empty)
        (Link::new(1, 1, 2), Link::new(0, 0, 0)),
        // Step 2: New link (1: 2 1) is created (swapped source and target)
        (Link::new(0, 0, 0), Link::new(1, 2, 1)),
        // Step 3: Link (2: 2 1) is directly updated to (2: 1 2)
        (Link::new(2, 2, 1), Link::new(2, 1, 2)),
    ];

    let result = simplify_changes(changes);

    // Expected - The simplification should show only the initial-to-final transformations
    assert_eq!(result.len(), 2);
    assert!(result.contains(&(Link::new(1, 1, 2), Link::new(1, 2, 1))));
    assert!(result.contains(&(Link::new(2, 2, 1), Link::new(2, 1, 2))));
}

#[test]
fn test_simplify_issue26_alternative_scenario() {
    // This tests the scenario where the same before state has multiple transitions:
    // one to null (deletion) and one to a non-null state (update)
    let changes = vec![
        // Let's say we get these individual changes that don't form a proper chain
        (Link::new(1, 1, 2), Link::new(0, 0, 0)), // delete
        (Link::new(1, 1, 2), Link::new(1, 2, 1)), // direct update (this is what should be kept)
        (Link::new(2, 2, 1), Link::new(2, 1, 2)), // direct update
    ];

    let result = simplify_changes(changes);

    // After the fix, we should prefer the non-null transition
    // So we should get only 2 changes: the two direct updates
    assert_eq!(result.len(), 2);
    assert!(result.contains(&(Link::new(1, 1, 2), Link::new(1, 2, 1))));
    assert!(result.contains(&(Link::new(2, 2, 1), Link::new(2, 1, 2))));
}

#[test]
fn test_simplify_multiple_branches_from_same_initial() {
    // Original transitions (Before -> After):
    // (0: 0 0) -> (1: 0 0)
    // (1: 0 0) -> (1: 1 1)
    // (0: 0 0) -> (2: 0 0)
    // (2: 0 0) -> (2: 2 2)
    let changes = vec![
        (Link::new(0, 0, 0), Link::new(1, 0, 0)),
        (Link::new(1, 0, 0), Link::new(1, 1, 1)),
        (Link::new(0, 0, 0), Link::new(2, 0, 0)),
        (Link::new(2, 0, 0), Link::new(2, 2, 2)),
    ];

    let result = simplify_changes(changes);

    // Expected final transitions (After simplification):
    // (0: 0 0) -> (1: 1 1)
    // (0: 0 0) -> (2: 2 2)
    assert_eq!(result.len(), 2);
    assert!(result.contains(&(Link::new(0, 0, 0), Link::new(1, 1, 1))));
    assert!(result.contains(&(Link::new(0, 0, 0), Link::new(2, 2, 2))));
}

#[test]
fn test_simplify_specific_example_removes_intermediate_states() {
    // (1: 2 1) ↦ (1: 0 0)
    // (2: 1 2) ↦ (2: 0 0)
    // (2: 0 0) ↦ (0: 0 0)
    // (1: 0 0) ↦ (0: 0 0)
    let changes = vec![
        (Link::new(1, 2, 1), Link::new(1, 0, 0)),
        (Link::new(2, 1, 2), Link::new(2, 0, 0)),
        (Link::new(2, 0, 0), Link::new(0, 0, 0)),
        (Link::new(1, 0, 0), Link::new(0, 0, 0)),
    ];

    let result = simplify_changes(changes);

    // Expected simplified changes:
    // (1: 2 1) ↦ (0: 0 0)
    // (2: 1 2) ↦ (0: 0 0)
    assert_eq!(result.len(), 2);
    assert!(result.contains(&(Link::new(1, 2, 1), Link::new(0, 0, 0))));
    assert!(result.contains(&(Link::new(2, 1, 2), Link::new(0, 0, 0))));
}

#[test]
fn test_simplify_keeps_unchanged_states() {
    // (1: 1 2) ↦ (1: 2 1)
    // (2: 2 2) ↦ (2: 2 2) (unchanged)
    let changes = vec![
        (Link::new(1, 1, 2), Link::new(1, 2, 1)),
        (Link::new(2, 2, 2), Link::new(2, 2, 2)),
    ];

    let result = simplify_changes(changes);

    // Expected simplified changes still have (2: 2 2):
    assert_eq!(result.len(), 2);
    assert!(result.contains(&(Link::new(1, 1, 2), Link::new(1, 2, 1))));
    assert!(result.contains(&(Link::new(2, 2, 2), Link::new(2, 2, 2))));
}
