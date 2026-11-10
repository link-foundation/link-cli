//! Pattern resolution for [`GenericQueryProcessor`].
//!
//! Extracted from `query_processor.rs` for issue #172: the file had grown to
//! 935 lines, past the 900-line warning threshold of
//! `rust/scripts/check-file-size.rs`. These methods turn parsed LiNo patterns
//! into resolved links, either read-only (for previews and matching) or while
//! creating named leaves for substitutions.

use anyhow::Result;
use std::collections::{HashMap, HashSet};

use crate::error::LinkError;
use crate::named_type_links::NamedTypeLinks;
use crate::query_address::QueryAddress;
use crate::query_types::{GenericResolvedLink as ResolvedLink, Pattern};

use super::{Changes, GenericQueryProcessor};

impl<T: QueryAddress> GenericQueryProcessor<T> {
    pub(super) fn resolve_patterns_readonly(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        patterns: &[Pattern],
        solution: &HashMap<String, T>,
        is_substitution: bool,
    ) -> Result<Vec<ResolvedLink<T>>> {
        let mut resolved = Vec::new();
        for pattern in patterns {
            if let Some(link) =
                self.resolve_pattern_readonly(storage, pattern, solution, is_substitution)?
            {
                resolved.push(link);
            }
        }
        Ok(resolved)
    }

    pub(super) fn resolve_pattern_readonly(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        pattern: &Pattern,
        solution: &HashMap<String, T>,
        is_substitution: bool,
    ) -> Result<Option<ResolvedLink<T>>> {
        if pattern.is_leaf() {
            let index = self.resolve_identifier_readonly(
                storage,
                &pattern.index,
                solution,
                if is_substitution {
                    T::from_byte(0)
                } else {
                    T::MAX
                },
            )?;
            return Ok(Some(ResolvedLink::new(index, T::MAX, T::MAX, None)));
        }

        let source_pattern = pattern
            .source
            .as_deref()
            .ok_or_else(|| LinkError::InvalidFormat("Invalid source pattern".to_string()))?;
        let target_pattern = pattern
            .target
            .as_deref()
            .ok_or_else(|| LinkError::InvalidFormat("Invalid target pattern".to_string()))?;

        let source = self
            .resolve_pattern_readonly(storage, source_pattern, solution, is_substitution)?
            .ok_or_else(|| LinkError::InvalidFormat("Invalid source pattern".to_string()))?
            .index;
        let target = self
            .resolve_pattern_readonly(storage, target_pattern, solution, is_substitution)?
            .ok_or_else(|| LinkError::InvalidFormat("Invalid target pattern".to_string()))?
            .index;
        let default_index = if is_substitution {
            T::from_byte(0)
        } else {
            T::MAX
        };
        let index =
            self.resolve_identifier_readonly(storage, &pattern.index, solution, default_index)?;

        Ok(Some(ResolvedLink::new(index, source, target, None)))
    }

    pub(super) fn resolve_identifier_readonly(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        identifier: &str,
        solution: &HashMap<String, T>,
        default_value: T,
    ) -> Result<T> {
        if identifier.is_empty() {
            return Ok(default_value);
        }
        if identifier == "*" {
            return Ok(T::MAX);
        }
        if let Some(value) = solution.get(identifier) {
            return Ok(*value);
        }
        if Self::is_variable(identifier) {
            return Ok(default_value);
        }
        if let Ok(parsed) = identifier.parse::<T>() {
            return Ok(parsed);
        }
        Ok(storage
            .get_by_name(crate::query_parser::name(identifier))?
            .unwrap_or(default_value))
    }

    pub(super) fn resolve_patterns(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        patterns: &[Pattern],
        solution: &HashMap<String, T>,
        is_substitution: bool,
        changes: &mut Changes<T>,
    ) -> Result<Vec<ResolvedLink<T>>> {
        let mut working_solution = solution.clone();
        let mut visited_indexes = HashSet::new();
        let mut resolved = Vec::new();
        for pattern in patterns {
            resolved.push(self.resolve_pattern(
                storage,
                pattern,
                &mut working_solution,
                is_substitution,
                &mut visited_indexes,
                changes,
            )?);
        }
        Ok(resolved)
    }

    pub(super) fn resolve_pattern(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        pattern: &Pattern,
        solution: &mut HashMap<String, T>,
        is_substitution: bool,
        visited_indexes: &mut HashSet<T>,
        changes: &mut Changes<T>,
    ) -> Result<ResolvedLink<T>> {
        if pattern.is_leaf() {
            let index = self.resolve_identifier(
                storage,
                &pattern.index,
                solution,
                if is_substitution {
                    T::from_byte(0)
                } else {
                    T::MAX
                },
                is_substitution,
                changes,
            )?;
            return Ok(ResolvedLink::new(index, T::MAX, T::MAX, None));
        }

        let mut source = self
            .resolve_pattern(
                storage,
                pattern.source.as_deref().unwrap(),
                solution,
                is_substitution,
                visited_indexes,
                changes,
            )?
            .index;
        let mut target = self
            .resolve_pattern(
                storage,
                pattern.target.as_deref().unwrap(),
                solution,
                is_substitution,
                visited_indexes,
                changes,
            )?
            .index;
        let default_index = if is_substitution {
            T::from_byte(0)
        } else {
            T::MAX
        };
        let mut index = self.resolve_identifier(
            storage,
            &pattern.index,
            solution,
            default_index,
            false,
            changes,
        )?;
        let mut name = None;

        if is_substitution
            && !pattern.index.is_empty()
            && !Self::is_numeric_or_wildcard(&pattern.index)
            && !Self::is_variable(&pattern.index)
        {
            name = Some(pattern.index.clone());
            if index == T::from_byte(0) {
                if let Some(existing_id) = storage.search(source, target) {
                    index = existing_id;
                }
            }
        }

        if is_substitution {
            Self::preserve_existing_substitution_parts(
                storage,
                pattern,
                solution,
                index,
                &mut source,
                &mut target,
                visited_indexes,
            )?;
        }

        Ok(ResolvedLink::new(index, source, target, name))
    }

    pub(super) fn resolve_identifier(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        identifier: &str,
        solution: &HashMap<String, T>,
        default_value: T,
        create_named_leaf: bool,
        changes: &mut Changes<T>,
    ) -> Result<T> {
        if identifier.is_empty() {
            return Ok(default_value);
        }
        if identifier == "*" {
            return Ok(T::MAX);
        }
        if let Some(value) = solution.get(identifier) {
            return Ok(*value);
        }
        if Self::is_variable(identifier) {
            return Ok(default_value);
        }
        if let Ok(parsed) = identifier.parse::<T>() {
            return Ok(parsed);
        }
        if create_named_leaf {
            return self.ensure_named_point_link(storage, identifier, changes);
        }
        Ok(storage
            .get_by_name(crate::query_parser::name(identifier))?
            .unwrap_or(default_value))
    }
}
