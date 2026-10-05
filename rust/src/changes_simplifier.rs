//! Simplifies the raw changes of one query to the net change of every link.
//!
//! Corresponds to `ChangesSimplifier.cs` in C#.

use crate::link::Link;
use std::collections::HashMap;

/// Reduces the raw `(before, after)` steps a store reports to one change per
/// link address: from its state before the query to its state after it.
///
/// The store reports every step it takes — a link is emptied to `(i: 0 0)`
/// before it is deleted and created as `(i: 0 0)` before it is set — and the
/// null link `(0: 0 0)` stands for "no link", before a creation and after a
/// deletion. The steps of one address form a chain, so its net change is the
/// `before` of its first step and the `after` of its last:
///
/// - a link created and deleted again within the query is not reported;
/// - a link that ends where it started is reported unchanged, like a link the
///   query only matched.
///
/// The changes are ordered by their `after` state, so deletions come first;
/// changes with equal `after` states keep the order of their first step.
pub fn simplify_changes(changes: Vec<(Link, Link)>) -> Vec<(Link, Link)> {
    let mut net_changes: Vec<(Link, Link)> = Vec::new();
    let mut position_of_address: HashMap<u32, usize> = HashMap::new();
    for (before, after) in changes {
        let address = if before.index != 0 {
            before.index
        } else {
            after.index
        };
        if address == 0 {
            continue;
        }
        match position_of_address.get(&address) {
            Some(&position) => net_changes[position].1 = after,
            None => {
                position_of_address.insert(address, net_changes.len());
                net_changes.push((before, after));
            }
        }
    }
    net_changes.retain(|(before, after)| !(before.is_null() && after.is_null()));
    net_changes.sort_by_key(|(_, after)| (after.index, after.source, after.target));
    net_changes
}
