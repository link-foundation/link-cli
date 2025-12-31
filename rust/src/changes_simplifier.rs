//! ChangesSimplifier - Simplifies a list of changes
//!
//! This module provides functionality to simplify a list of changes by
//! identifying chains of transformations.
//! Corresponds to ChangesSimplifier.cs in C#

use crate::link::Link;
use std::collections::{HashMap, HashSet};

/// Simplifies a list of changes by identifying chains of transformations.
///
/// If multiple final states are reachable from the same initial state, returns multiple simplified changes.
/// If a scenario arises where no initial or final states can be identified (no-ops), returns the original transitions as-is.
pub fn simplify_changes(changes: Vec<(Link, Link)>) -> Vec<(Link, Link)> {
    if changes.is_empty() {
        return vec![];
    }

    // First, handle unchanged states directly
    let mut unchanged_states = Vec::new();
    let mut changed_states = Vec::new();

    for (before, after) in changes.iter() {
        if before == after {
            unchanged_states.push((*before, *after));
        } else {
            changed_states.push((*before, *after));
        }
    }

    // Gather all 'Before' links and all 'After' links from changed states
    let before_links: HashSet<Link> = changed_states.iter().map(|(b, _)| *b).collect();
    let after_links: HashSet<Link> = changed_states.iter().map(|(_, a)| *a).collect();

    // Identify initial states: appear as Before but never as After
    let initial_states: Vec<Link> = before_links
        .iter()
        .filter(|b| !after_links.contains(b))
        .copied()
        .collect();

    // Identify final states: appear as After but never as Before
    let final_states: HashSet<Link> = after_links
        .iter()
        .filter(|a| !before_links.contains(a))
        .copied()
        .collect();

    // Build adjacency (Before -> possible list of After links)
    let mut adjacency: HashMap<Link, Vec<Link>> = HashMap::new();
    for (before, after) in changed_states.iter() {
        adjacency.entry(*before).or_default().push(*after);
    }

    // If we have no identified initial states, treat it as a no-op scenario:
    // just return original transitions.
    if initial_states.is_empty() {
        return changes;
    }

    let mut results = Vec::new();

    // Add unchanged states first
    results.extend(unchanged_states);

    // Traverse each initial state with DFS
    for initial in initial_states.iter() {
        let mut stack = vec![*initial];
        let mut visited: HashSet<Link> = HashSet::new();

        while let Some(current) = stack.pop() {
            // Skip if already visited
            if !visited.insert(current) {
                continue;
            }

            let has_next = adjacency.contains_key(&current);
            let next_links = adjacency.get(&current);
            let is_final_or_dead_end = final_states.contains(&current)
                || !has_next
                || next_links.is_none_or(|v| v.is_empty());

            // If final or no further transitions, record (initial -> current)
            if is_final_or_dead_end {
                results.push((*initial, current));
            }

            // Otherwise push neighbors
            if let Some(next_links) = next_links {
                for next in next_links {
                    stack.push(*next);
                }
            }
        }
    }

    // Sort the final results so that items appear in ascending order by their After link.
    // This ensures tests that expect a specific order pass reliably.
    results.sort_by(|a, b| {
        a.1.index
            .cmp(&b.1.index)
            .then_with(|| a.1.source.cmp(&b.1.source))
            .then_with(|| a.1.target.cmp(&b.1.target))
    });

    results
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
