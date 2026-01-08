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

    // **FIX for Issue #26**: Remove duplicate before states by keeping the non-null transitions
    // This handles cases where the same link is reported with multiple different transformations
    let changes = remove_duplicate_before_states(changes);

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

/// Removes problematic duplicate before states that lead to simplification issues.
/// This fixes Issue #26 where multiple transformations from the same before state
/// to conflicting after states (including null states) would cause the simplifier to fail.
///
/// The key insight: If we have multiple transitions from the same before state,
/// and one of them is to a "null" state (0: 0 0), we should prefer the non-null transition
/// as it represents the actual final transformation.
fn remove_duplicate_before_states(changes: Vec<(Link, Link)>) -> Vec<(Link, Link)> {
    // Group changes by their before state
    let mut grouped: HashMap<Link, Vec<(Link, Link)>> = HashMap::new();
    for change in changes {
        grouped.entry(change.0).or_default().push(change);
    }

    let mut result = Vec::new();

    for (_before, changes_for_this_before) in grouped {
        if changes_for_this_before.len() == 1 {
            // No duplicates, keep as is
            result.extend(changes_for_this_before);
        } else {
            // Multiple changes from the same before state
            // Check if any of them is to a null state (0: 0 0)
            let null_link = Link::new(0, 0, 0);
            let has_null_transition = changes_for_this_before
                .iter()
                .any(|(_, after)| *after == null_link);
            let non_null_transitions: Vec<_> = changes_for_this_before
                .iter()
                .filter(|(_, after)| *after != null_link)
                .cloned()
                .collect();

            if has_null_transition && !non_null_transitions.is_empty() {
                // Issue #26 scenario: We have both null and non-null transitions
                // Prefer the non-null transitions as they represent the actual final states
                result.extend(non_null_transitions);
            } else {
                // No null transitions involved, this is a legitimate multiple-branch scenario
                // Keep all transitions
                result.extend(changes_for_this_before);
            }
        }
    }

    result
}
