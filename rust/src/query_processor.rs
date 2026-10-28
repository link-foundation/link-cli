//! QueryProcessor - Handles LiNo query parsing and execution
//!
//! This module provides the QueryProcessor for processing LiNo queries.
//! Corresponds to BasicQueryProcessor, MixedQueryProcessor, and AdvancedMixedQueryProcessor in C#

use anyhow::Result;
use std::collections::{HashMap, HashSet};

use crate::changes_simplifier::simplify_generic_changes;
use crate::error::LinkError;
use crate::link::GenericLink as Link;
use crate::link_reference_validator::GenericLinkReferenceValidator;
use crate::lino_link::LinoLink;
use crate::named_type_links::NamedTypeLinks;
use crate::query_address::QueryAddress;
use crate::query_types::{GenericResolvedLink as ResolvedLink, Pattern};

// Pattern matching lives in a submodule; see query_processor/matching.rs.
mod matching;
// Write-side operations live in a submodule; see query_processor/mutations.rs.
mod mutations;

/// The `(before, after)` states a query reports, where `None` is no link:
/// `(None, Some(link))` is a creation and `(Some(link), None)` a deletion.
pub type Changes<T = u32> = Vec<(Option<Link<T>>, Option<Link<T>>)>;

type ResolvedChange<T> = (Option<ResolvedLink<T>>, Option<ResolvedLink<T>>);

/// The existing u32 CLI facade.
pub type QueryProcessor = GenericQueryProcessor<u32>;

/// QueryProcessor handles LiNo query parsing and execution
/// Corresponds to AdvancedMixedQueryProcessor in C#
pub struct GenericQueryProcessor<T: QueryAddress> {
    address: std::marker::PhantomData<T>,
    trace: bool,
    auto_create_missing_references: bool,
    validate_references: bool,
}

impl<T: QueryAddress> GenericQueryProcessor<T> {
    /// Creates a new QueryProcessor
    pub fn new(trace: bool) -> Self {
        Self {
            address: std::marker::PhantomData,
            trace,
            auto_create_missing_references: false,
            validate_references: true,
        }
    }

    pub fn with_auto_create_missing_references(
        mut self,
        auto_create_missing_references: bool,
    ) -> Self {
        self.auto_create_missing_references = auto_create_missing_references;
        self
    }

    /// Processes a LiNo query and returns the list of changes.
    ///
    /// Every scenario is simplified on the way out, matching the C# CLI, which
    /// collects the raw handler calls and runs `SimplifyChanges` over them once
    /// in `Program.cs` regardless of which branch of the processor produced
    /// them.
    pub fn process_query(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        query: &str,
    ) -> Result<Changes<T>> {
        let changes = self.process_query_raw(storage, query)?;
        Ok(self.simplify_changes_list(&changes))
    }

    /// The processor proper: applies `query` and reports the raw
    /// `(before, after)` states, in the order they happened.
    fn process_query_raw(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        query: &str,
    ) -> Result<Changes<T>> {
        if storage
            .query_limits()
            .is_some_and(|limits| query.len() > limits.max_bytes)
        {
            return Err(LinkError::QueryError("query byte limit exceeded".into()).into());
        }
        self.trace_msg(&format!("[ProcessQuery] Query: \"{}\"", query));

        let query = query.trim();
        if query.is_empty() {
            self.trace_msg("[ProcessQuery] Query is empty, returning.");
            return Ok(vec![]);
        }

        let parsed_links = crate::query_parser::parse_for::<T>(query, storage.query_limits())?;

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

        self.process_lino_raw(storage, restriction_link, substitution_link)
    }

