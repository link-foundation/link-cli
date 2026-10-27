//! Pattern matching for [`GenericQueryProcessor`].
//!
//! Extracted from `query_processor.rs` for issue #96: the file had grown to 994
//! lines and CI warned that it was approaching the 1000-line limit enforced by
//! `rust/scripts/check-file-size.rs`. These are the read-only helpers that
//! decide whether a stored link satisfies a pattern; they carry no state beyond
//! `QueryProcessor`'s tracing flag.

use anyhow::Result;
use std::collections::HashMap;

use crate::link::GenericLink as Link;
use crate::named_type_links::NamedTypeLinks;
use crate::query_address::QueryAddress;
use crate::query_types::Pattern;

use super::GenericQueryProcessor;

impl<T: QueryAddress> GenericQueryProcessor<T> {
    pub(super) fn match_pattern(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        pattern: &Pattern,
        current_solution: &HashMap<String, T>,
    ) -> Result<Vec<HashMap<String, T>>> {
        if pattern.is_leaf() {
            let resolved_index =
                self.resolve_match_id(storage, &pattern.index, current_solution)?;
            let mut matches = Vec::new();
            let mut bindings = 0usize;
            let mut bytes = 0usize;
            for link in storage
                .all_links()
                .into_iter()
                .filter(|link| Self::is_any(resolved_index) || link.index == resolved_index)
            {
                let variable = Self::is_variable(&pattern.index) && link.index != T::from_byte(0);
                let count = usize::from(variable);
                let size = if variable {
                    Self::binding_cost(&pattern.index)
                } else {
                    0
                };
                Self::check_solution_capacity(
                    storage,
                    matches.len() + 1,
                    bindings.saturating_add(count),
                    bytes.saturating_add(size),
                )?;
                let mut assignments = HashMap::new();
                Self::assign_variable(&pattern.index, link.index, &mut assignments);
                matches.push(assignments);
                bindings = bindings.saturating_add(count);
                bytes = bytes.saturating_add(size);
            }
            return Ok(matches);
        }

        let resolved_index = self.resolve_match_id(storage, &pattern.index, current_solution)?;

        if !Self::is_variable(&pattern.index)
            && !Self::is_any(resolved_index)
            && resolved_index != T::from_byte(0)
            && storage.exists(resolved_index)
        {
            let link = storage.get_link(resolved_index).unwrap();
            return self.match_link_against_pattern(storage, pattern, link, current_solution);
        }

        let mut results = Vec::new();
        let mut bindings = 0usize;
        let mut bytes = 0usize;
        for link in storage.all_links() {
            let matched =
                self.match_link_against_pattern(storage, pattern, link, current_solution)?;
            for solution in &matched {
                let (count, size) = Self::solution_cost(solution);
                bindings = bindings.saturating_add(count);
                bytes = bytes.saturating_add(size);
            }
            Self::check_solution_capacity(
                storage,
                results.len().saturating_add(matched.len()),
                bindings,
                bytes,
            )?;
            results.extend(matched);
        }
        Ok(results)
    }

