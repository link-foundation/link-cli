use anyhow::Result;
use std::collections::{HashMap, HashSet};

use crate::error::LinkError;
use crate::link::Link;
use crate::named_type_links::NamedTypeLinks;
use crate::query_processor::QueryProcessor;
use crate::query_types::Pattern;

impl QueryProcessor {
    pub(crate) fn preserve_existing_substitution_parts(
        storage: &mut impl NamedTypeLinks,
        pattern: &Pattern,
        solution: &mut HashMap<String, u32>,
        index: u32,
        source: &mut u32,
        target: &mut u32,
        visited_indexes: &mut HashSet<u32>,
    ) -> Result<()> {
        if !is_normal_index(index) || !storage.exists(index) {
            return Ok(());
        }

        if !visited_indexes.insert(index) {
            return Ok(());
        }

        let existing_link = storage.get_link(index).ok_or(LinkError::NotFound(index))?;

        if should_preserve_existing_part(pattern.source.as_deref(), solution)
            && can_preserve_existing_part(existing_link, existing_link.source, visited_indexes)
        {
            *source = existing_link.source;
            assign_variable_from_pattern(pattern.source.as_deref(), *source, solution);
        } else if let Some(bound_source) =
            resolved_variable_part(pattern.source.as_deref(), solution)
        {
            *source = bound_source;
        }

        if should_preserve_existing_part(pattern.target.as_deref(), solution)
            && can_preserve_existing_part(existing_link, existing_link.target, visited_indexes)
        {
            *target = existing_link.target;
            assign_variable_from_pattern(pattern.target.as_deref(), *target, solution);
        } else if let Some(bound_target) =
            resolved_variable_part(pattern.target.as_deref(), solution)
        {
            *target = bound_target;
        }

        visited_indexes.remove(&index);
        Ok(())
    }
}

fn should_preserve_existing_part(
    pattern: Option<&Pattern>,
    solution: &HashMap<String, u32>,
) -> bool {
    pattern.is_some_and(|pattern| {
        pattern.is_leaf() && is_variable(&pattern.index) && !solution.contains_key(&pattern.index)
    })
}

fn resolved_variable_part(
    pattern: Option<&Pattern>,
    solution: &HashMap<String, u32>,
) -> Option<u32> {
    pattern.and_then(|pattern| {
        if pattern.is_leaf() && is_variable(&pattern.index) {
            solution.get(&pattern.index).copied()
        } else {
            None
        }
    })
}

fn assign_variable_from_pattern(
    pattern: Option<&Pattern>,
    value: u32,
    solution: &mut HashMap<String, u32>,
) {
    if let Some(pattern) = pattern {
        assign_variable(&pattern.index, value, solution);
    }
}

fn assign_variable(id: &str, value: u32, assignments: &mut HashMap<String, u32>) {
    if is_variable(id) && value != 0 {
        assignments.insert(id.to_string(), value);
    }
}

fn can_preserve_existing_part(
    existing_link: Link,
    part: u32,
    visited_indexes: &HashSet<u32>,
) -> bool {
    existing_link.is_full_point()
        || existing_link.is_partial_point()
        || !visited_indexes.contains(&part)
}

fn is_variable(identifier: &str) -> bool {
    !identifier.is_empty() && identifier.starts_with('$')
}

fn is_normal_index(value: u32) -> bool {
    value != 0 && value != u32::MAX
}