    /// Reuse the complete execution path on an owned scratch store. Only the
    /// reference-validation recursion is disabled; matching and allocation are
    /// exactly the operations the real session will execute.
    pub(crate) fn preview(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        restriction_link: &LinoLink,
        substitution_link: &LinoLink,
    ) -> Result<()> {
        let preview = Self {
            address: std::marker::PhantomData,
            trace: self.trace,
            auto_create_missing_references: self.auto_create_missing_references,
            validate_references: false,
        };
        let changes = preview.process_lino_raw(storage, restriction_link, substitution_link)?;
        if storage.query_limits().is_some() {
            // The protocol names references from the final staged state. Check
            // that same output while only owned preview rows have changed.
            let mut budget =
                crate::query_reply_budget::QueryReplyBudget::new(storage.query_limits());
            for (before, after) in preview.simplify_changes_list(&changes) {
                for link in [before, after].into_iter().flatten() {
                    for id in [link.index, link.source, link.target] {
                        budget.reference(storage, id)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn process_lino_raw(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        restriction_link: &LinoLink,
        substitution_link: &LinoLink,
    ) -> Result<Changes<T>> {
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
                changes_list.extend(self.validate_links_exist_or_will_be_created(
                    storage,
                    restriction_link,
                    substitution_link,
                )?);

                for link_to_create in values {
                    let created_id =
                        self.ensure_link_created(storage, link_to_create, &mut changes_list)?;
                    self.trace_msg(&format!(
                        "[ProcessQuery] Created link ID #{} from substitution pattern.",
                        created_id
                    ));
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
            changes_list.extend(self.validate_links_exist_or_will_be_created(
                storage,
                restriction_link,
                substitution_link,
            )?);

            let restriction_patterns = self.patterns_from_lino(restriction_link);
            let mut links_to_delete = Vec::new();
            for pattern in &restriction_patterns {
                links_to_delete.extend(self.matched_links(storage, pattern, &HashMap::new())?);
            }
            links_to_delete.sort_by_key(|link| link.index);
            links_to_delete.dedup_by_key(|link| link.index);

            for link in links_to_delete {
                if storage.exists(link.index) {
                    self.delete_observed(storage, link.index, &mut changes_list)?;
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
        changes_list.extend(self.validate_links_exist_or_will_be_created(
            storage,
            restriction_link,
            substitution_link,
        )?);
        let solutions = self.find_all_solutions(storage, &restriction_patterns)?;

        if solutions.is_empty() {
            self.trace_msg("[ProcessQuery] No solutions found => returning.");
            if !changes_list.is_empty() {
                storage.save()?;
            }
            return Ok(changes_list);
        }

        let mut all_solutions_no_operation = true;
        for solution in &solutions {
            if !self.solution_is_no_operation(
                storage,
                solution,
                &restriction_patterns,
                &substitution_patterns,
            )? {
                all_solutions_no_operation = false;
                break;
            }
        }

        if all_solutions_no_operation {
            for solution in &solutions {
                for pattern in &restriction_patterns {
                    for link in self.matched_links(storage, pattern, solution)? {
                        if !changes_list.contains(&(Some(link), Some(link))) {
                            changes_list.push((Some(link), Some(link)));
                        }
                    }
                }
            }
            return Ok(changes_list);
        }

        let mut all_planned_operations = Vec::new();
        for solution in &solutions {
            let restriction_links = self.resolve_patterns(
                storage,
                &restriction_patterns,
                solution,
                false,
                &mut changes_list,
            )?;
            let substitution_links = self.resolve_patterns(
                storage,
                &substitution_patterns,
                solution,
                true,
                &mut changes_list,
            )?;
            let operations = self.determine_operations(&restriction_links, &substitution_links);
            Self::check_result_count(
                storage,
                all_planned_operations
                    .len()
                    .saturating_add(operations.len()),
            )?;
            all_planned_operations.extend(operations);
        }

        let intended_final_states = Self::intended_final_states(&all_planned_operations);

        for (before, after) in all_planned_operations {
            self.apply_operation(storage, before, after, &mut changes_list)?;
        }

        self.restore_unexpected_deletions(storage, &intended_final_states, &mut changes_list)?;

        storage.save()?;

        Ok(changes_list)
    }

    fn validate_links_exist_or_will_be_created(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        restriction: &LinoLink,
        substitution: &LinoLink,
    ) -> Result<Changes<T>> {
        if !self.validate_references {
            return Ok(Vec::new());
        }
        GenericLinkReferenceValidator::<T>::for_query(
            self.trace,
            self.auto_create_missing_references,
        )
        .validate_with_plan(
            storage,
            restriction.values.as_deref().unwrap_or(&[]),
            substitution.values.as_deref().unwrap_or(&[]),
            |storage| storage.reserve_query_allocations(self, restriction, substitution),
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
        storage: &mut impl NamedTypeLinks<T>,
        patterns: &[Pattern],
    ) -> Result<Vec<HashMap<String, T>>> {
        let mut partial_solutions = vec![HashMap::new()];

        for pattern in patterns {
            let mut new_solutions = Vec::new();
            let mut bindings = 0usize;
            let mut bytes = 0usize;
            for solution in &partial_solutions {
                for match_solution in self.match_pattern(storage, pattern, solution)? {
                    if Self::solutions_are_compatible(solution, &match_solution) {
                        // Admit the union before cloning either keys or maps.
                        let (mut count, mut size) = Self::solution_cost(solution);
                        for key in match_solution
                            .keys()
                            .filter(|key| !solution.contains_key(*key))
                        {
                            count = count.saturating_add(1);
                            size = size.saturating_add(Self::binding_cost(key));
                        }
                        Self::check_solution_capacity(
                            storage,
                            new_solutions.len() + 1,
                            bindings.saturating_add(count),
                            bytes.saturating_add(size),
                        )?;
                        let mut combined = solution.clone();
                        combined.extend(match_solution);
                        bindings = bindings.saturating_add(count);
                        bytes = bytes.saturating_add(size);
                        new_solutions.push(combined);
                    }
                }
            }
            partial_solutions = new_solutions;
            if partial_solutions.is_empty() {
                break;
            }
        }

        Ok(partial_solutions)
    }

    fn check_result_count(storage: &impl NamedTypeLinks<T>, count: usize) -> Result<()> {
        if storage
            .query_limits()
            .is_some_and(|limits| count > limits.max_results)
        {
            return Err(LinkError::QueryError("query result limit exceeded".into()).into());
        }
        Ok(())
    }

    fn binding_cost(key: &str) -> usize {
        // Include a conservative entry/table allowance as well as owned key bytes.
        key.len().saturating_add(64 + std::mem::size_of::<T>())
    }

    fn solution_cost(solution: &HashMap<String, T>) -> (usize, usize) {
        (
            solution.len(),
            solution.keys().fold(0usize, |bytes, key| {
                bytes.saturating_add(Self::binding_cost(key))
            }),
        )
    }

    fn check_solution_capacity(
        storage: &impl NamedTypeLinks<T>,
        results: usize,
        bindings: usize,
        bytes: usize,
    ) -> Result<()> {
        Self::check_result_count(storage, results)?;
        if storage.query_limits().is_some_and(|limits| {
            bindings > limits.max_bindings || bytes > limits.max_binding_bytes
        }) {
            return Err(LinkError::QueryError("query binding memory limit exceeded".into()).into());
        }
        Ok(())
    }

    fn solutions_are_compatible(
        existing: &HashMap<String, T>,
        new_assignments: &HashMap<String, T>,
    ) -> bool {
        new_assignments
            .iter()
            .all(|(key, value)| existing.get(key).is_none_or(|existing| existing == value))
    }

    fn resolve_patterns_readonly(
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

    fn resolve_pattern_readonly(
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

    fn resolve_identifier_readonly(
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

    fn resolve_patterns(
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

    fn resolve_pattern(
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

    fn resolve_identifier(
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

    fn determine_operations(
        &self,
        restrictions: &[ResolvedLink<T>],
        substitutions: &[ResolvedLink<T>],
    ) -> Vec<ResolvedChange<T>> {
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
        storage: &mut impl NamedTypeLinks<T>,
        before: Option<ResolvedLink<T>>,
        after: Option<ResolvedLink<T>>,
        changes: &mut Changes<T>,
    ) -> Result<()> {
        match (before, after) {
            (Some(before), None) => {
                let mut links = self.links_matching_definition(storage, &before)?;
                links.sort_by_key(|link| link.index);
                links.dedup_by_key(|link| link.index);
                for link in links {
                    if storage.exists(link.index) {
                        self.delete_observed(storage, link.index, changes)?;
                    }
                }
            }
            (None, Some(after)) => {
                self.create_or_update_resolved_link(storage, &after, changes)?;
            }
            (Some(before), Some(after)) => {
                if before.index == after.index && storage.exists(before.index) {
                    let stored = storage.get_link(before.index).unwrap();
                    if stored.source != after.source || stored.target != after.target {
                        // A merge into a duplicate deletes `before.index`;
                        // `restore_unexpected_deletions` puts it back when the
                        // query asked for it, exactly as the C# processor does.
                        self.update_observed(
                            storage,
                            before.index,
                            after.source,
                            after.target,
                            changes,
                        )?;
                    } else {
                        changes.push((Some(stored), Some(stored)));
                    }
                    if let Some(name) = &after.name {
                        storage.set_name(before.index, crate::query_parser::name(name))?;
                    }
                } else {
                    self.apply_operation(storage, Some(before), None, changes)?;
                    self.apply_operation(storage, None, Some(after), changes)?;
                }
            }
            (None, None) => {}
        }

        Ok(())
    }

    fn links_matching_definition(
        &self,
        storage: &mut impl NamedTypeLinks<T>,
        definition: &ResolvedLink<T>,
    ) -> Result<Vec<Link<T>>> {
        Ok(storage
            .all_links()
            .into_iter()
            .filter(|link| {
                (definition.index == T::from_byte(0)
                    || Self::is_any(definition.index)
                    || link.index == definition.index)
                    && (Self::is_any(definition.source) || link.source == definition.source)
                    && (Self::is_any(definition.target) || link.target == definition.target)
            })
            .collect())
    }

    fn assign_variable(id: &str, value: T, assignments: &mut HashMap<String, T>) {
        // Null is a concrete stored reference. Only a missing map entry means
        // that a variable is unbound; dropping zero loses swaps and equality.
        if Self::is_variable(id) {
            assignments.insert(id.to_string(), value);
        }
    }

    fn is_variable(identifier: &str) -> bool {
        !identifier.is_empty() && identifier.starts_with('$')
    }

    fn is_any(value: T) -> bool {
        value == T::MAX
    }

    /// Resolves a half a query left unspecified against the half already
    /// stored, the way C# resolves its `any` constant on the way into the
    /// store.
    ///
    /// The C# processor marks an unbound substitution variable — and a `*` — with
    /// `links.Constants.Any`, which is a value the *store* understands:
    /// `Update` leaves a half substituted with `any` exactly as it was, and a
    /// link created from one gets `null` there. This processor marks the same
    /// thing with `T::MAX`, which the store underneath does not recognise
    /// (its `any` is `2147483644`, the hybrid-aware constant), so `() (($a $a))`
    /// used to store the literal `4294967295` in both halves where C# stores
    /// `(1: 0 0)`. Resolving at the write boundary keeps `T::MAX` as this
    /// crate's single internal marker while writing what C# writes.
    fn resolve_unspecified(value: T, existing: T) -> T {
        if Self::is_any(value) {
            existing
        } else {
            value
        }
    }

    /// Looks a doublet up with unspecified halves treated as wildcards, the way
    /// C#'s `SearchOrDefault` does.
    ///
    /// `SearchOrDefault` runs through `Each`, which reads `any` in a query as
    /// "every value" rather than as a literal address, so `() ((1 $a))` finds a
    /// stored `(1: 1 1)` instead of creating a second link beside it.
    /// [`NamedTypeLinks::search`] matches literally on purpose — it backs
    /// uniqueness resolution — so the wildcard pass belongs here.
    fn search_unspecified(storage: &mut impl NamedTypeLinks<T>, source: T, target: T) -> Option<T> {
        if !Self::is_any(source) && !Self::is_any(target) {
            return storage.search(source, target);
        }
        storage
            .all_links()
            .into_iter()
            .filter(|link| {
                (Self::is_any(source) || link.source == source)
                    && (Self::is_any(target) || link.target == target)
            })
            .map(|link| link.index)
            .min()
    }

    fn is_normal_index(value: T) -> bool {
        value != T::from_byte(0) && !Self::is_any(value)
    }

    fn is_numeric_or_wildcard(identifier: &str) -> bool {
        identifier == "*" || identifier.parse::<T>().is_ok()
    }

    /// Simplifies the changes list.
    ///
    /// A missing side — the state before a creation, or the state after a
    /// deletion — becomes the null link `(0: 0 0)` on the way in and turns back
    /// into `None` on the way out. C# has no option type here and feeds the
    /// simplifier `default(Link<T><uint>)` for both, so routing the null states
    /// around the simplifier (as this used to) both reported creations and
    /// deletions in a different order than C# and hid them from the chain
    /// collapsing that is the whole point of the pass.
    fn simplify_changes_list(&self, changes: &Changes<T>) -> Changes<T> {
        let to_simplify: Vec<(Link<T>, Link<T>)> = changes
            .iter()
            .map(|(before, after)| {
                (
                    before.unwrap_or_else(Link::null),
                    after.unwrap_or_else(Link::null),
                )
            })
            .collect();

        simplify_generic_changes(to_simplify)
            .into_iter()
            .map(|(before, after)| {
                (
                    (!before.is_null()).then_some(before),
                    (!after.is_null()).then_some(after),
                )
            })
            .collect()
    }

    /// Logs a trace message if tracing is enabled
    fn trace_msg(&self, msg: &str) {
        if self.trace {
            crate::diagnostics::stderr!("{}", msg);
        }
    }
}