    pub(super) fn match_link_against_pattern(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        pattern: &Pattern,
        link: Link<T>,
        current_solution: &HashMap<String, T>,
    ) -> Result<Vec<HashMap<String, T>>> {
        if !self.check_id_match(storage, &pattern.index, link.index, current_solution)? {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
        let mut bindings = 0usize;
        let mut bytes = 0usize;
        let source_matches = self.recursive_match_subpattern(
            storage,
            pattern.source.as_deref(),
            link.source,
            current_solution,
        )?;

        for source_solution in source_matches {
            let target_matches = self.recursive_match_subpattern(
                storage,
                pattern.target.as_deref(),
                link.target,
                &source_solution,
            )?;
            for mut target_solution in target_matches {
                let (count, size) =
                    Self::extended_solution_cost(&target_solution, &pattern.index, link.index);
                Self::check_solution_capacity(
                    storage,
                    results.len() + 1,
                    bindings.saturating_add(count),
                    bytes.saturating_add(size),
                )?;
                Self::assign_variable(&pattern.index, link.index, &mut target_solution);
                bindings = bindings.saturating_add(count);
                bytes = bytes.saturating_add(size);
                results.push(target_solution);
            }
        }

        Ok(results)
    }

    pub(super) fn recursive_match_subpattern(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        pattern: Option<&Pattern>,
        link_id: T,
        current_solution: &HashMap<String, T>,
    ) -> Result<Vec<HashMap<String, T>>> {
        let Some(pattern) = pattern else {
            let (count, size) = Self::solution_cost(current_solution);
            Self::check_solution_capacity(storage, 1, count, size)?;
            return Ok(vec![current_solution.clone()]);
        };

        if pattern.is_leaf() {
            if self.check_id_match(storage, &pattern.index, link_id, current_solution)? {
                let (count, size) =
                    Self::extended_solution_cost(current_solution, &pattern.index, link_id);
                Self::check_solution_capacity(storage, 1, count, size)?;
                let mut solution = current_solution.clone();
                Self::assign_variable(&pattern.index, link_id, &mut solution);
                return Ok(vec![solution]);
            }
            return Ok(Vec::new());
        }

        let Some(link) = storage.get_link(link_id) else {
            return Ok(Vec::new());
        };

        self.match_link_against_pattern(storage, pattern, link, current_solution)
    }

    fn extended_solution_cost(solution: &HashMap<String, T>, key: &str, id: T) -> (usize, usize) {
        let (count, size) = Self::solution_cost(solution);
        if Self::is_variable(key) && id != T::from_byte(0) && !solution.contains_key(key) {
            (
                count.saturating_add(1),
                size.saturating_add(Self::binding_cost(key)),
            )
        } else {
            (count, size)
        }
    }

    pub(super) fn check_id_match(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        pattern_id: &str,
        candidate_id: T,
        current_solution: &HashMap<String, T>,
    ) -> Result<bool> {
        if pattern_id.is_empty() || pattern_id == "*" {
            return Ok(true);
        }

        if Self::is_variable(pattern_id) {
            return Ok(current_solution
                .get(pattern_id)
                .is_none_or(|existing| *existing == candidate_id));
        }

        if let Ok(parsed) = pattern_id.parse::<T>() {
            return Ok(parsed == candidate_id);
        }

        Ok(storage
            .get_by_name(crate::query_parser::name(pattern_id))?
            .is_some_and(|named_id| named_id == candidate_id))
    }

    pub(super) fn resolve_match_id(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        identifier: &str,
        current_solution: &HashMap<String, T>,
    ) -> Result<T> {
        if identifier.is_empty() || identifier == "*" {
            return Ok(T::MAX);
        }
        if let Some(value) = current_solution.get(identifier) {
            return Ok(*value);
        }
        if Self::is_variable(identifier) {
            return Ok(T::MAX);
        }
        if let Ok(parsed) = identifier.parse::<T>() {
            return Ok(parsed);
        }
        Ok(storage
            .get_by_name(crate::query_parser::name(identifier))?
            .unwrap_or(T::from_byte(0)))
    }

    pub(super) fn matched_links(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        pattern: &Pattern,
        solution: &HashMap<String, T>,
    ) -> Result<Vec<Link<T>>> {
        if pattern.is_leaf() {
            let resolved_index = self.resolve_match_id(storage, &pattern.index, solution)?;
            return Ok(storage
                .all_links()
                .into_iter()
                .filter(|link| Self::is_any(resolved_index) || link.index == resolved_index)
                .collect());
        }

        let mut links = Vec::new();
        for matched_solution in self.match_pattern(storage, pattern, solution)? {
            if let Some(definition) =
                self.resolve_pattern_readonly(storage, pattern, &matched_solution, false)?
            {
                links.extend(self.links_matching_definition(storage, &definition)?);
            }
        }
        Ok(links)
    }

    pub(super) fn solution_is_no_operation(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        solution: &HashMap<String, T>,
        restrictions: &[Pattern],
        substitutions: &[Pattern],
    ) -> Result<bool> {
        let mut restriction_links = self
            .resolve_patterns_readonly(storage, restrictions, solution, false)?
            .into_iter()
            .map(|definition| definition.to_link())
            .collect::<Vec<_>>();
        let mut substitution_links = self
            .resolve_patterns_readonly(storage, substitutions, solution, true)?
            .into_iter()
            .map(|definition| definition.to_link())
            .collect::<Vec<_>>();

        restriction_links.sort_by_key(|link| link.index);
        substitution_links.sort_by_key(|link| link.index);

        Ok(restriction_links == substitution_links)
    }
}
