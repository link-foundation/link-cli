//! QueryProcessor - Handles LiNo query parsing and execution
//!
//! This module provides the QueryProcessor for processing LiNo queries.
//! Corresponds to BasicQueryProcessor, MixedQueryProcessor, and AdvancedMixedQueryProcessor in C#

use anyhow::Result;
use std::collections::HashMap;

use crate::changes_simplifier::simplify_changes;
use crate::error::LinkError;
use crate::link::Link;
use crate::link_reference_validator::LinkReferenceValidator;
use crate::link_storage::LinkStorage;
use crate::lino_link::LinoLink;
use crate::parser::Parser;

/// QueryProcessor handles LiNo query parsing and execution
/// Corresponds to AdvancedMixedQueryProcessor in C#
pub struct QueryProcessor {
    trace: bool,
    auto_create_missing_references: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct Pattern {
    index: String,
    source: Option<Box<Pattern>>,
    target: Option<Box<Pattern>>,
}

impl Pattern {
    fn new(index: String, source: Option<Pattern>, target: Option<Pattern>) -> Self {
        Self {
            index,
            source: source.map(Box::new),
            target: target.map(Box::new),
        }
    }

    fn is_leaf(&self) -> bool {
        self.source.is_none() && self.target.is_none()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ResolvedLink {
    index: u32,
    source: u32,
    target: u32,
    name: Option<String>,
}

impl ResolvedLink {
    fn new(index: u32, source: u32, target: u32, name: Option<String>) -> Self {
        Self {
            index,
            source,
            target,
            name,
        }
    }

    fn to_link(&self) -> Link {
        Link::new(self.index, self.source, self.target)
    }
}

impl QueryProcessor {
    /// Creates a new QueryProcessor
    pub fn new(trace: bool) -> Self {
        Self {
            trace,
            auto_create_missing_references: false,
        }
    }

    pub fn with_auto_create_missing_references(
        mut self,
        auto_create_missing_references: bool,
    ) -> Self {
        self.auto_create_missing_references = auto_create_missing_references;
        self
    }

    /// Processes a LiNo query and returns the list of changes
    pub fn process_query(
        &self,
        storage: &mut LinkStorage,
        query: &str,
    ) -> Result<Vec<(Option<Link>, Option<Link>)>> {
        self.trace_msg(&format!("[ProcessQuery] Query: \"{}\"", query));

        let query = query.trim();
        if query.is_empty() {
            self.trace_msg("[ProcessQuery] Query is empty, returning.");
            return Ok(vec![]);
        }

        let parser = Parser::new();
        let parsed_links = parser.parse(query)?;

        self.trace_msg(&format!(
            "[ProcessQuery] Parser returned {} top-level link(s).",
            parsed_links.len()
        ));

        if parsed_links.is_empty() {
            self.trace_msg("[ProcessQuery] No top-level parsed links found, returning.");
            return Ok(vec![]);
        }

        // Accept both the wrapped form `((restriction) (substitution))` and
        // the C# parser-compatible form `restriction substitution`.
        let (restriction_link, substitution_link) = match &parsed_links[0].values {
            Some(values) if values.len() >= 2 => (&values[0], &values[1]),
            _ if parsed_links.len() >= 2 => (&parsed_links[0], &parsed_links[1]),
            _ => {
                self.trace_msg("[ProcessQuery] Query has fewer than 2 links, returning.");
                return Ok(vec![]);
            }
        };

        self.trace_msg(&format!(
            "[ProcessQuery] Restriction link => Id={:?} Values.Count={}",
            restriction_link.id,
            restriction_link.values_count()
        ));
        self.trace_msg(&format!(
            "[ProcessQuery] Substitution link => Id={:?} Values.Count={}",
            substitution_link.id,
            substitution_link.values_count()
        ));

        let mut changes_list = Vec::new();

        // If both restriction and substitution are empty, do nothing
        if restriction_link.is_empty() && substitution_link.is_empty() {
            self.trace_msg(
                "[ProcessQuery] Restriction & substitution both empty => no operation, returning.",
            );
            return Ok(vec![]);
        }

        // Creation scenario: no restriction, only substitution
        if restriction_link.is_empty() && !substitution_link.is_empty() {
            self.trace_msg(
                "[ProcessQuery] No restriction, but substitution is non-empty => creation scenario.",
            );
            if let Some(values) = &substitution_link.values {
                changes_list.extend(
                    self.validate_links_exist_or_will_be_created(storage, &[], values)?
                        .into_iter()
                        .map(|link| (None, Some(link))),
                );

                for link_to_create in values {
                    let created_id = self.ensure_link_created(storage, link_to_create)?;
                    self.trace_msg(&format!(
                        "[ProcessQuery] Created link ID #{} from substitution pattern.",
                        created_id
                    ));
                    if let Some(link) = storage.get(created_id) {
                        changes_list.push((None, Some(*link)));
                    }
                }
            }
            storage.save()?;
            return Ok(changes_list);
        }

        // Deletion scenario: restriction but no substitution
        if !restriction_link.is_empty() && substitution_link.is_empty() {
            self.trace_msg(
                "[ProcessQuery] Restriction non-empty, substitution empty => deletion scenario.",
            );
            let restriction_values = restriction_link.values.as_deref().unwrap_or(&[]);
            changes_list.extend(
                self.validate_links_exist_or_will_be_created(storage, restriction_values, &[])?
                    .into_iter()
                    .map(|link| (None, Some(link))),
            );

            let restriction_patterns = self.patterns_from_lino(restriction_link);
            let mut links_to_delete = Vec::new();
            for pattern in &restriction_patterns {
                links_to_delete.extend(self.matched_links(storage, pattern, &HashMap::new()));
            }
            links_to_delete.sort_by_key(|link| link.index);
            links_to_delete.dedup_by_key(|link| link.index);

            for link in links_to_delete {
                if storage.exists(link.index) {
                    let before = storage.delete(link.index)?;
                    changes_list.push((Some(before), None));
                    self.trace_msg(&format!("[ProcessQuery] Deleted link ID #{}.", link.index));
                }
            }
            storage.save()?;
            return Ok(changes_list);
        }

        // Update/Mixed scenario: both restriction and substitution have values
        self.trace_msg(
            "[ProcessQuery] Both restriction and substitution non-empty => update/mixed scenario.",
        );

        let restriction_patterns = self.patterns_from_lino(restriction_link);
        let substitution_patterns = self.patterns_from_lino(substitution_link);
        let restriction_values = restriction_link.values.as_deref().unwrap_or(&[]);
        let substitution_values = substitution_link.values.as_deref().unwrap_or(&[]);
        changes_list.extend(
            self.validate_links_exist_or_will_be_created(
                storage,
                restriction_values,
                substitution_values,
            )?
            .into_iter()
            .map(|link| (None, Some(link))),
        );
        let solutions = self.find_all_solutions(storage, &restriction_patterns);

        if solutions.is_empty() {
            self.trace_msg("[ProcessQuery] No solutions found => returning.");
            if !changes_list.is_empty() {
                storage.save()?;
            }
            return Ok(changes_list);
        }

        let all_solutions_no_operation = solutions.iter().all(|solution| {
            self.solution_is_no_operation(
                storage,
                solution,
                &restriction_patterns,
                &substitution_patterns,
            )
        });

        if all_solutions_no_operation {
            for solution in &solutions {
                for pattern in &restriction_patterns {
                    for link in self.matched_links(storage, pattern, solution) {
                        if !changes_list.contains(&(Some(link), Some(link))) {
                            changes_list.push((Some(link), Some(link)));
                        }
                    }
                }
            }
            return Ok(changes_list);
        }

        for solution in &solutions {
            let restriction_links =
                self.resolve_patterns(storage, &restriction_patterns, solution, false)?;
            let substitution_links =
                self.resolve_patterns(storage, &substitution_patterns, solution, true)?;
            let operations = self.determine_operations(&restriction_links, &substitution_links);
            for (before, after) in operations {
                self.apply_operation(storage, before, after, &mut changes_list)?;
            }
        }

        storage.save()?;

        // Simplify changes
        let simplified = self.simplify_changes_list(&changes_list);

        Ok(simplified)
    }

    fn validate_links_exist_or_will_be_created(
        &self,
        storage: &mut LinkStorage,
        restriction_patterns: &[LinoLink],
        substitution_patterns: &[LinoLink],
    ) -> Result<Vec<Link>> {
        LinkReferenceValidator::new(self.trace, self.auto_create_missing_references)
            .validate_links_exist_or_will_be_created(
                storage,
                restriction_patterns,
                substitution_patterns,
            )
    }

    fn patterns_from_lino(&self, lino_link: &LinoLink) -> Vec<Pattern> {
        let mut patterns = lino_link
            .values
            .as_ref()
            .map(|values| {
                values
                    .iter()
                    .map(Self::create_pattern_from_lino)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        if lino_link.id.is_some() {
            patterns.insert(0, Self::create_pattern_from_lino(lino_link));
        }

        patterns
    }

    fn create_pattern_from_lino(lino_link: &LinoLink) -> Pattern {
        let index = lino_link.id.clone().unwrap_or_default();
        match &lino_link.values {
            Some(values) if values.len() == 2 => Pattern::new(
                index,
                Some(Self::create_pattern_from_lino(&values[0])),
                Some(Self::create_pattern_from_lino(&values[1])),
            ),
            _ => Pattern::new(index, None, None),
        }
    }

    fn find_all_solutions(
        &self,
        storage: &LinkStorage,
        patterns: &[Pattern],
    ) -> Vec<HashMap<String, u32>> {
        let mut partial_solutions = vec![HashMap::new()];

        for pattern in patterns {
            let mut new_solutions = Vec::new();
            for solution in &partial_solutions {
                for match_solution in self.match_pattern(storage, pattern, solution) {
                    if Self::solutions_are_compatible(solution, &match_solution) {
                        let mut combined = solution.clone();
                        combined.extend(match_solution);
                        new_solutions.push(combined);
                    }
                }
            }
            partial_solutions = new_solutions;
            if partial_solutions.is_empty() {
                break;
            }
        }

        partial_solutions
    }

    fn solutions_are_compatible(
        existing: &HashMap<String, u32>,
        new_assignments: &HashMap<String, u32>,
    ) -> bool {
        new_assignments
            .iter()
            .all(|(key, value)| existing.get(key).is_none_or(|existing| existing == value))
    }

    fn match_pattern(
        &self,
        storage: &LinkStorage,
        pattern: &Pattern,
        current_solution: &HashMap<String, u32>,
    ) -> Vec<HashMap<String, u32>> {
        if pattern.is_leaf() {
            let resolved_index = self.resolve_match_id(storage, &pattern.index, current_solution);
            return storage
                .all()
                .into_iter()
                .filter(|link| Self::is_any(resolved_index) || link.index == resolved_index)
                .map(|link| {
                    let mut assignments = HashMap::new();
                    Self::assign_variable(&pattern.index, link.index, &mut assignments);
                    assignments
                })
                .collect();
        }

        let resolved_index = self.resolve_match_id(storage, &pattern.index, current_solution);

        if !Self::is_variable(&pattern.index)
            && !Self::is_any(resolved_index)
            && resolved_index != 0
            && storage.exists(resolved_index)
        {
            let link = *storage.get(resolved_index).unwrap();
            return self.match_link_against_pattern(storage, pattern, link, current_solution);
        }

        storage
            .all()
            .into_iter()
            .copied()
            .flat_map(|link| {
                self.match_link_against_pattern(storage, pattern, link, current_solution)
            })
            .collect()
    }

    fn match_link_against_pattern(
        &self,
        storage: &LinkStorage,
        pattern: &Pattern,
        link: Link,
        current_solution: &HashMap<String, u32>,
    ) -> Vec<HashMap<String, u32>> {
        if !self.check_id_match(storage, &pattern.index, link.index, current_solution) {
            return Vec::new();
        }

        let mut results = Vec::new();
        let source_matches = self.recursive_match_subpattern(
            storage,
            pattern.source.as_deref(),
            link.source,
            current_solution,
        );

        for source_solution in source_matches {
            let target_matches = self.recursive_match_subpattern(
                storage,
                pattern.target.as_deref(),
                link.target,
                &source_solution,
            );
            for mut target_solution in target_matches {
                Self::assign_variable(&pattern.index, link.index, &mut target_solution);
                results.push(target_solution);
            }
        }

        results
    }

    fn recursive_match_subpattern(
        &self,
        storage: &LinkStorage,
        pattern: Option<&Pattern>,
        link_id: u32,
        current_solution: &HashMap<String, u32>,
    ) -> Vec<HashMap<String, u32>> {
        let Some(pattern) = pattern else {
            return vec![current_solution.clone()];
        };

        if pattern.is_leaf() {
            if self.check_id_match(storage, &pattern.index, link_id, current_solution) {
                let mut solution = current_solution.clone();
                Self::assign_variable(&pattern.index, link_id, &mut solution);
                return vec![solution];
            }
            return Vec::new();
        }

        let Some(link) = storage.get(link_id).copied() else {
            return Vec::new();
        };

        self.match_link_against_pattern(storage, pattern, link, current_solution)
    }

    fn check_id_match(
        &self,
        storage: &LinkStorage,
        pattern_id: &str,
        candidate_id: u32,
        current_solution: &HashMap<String, u32>,
    ) -> bool {
        if pattern_id.is_empty() || pattern_id == "*" {
            return true;
        }

        if Self::is_variable(pattern_id) {
            return current_solution
                .get(pattern_id)
                .is_none_or(|existing| *existing == candidate_id);
        }

        if let Ok(parsed) = pattern_id.parse::<u32>() {
            return parsed == candidate_id;
        }

        storage
            .get_by_name(pattern_id)
            .is_some_and(|named_id| named_id == candidate_id)
    }

    fn resolve_match_id(
        &self,
        storage: &LinkStorage,
        identifier: &str,
        current_solution: &HashMap<String, u32>,
    ) -> u32 {
        if identifier.is_empty() || identifier == "*" {
            return u32::MAX;
        }
        if let Some(value) = current_solution.get(identifier) {
            return *value;
        }
        if Self::is_variable(identifier) {
            return u32::MAX;
        }
        if let Ok(parsed) = identifier.parse::<u32>() {
            return parsed;
        }
        storage.get_by_name(identifier).unwrap_or(0)
    }

    fn matched_links(
        &self,
        storage: &LinkStorage,
        pattern: &Pattern,
        solution: &HashMap<String, u32>,
    ) -> Vec<Link> {
        if pattern.is_leaf() {
            let resolved_index = self.resolve_match_id(storage, &pattern.index, solution);
            return storage
                .all()
                .into_iter()
                .filter(|link| Self::is_any(resolved_index) || link.index == resolved_index)
                .copied()
                .collect();
        }

        self.match_pattern(storage, pattern, solution)
            .into_iter()
            .filter_map(|matched_solution| {
                self.resolve_pattern_readonly(storage, pattern, &matched_solution, false)
            })
            .flat_map(|definition| self.links_matching_definition(storage, &definition))
            .collect()
    }

    fn solution_is_no_operation(
        &self,
        storage: &LinkStorage,
        solution: &HashMap<String, u32>,
        restrictions: &[Pattern],
        substitutions: &[Pattern],
    ) -> bool {
        let mut restriction_links = self
            .resolve_patterns_readonly(storage, restrictions, solution, false)
            .into_iter()
            .map(|definition| definition.to_link())
            .collect::<Vec<_>>();
        let mut substitution_links = self
            .resolve_patterns_readonly(storage, substitutions, solution, true)
            .into_iter()
            .map(|definition| definition.to_link())
            .collect::<Vec<_>>();

        restriction_links.sort_by_key(|link| link.index);
        substitution_links.sort_by_key(|link| link.index);

        restriction_links == substitution_links
    }

    fn resolve_patterns_readonly(
        &self,
        storage: &LinkStorage,
        patterns: &[Pattern],
        solution: &HashMap<String, u32>,
        is_substitution: bool,
    ) -> Vec<ResolvedLink> {
        patterns
            .iter()
            .filter_map(|pattern| {
                self.resolve_pattern_readonly(storage, pattern, solution, is_substitution)
            })
            .collect()
    }

    fn resolve_pattern_readonly(
        &self,
        storage: &LinkStorage,
        pattern: &Pattern,
        solution: &HashMap<String, u32>,
        is_substitution: bool,
    ) -> Option<ResolvedLink> {
        if pattern.is_leaf() {
            let index = self.resolve_identifier_readonly(
                storage,
                &pattern.index,
                solution,
                if is_substitution { 0 } else { u32::MAX },
            );
            return Some(ResolvedLink::new(index, u32::MAX, u32::MAX, None));
        }

        let source = self
            .resolve_pattern_readonly(
                storage,
                pattern.source.as_deref()?,
                solution,
                is_substitution,
            )?
            .index;
        let target = self
            .resolve_pattern_readonly(
                storage,
                pattern.target.as_deref()?,
                solution,
                is_substitution,
            )?
            .index;
        let default_index = if is_substitution { 0 } else { u32::MAX };
        let index =
            self.resolve_identifier_readonly(storage, &pattern.index, solution, default_index);

        Some(ResolvedLink::new(index, source, target, None))
    }

    fn resolve_identifier_readonly(
        &self,
        storage: &LinkStorage,
        identifier: &str,
        solution: &HashMap<String, u32>,
        default_value: u32,
    ) -> u32 {
        if identifier.is_empty() {
            return default_value;
        }
        if identifier == "*" {
            return u32::MAX;
        }
        if let Some(value) = solution.get(identifier) {
            return *value;
        }
        if Self::is_variable(identifier) {
            return default_value;
        }
        if let Ok(parsed) = identifier.parse::<u32>() {
            return parsed;
        }
        storage.get_by_name(identifier).unwrap_or(default_value)
    }

    fn resolve_patterns(
        &self,
        storage: &mut LinkStorage,
        patterns: &[Pattern],
        solution: &HashMap<String, u32>,
        is_substitution: bool,
    ) -> Result<Vec<ResolvedLink>> {
        patterns
            .iter()
            .map(|pattern| self.resolve_pattern(storage, pattern, solution, is_substitution))
            .collect()
    }

    fn resolve_pattern(
        &self,
        storage: &mut LinkStorage,
        pattern: &Pattern,
        solution: &HashMap<String, u32>,
        is_substitution: bool,
    ) -> Result<ResolvedLink> {
        if pattern.is_leaf() {
            let index = self.resolve_identifier(
                storage,
                &pattern.index,
                solution,
                if is_substitution { 0 } else { u32::MAX },
                is_substitution,
            )?;
            return Ok(ResolvedLink::new(index, u32::MAX, u32::MAX, None));
        }

        let source = self
            .resolve_pattern(
                storage,
                pattern.source.as_deref().unwrap(),
                solution,
                is_substitution,
            )?
            .index;
        let target = self
            .resolve_pattern(
                storage,
                pattern.target.as_deref().unwrap(),
                solution,
                is_substitution,
            )?
            .index;
        let default_index = if is_substitution { 0 } else { u32::MAX };
        let mut index =
            self.resolve_identifier(storage, &pattern.index, solution, default_index, false)?;
        let mut name = None;

        if is_substitution
            && !pattern.index.is_empty()
            && !Self::is_numeric_or_wildcard(&pattern.index)
        {
            name = Some(pattern.index.clone());
            if index == 0 {
                if let Some(existing_id) = storage.search(source, target) {
                    index = existing_id;
                }
            }
        }

        Ok(ResolvedLink::new(index, source, target, name))
    }

    fn resolve_identifier(
        &self,
        storage: &mut LinkStorage,
        identifier: &str,
        solution: &HashMap<String, u32>,
        default_value: u32,
        create_named_leaf: bool,
    ) -> Result<u32> {
        if identifier.is_empty() {
            return Ok(default_value);
        }
        if identifier == "*" {
            return Ok(u32::MAX);
        }
        if let Some(value) = solution.get(identifier) {
            return Ok(*value);
        }
        if Self::is_variable(identifier) {
            return Ok(default_value);
        }
        if let Ok(parsed) = identifier.parse::<u32>() {
            return Ok(parsed);
        }
        if let Some(named_id) = storage.get_by_name(identifier) {
            return Ok(named_id);
        }
        if create_named_leaf {
            return Ok(storage.get_or_create_named(identifier));
        }
        Ok(default_value)
    }

    fn determine_operations(
        &self,
        restrictions: &[ResolvedLink],
        substitutions: &[ResolvedLink],
    ) -> Vec<(Option<ResolvedLink>, Option<ResolvedLink>)> {
        let mut operations = Vec::new();
        let mut restriction_by_index = HashMap::new();
        let mut substitution_by_index = HashMap::new();
        let mut wildcard_restrictions = Vec::new();
        let mut wildcard_substitutions = Vec::new();

        for restriction in restrictions {
            if Self::is_normal_index(restriction.index) {
                restriction_by_index.insert(restriction.index, restriction.clone());
            } else {
                wildcard_restrictions.push(restriction.clone());
            }
        }

        for substitution in substitutions {
            if Self::is_normal_index(substitution.index) {
                substitution_by_index.insert(substitution.index, substitution.clone());
            } else {
                wildcard_substitutions.push(substitution.clone());
            }
        }

        let mut all_indices = restriction_by_index
            .keys()
            .chain(substitution_by_index.keys())
            .copied()
            .collect::<Vec<_>>();
        all_indices.sort_unstable();
        all_indices.dedup();

        for index in all_indices {
            match (
                restriction_by_index.get(&index),
                substitution_by_index.get(&index),
            ) {
                (Some(before), Some(after)) => {
                    operations.push((Some(before.clone()), Some(after.clone())));
                }
                (Some(before), None) => operations.push((Some(before.clone()), None)),
                (None, Some(after)) => operations.push((None, Some(after.clone()))),
                (None, None) => {}
            }
        }

        operations.extend(
            wildcard_restrictions
                .into_iter()
                .map(|restriction| (Some(restriction), None)),
        );
        operations.extend(
            wildcard_substitutions
                .into_iter()
                .map(|substitution| (None, Some(substitution))),
        );

        operations
    }

    fn apply_operation(
        &self,
        storage: &mut LinkStorage,
        before: Option<ResolvedLink>,
        after: Option<ResolvedLink>,
        changes: &mut Vec<(Option<Link>, Option<Link>)>,
    ) -> Result<()> {
        match (before, after) {
            (Some(before), None) => {
                let mut links = self.links_matching_definition(storage, &before);
                links.sort_by_key(|link| link.index);
                links.dedup_by_key(|link| link.index);
                for link in links {
                    if storage.exists(link.index) {
                        let deleted = storage.delete(link.index)?;
                        changes.push((Some(deleted), None));
                    }
                }
            }
            (None, Some(after)) => {
                let created = self.create_or_update_resolved_link(storage, &after)?;
                changes.push((None, Some(created)));
            }
            (Some(before), Some(after)) => {
                if before.index == after.index && storage.exists(before.index) {
                    let before_link = *storage.get(before.index).unwrap();
                    if before_link.source != after.source || before_link.target != after.target {
                        storage.update(before.index, after.source, after.target)?;
                    }
                    if let Some(name) = &after.name {
                        storage.set_name(before.index, name);
                    }
                    let after_link = *storage.get(before.index).unwrap();
                    changes.push((Some(before_link), Some(after_link)));
                } else {
                    self.apply_operation(storage, Some(before), None, changes)?;
                    self.apply_operation(storage, None, Some(after), changes)?;
                }
            }
            (None, None) => {}
        }

        Ok(())
    }

    fn create_or_update_resolved_link(
        &self,
        storage: &mut LinkStorage,
        definition: &ResolvedLink,
    ) -> Result<Link> {
        let id = if Self::is_normal_index(definition.index) {
            storage.ensure_created(definition.index);
            storage.update(definition.index, definition.source, definition.target)?;
            definition.index
        } else if let Some(existing_id) = storage.search(definition.source, definition.target) {
            existing_id
        } else {
            storage.create(definition.source, definition.target)
        };

        if let Some(name) = &definition.name {
            storage.set_name(id, name);
        }

        Ok(*storage.get(id).unwrap())
    }

    fn links_matching_definition(
        &self,
        storage: &LinkStorage,
        definition: &ResolvedLink,
    ) -> Vec<Link> {
        storage
            .all()
            .into_iter()
            .filter(|link| {
                (definition.index == 0
                    || Self::is_any(definition.index)
                    || link.index == definition.index)
                    && (Self::is_any(definition.source) || link.source == definition.source)
                    && (Self::is_any(definition.target) || link.target == definition.target)
            })
            .copied()
            .collect()
    }

    fn assign_variable(id: &str, value: u32, assignments: &mut HashMap<String, u32>) {
        if Self::is_variable(id) && value != 0 {
            assignments.insert(id.to_string(), value);
        }
    }

    fn is_variable(identifier: &str) -> bool {
        !identifier.is_empty() && identifier.starts_with('$')
    }

    fn is_any(value: u32) -> bool {
        value == u32::MAX
    }

    fn is_normal_index(value: u32) -> bool {
        value != 0 && !Self::is_any(value)
    }

    fn is_numeric_or_wildcard(identifier: &str) -> bool {
        identifier == "*" || identifier.parse::<u32>().is_ok()
    }

    /// Ensures a link is created from a LinoLink pattern
    fn ensure_link_created(&self, storage: &mut LinkStorage, lino_link: &LinoLink) -> Result<u32> {
        // Handle leaf nodes (names or numbers)
        if !lino_link.has_values() {
            if let Some(ref id) = lino_link.id {
                if id == "*" || Self::is_variable(id) {
                    return Ok(u32::MAX);
                }

                // Check if it's a number
                if let Ok(num) = id.parse::<u32>() {
                    return Ok(num);
                }

                // It's a name - get or create
                return Ok(storage.get_or_create_named(id));
            }
            return Ok(0);
        }

        // Handle composite links with 2 values
        if lino_link.values_count() == 2 {
            let values = lino_link.values.as_ref().unwrap();

            // Recursively ensure source and target exist
            let source_id = self.ensure_link_created(storage, &values[0])?;
            let target_id = self.ensure_link_created(storage, &values[1])?;

            // Create or get the composite link
            let link_id = if let Some(ref id) = lino_link.id {
                if let Ok(num) = id.parse::<u32>() {
                    // Specific ID requested
                    storage.ensure_created(num);
                    storage.update(num, source_id, target_id)?;
                    num
                } else if id == "*" || Self::is_variable(id) {
                    storage.get_or_create(source_id, target_id)
                } else {
                    // Named link
                    let existing = storage.get_by_name(id);
                    if let Some(id_num) = existing {
                        storage.update(id_num, source_id, target_id)?;
                        id_num
                    } else {
                        let new_id = storage.create(source_id, target_id);
                        storage.set_name(new_id, id);
                        new_id
                    }
                }
            } else {
                // Anonymous link
                storage.get_or_create(source_id, target_id)
            };

            return Ok(link_id);
        }

        Err(LinkError::InvalidFormat("Invalid link structure".to_string()).into())
    }

    /// Simplifies the changes list
    fn simplify_changes_list(
        &self,
        changes: &[(Option<Link>, Option<Link>)],
    ) -> Vec<(Option<Link>, Option<Link>)> {
        // Convert to the format expected by simplify_changes
        let mut to_simplify: Vec<(Link, Link)> = Vec::new();
        let mut non_simplifiable: Vec<(Option<Link>, Option<Link>)> = Vec::new();

        for (before, after) in changes {
            match (before, after) {
                (Some(b), Some(a)) => {
                    to_simplify.push((*b, *a));
                }
                _ => {
                    non_simplifiable.push((*before, *after));
                }
            }
        }

        let simplified = simplify_changes(to_simplify);

        let mut result: Vec<(Option<Link>, Option<Link>)> = non_simplifiable;
        for (b, a) in simplified {
            result.push((Some(b), Some(a)));
        }

        result
    }

    /// Logs a trace message if tracing is enabled
    fn trace_msg(&self, msg: &str) {
        if self.trace {
            eprintln!("{}", msg);
        }
    }
}
