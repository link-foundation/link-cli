//! The substitution half of [`GenericQueryProcessor`]: turning a matched restriction
//! plus a substitution pattern into the links to write.

use anyhow::Result;
use std::collections::{HashMap, HashSet};

use crate::error::LinkError;
use crate::link::GenericLink as Link;
use crate::named_type_links::NamedTypeLinks;
use crate::query_address::QueryAddress;
use crate::query_processor::GenericQueryProcessor;
use crate::query_types::Pattern;

impl<T: QueryAddress> GenericQueryProcessor<T> {
    pub fn preserve_existing_substitution_parts(
        storage: &mut impl NamedTypeLinks<T>,
        pattern: &Pattern,
        solution: &mut HashMap<String, T>,
        index: T,
        source: &mut T,
        target: &mut T,
        visited_indexes: &mut HashSet<T>,
    ) -> Result<()> {
        if !is_normal_index(index) || !storage.exists(index) {
            return Ok(());
        }

        if !visited_indexes.insert(index) {
            return Ok(());
        }

        let existing_link = storage.get_link(index).ok_or(LinkError::not_found(index))?;

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

fn should_preserve_existing_part<T: QueryAddress>(
    pattern: Option<&Pattern>,
    solution: &HashMap<String, T>,
) -> bool {
    pattern.is_some_and(|pattern| {
        pattern.is_leaf() && is_variable(&pattern.index) && !solution.contains_key(&pattern.index)
    })
}

fn resolved_variable_part<T: QueryAddress>(
    pattern: Option<&Pattern>,
    solution: &HashMap<String, T>,
) -> Option<T> {
    pattern.and_then(|pattern| {
        if pattern.is_leaf() && is_variable(&pattern.index) {
            solution.get(&pattern.index).copied()
        } else {
            None
        }
    })
}

fn assign_variable_from_pattern<T: QueryAddress>(
    pattern: Option<&Pattern>,
    value: T,
    solution: &mut HashMap<String, T>,
) {
    if let Some(pattern) = pattern {
        assign_variable(&pattern.index, value, solution);
    }
}

fn assign_variable<T: QueryAddress>(id: &str, value: T, assignments: &mut HashMap<String, T>) {
    if is_variable(id) {
        assignments.insert(id.to_string(), value);
    }
}

fn can_preserve_existing_part<T: QueryAddress>(
    existing_link: Link<T>,
    part: T,
    visited_indexes: &HashSet<T>,
) -> bool {
    existing_link.is_full_point()
        || existing_link.is_partial_point()
        || !visited_indexes.contains(&part)
}

fn is_variable(identifier: &str) -> bool {
    !identifier.is_empty() && identifier.starts_with('$')
}

fn is_normal_index<T: QueryAddress>(value: T) -> bool {
    value != T::from_byte(0) && value != T::MAX
}
