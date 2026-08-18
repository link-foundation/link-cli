//! Pattern matching for [`QueryProcessor`].
//!
//! Extracted from `query_processor.rs` for issue #96: the file had grown to 994
//! lines and CI warned that it was approaching the 1000-line limit enforced by
//! `rust/scripts/check-file-size.rs`. These are the read-only helpers that
//! decide whether a stored link satisfies a pattern; they carry no state beyond
//! `QueryProcessor`'s tracing flag.

use anyhow::Result;
use std::collections::HashMap;

use crate::link::Link;
use crate::named_type_links::NamedTypeLinks;
use crate::query_types::Pattern;

use super::QueryProcessor;

impl QueryProcessor {
    pub(super) fn match_pattern(
        &self,
        storage: &mut impl NamedTypeLinks,
        pattern: &Pattern,
        current_solution: &HashMap<String, u32>,
    ) -> Result<Vec<HashMap<String, u32>>> {
        if pattern.is_leaf() {
            let resolved_index =
                self.resolve_match_id(storage, &pattern.index, current_solution)?;
            return Ok(storage
                .all_links()
                .into_iter()
                .filter(|link| Self::is_any(resolved_index) || link.index == resolved_index)
                .map(|link| {
                    let mut assignments = HashMap::new();
                    Self::assign_variable(&pattern.index, link.index, &mut assignments);
                    assignments
                })
                .collect());
        }

        let resolved_index = self.resolve_match_id(storage, &pattern.index, current_solution)?;

        if !Self::is_variable(&pattern.index)
            && !Self::is_any(resolved_index)
            && resolved_index != 0
            && storage.exists(resolved_index)
        {
            let link = storage.get_link(resolved_index).unwrap();
            return self.match_link_against_pattern(storage, pattern, link, current_solution);
        }

        let mut results = Vec::new();
        for link in storage.all_links() {
            results.extend(self.match_link_against_pattern(
                storage,
                pattern,
                link,
                current_solution,
            )?);
        }
        Ok(results)
    }

    pub(super) fn match_link_against_pattern(
        &self,
        storage: &mut impl NamedTypeLinks,
        pattern: &Pattern,
        link: Link,
        current_solution: &HashMap<String, u32>,
    ) -> Result<Vec<HashMap<String, u32>>> {
        if !self.check_id_match(storage, &pattern.index, link.index, current_solution)? {
            return Ok(Vec::new());
        }

        let mut results = Vec::new();
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
                Self::assign_variable(&pattern.index, link.index, &mut target_solution);
                results.push(target_solution);
            }
        }

        Ok(results)
    }

    pub(super) fn recursive_match_subpattern(
        &self,
        storage: &mut impl NamedTypeLinks,
        pattern: Option<&Pattern>,
        link_id: u32,
        current_solution: &HashMap<String, u32>,
    ) -> Result<Vec<HashMap<String, u32>>> {
        let Some(pattern) = pattern else {
            return Ok(vec![current_solution.clone()]);
        };

        if pattern.is_leaf() {
            if self.check_id_match(storage, &pattern.index, link_id, current_solution)? {
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

    pub(super) fn check_id_match(
        &self,
        storage: &mut impl NamedTypeLinks,
        pattern_id: &str,
        candidate_id: u32,
        current_solution: &HashMap<String, u32>,
    ) -> Result<bool> {
        if pattern_id.is_empty() || pattern_id == "*" {
            return Ok(true);
        }

        if Self::is_variable(pattern_id) {
            return Ok(current_solution
                .get(pattern_id)
                .is_none_or(|existing| *existing == candidate_id));
        }

        if let Ok(parsed) = pattern_id.parse::<u32>() {
            return Ok(parsed == candidate_id);
        }

        Ok(storage
            .get_by_name(pattern_id)?
            .is_some_and(|named_id| named_id == candidate_id))
    }

    pub(super) fn resolve_match_id(
        &self,
        storage: &mut impl NamedTypeLinks,
        identifier: &str,
        current_solution: &HashMap<String, u32>,
    ) -> Result<u32> {
        if identifier.is_empty() || identifier == "*" {
            return Ok(u32::MAX);
        }
        if let Some(value) = current_solution.get(identifier) {
            return Ok(*value);
        }
        if Self::is_variable(identifier) {
            return Ok(u32::MAX);
        }
        if let Ok(parsed) = identifier.parse::<u32>() {
            return Ok(parsed);
        }
        Ok(storage.get_by_name(identifier)?.unwrap_or(0))
    }

    pub(super) fn matched_links(
        &self,
        storage: &mut impl NamedTypeLinks,
        pattern: &Pattern,
        solution: &HashMap<String, u32>,
    ) -> Result<Vec<Link>> {
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
        storage: &mut impl NamedTypeLinks,
        solution: &HashMap<String, u32>,
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
