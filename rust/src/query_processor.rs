//! QueryProcessor - Handles LiNo query parsing and execution
//!
//! This module provides the QueryProcessor for processing LiNo queries.
//! Corresponds to BasicQueryProcessor, MixedQueryProcessor, and AdvancedMixedQueryProcessor in C#

use anyhow::Result;
use std::collections::{HashMap, HashSet};

use crate::changes_simplifier::simplify_changes;
use crate::error::LinkError;
use crate::link::Link;
use crate::link_reference_validator::LinkReferenceValidator;
use crate::lino_link::LinoLink;
use crate::named_type_links::NamedTypeLinks;
use crate::parser::Parser;
use crate::query_types::{Pattern, ResolvedLink};

// Pattern matching lives in a submodule; see query_processor/matching.rs.
mod matching;
// Write-side operations live in a submodule; see query_processor/mutations.rs.
mod mutations;

/// QueryProcessor handles LiNo query parsing and execution
/// Corresponds to AdvancedMixedQueryProcessor in C#
pub struct QueryProcessor {
    trace: bool,
    auto_create_missing_references: bool,
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

    /// Processes a LiNo query and returns the list of changes.
    ///
    /// Every scenario is simplified on the way out, matching the C# CLI, which
    /// collects the raw handler calls and runs `SimplifyChanges` over them once
    /// in `Program.cs` regardless of which branch of the processor produced
    /// them.
    pub fn process_query(
        &self,
        storage: &mut impl NamedTypeLinks,
        query: &str,
    ) -> Result<Vec<(Option<Link>, Option<Link>)>> {
        let changes = self.process_query_raw(storage, query)?;
        Ok(self.simplify_changes_list(&changes))
    }

    /// The processor proper: applies `query` and reports the raw
    /// `(before, after)` states, in the order they happened.
    fn process_query_raw(
        &self,
        storage: &mut impl NamedTypeLinks,
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
                        .map(|(before, after)| (Some(before), Some(after))),
                );

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
            let restriction_values = restriction_link.values.as_deref().unwrap_or(&[]);
            changes_list.extend(
                self.validate_links_exist_or_will_be_created(storage, restriction_values, &[])?
                    .into_iter()
                    .map(|(before, after)| (Some(before), Some(after))),
            );

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
        let restriction_values = restriction_link.values.as_deref().unwrap_or(&[]);
        let substitution_values = substitution_link.values.as_deref().unwrap_or(&[]);
        changes_list.extend(
            self.validate_links_exist_or_will_be_created(
                storage,
                restriction_values,
                substitution_values,
            )?
            .into_iter()
            .map(|(before, after)| (Some(before), Some(after))),
        );
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
            let restriction_links =
                self.resolve_patterns(storage, &restriction_patterns, solution, false)?;
            let substitution_links =
                self.resolve_patterns(storage, &substitution_patterns, solution, true)?;
            all_planned_operations
                .extend(self.determine_operations(&restriction_links, &substitution_links));
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
        storage: &mut impl NamedTypeLinks,
        restriction_patterns: &[LinoLink],
        substitution_patterns: &[LinoLink],
    ) -> Result<Vec<(Link, Link)>> {
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
        storage: &mut impl NamedTypeLinks,
        patterns: &[Pattern],
    ) -> Result<Vec<HashMap<String, u32>>> {
        let mut partial_solutions = vec![HashMap::new()];

        for pattern in patterns {
            let mut new_solutions = Vec::new();
            for solution in &partial_solutions {
                for match_solution in self.match_pattern(storage, pattern, solution)? {
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

        Ok(partial_solutions)
    }

    fn solutions_are_compatible(
        existing: &HashMap<String, u32>,
        new_assignments: &HashMap<String, u32>,
    ) -> bool {
        new_assignments
            .iter()
            .all(|(key, value)| existing.get(key).is_none_or(|existing| existing == value))
    }

    fn resolve_patterns_readonly(
        &self,
        storage: &mut impl NamedTypeLinks,
        patterns: &[Pattern],
        solution: &HashMap<String, u32>,
        is_substitution: bool,
    ) -> Result<Vec<ResolvedLink>> {
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
        storage: &mut impl NamedTypeLinks,
        pattern: &Pattern,
        solution: &HashMap<String, u32>,
        is_substitution: bool,
    ) -> Result<Option<ResolvedLink>> {
        if pattern.is_leaf() {
            let index = self.resolve_identifier_readonly(
                storage,
                &pattern.index,
                solution,
                if is_substitution { 0 } else { u32::MAX },
            )?;
            return Ok(Some(ResolvedLink::new(index, u32::MAX, u32::MAX, None)));
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
        let default_index = if is_substitution { 0 } else { u32::MAX };
        let index =
            self.resolve_identifier_readonly(storage, &pattern.index, solution, default_index)?;

        Ok(Some(ResolvedLink::new(index, source, target, None)))
    }

    fn resolve_identifier_readonly(
        &self,
        storage: &mut impl NamedTypeLinks,
        identifier: &str,
        solution: &HashMap<String, u32>,
        default_value: u32,
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
        Ok(storage.get_by_name(identifier)?.unwrap_or(default_value))
    }

    fn resolve_patterns(
        &self,
        storage: &mut impl NamedTypeLinks,
        patterns: &[Pattern],
        solution: &HashMap<String, u32>,
        is_substitution: bool,
    ) -> Result<Vec<ResolvedLink>> {
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
            )?);
        }
        Ok(resolved)
    }

    fn resolve_pattern(
        &self,
        storage: &mut impl NamedTypeLinks,
        pattern: &Pattern,
        solution: &mut HashMap<String, u32>,
        is_substitution: bool,
        visited_indexes: &mut HashSet<u32>,
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

        let mut source = self
            .resolve_pattern(
                storage,
                pattern.source.as_deref().unwrap(),
                solution,
                is_substitution,
                visited_indexes,
            )?
            .index;
        let mut target = self
            .resolve_pattern(
                storage,
                pattern.target.as_deref().unwrap(),
                solution,
                is_substitution,
                visited_indexes,
            )?
            .index;
        let default_index = if is_substitution { 0 } else { u32::MAX };
        let mut index =
            self.resolve_identifier(storage, &pattern.index, solution, default_index, false)?;
        let mut name = None;

        if is_substitution
            && !pattern.index.is_empty()
            && !Self::is_numeric_or_wildcard(&pattern.index)
            && !Self::is_variable(&pattern.index)
        {
            name = Some(pattern.index.clone());
            if index == 0 {
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
        storage: &mut impl NamedTypeLinks,
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
        if let Some(named_id) = storage.get_by_name(identifier)? {
            return Ok(named_id);
        }
        if create_named_leaf {
            return storage.get_or_create_named(identifier);
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
        storage: &mut impl NamedTypeLinks,
        before: Option<ResolvedLink>,
        after: Option<ResolvedLink>,
        changes: &mut Vec<(Option<Link>, Option<Link>)>,
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
                let (before, created) = self.create_or_update_resolved_link(storage, &after)?;
                changes.push((before, Some(created)));
            }
            (Some(before), Some(after)) => {
                if before.index == after.index && storage.exists(before.index) {
                    let before_link = storage.get_link(before.index).unwrap();
                    if before_link.source != after.source || before_link.target != after.target {
                        storage.update(before.index, after.source, after.target)?;
                    }
                    if let Some(name) = &after.name {
                        storage.set_name(before.index, name)?;
                    }
                    // The update can be resolved into a merge, which deletes
                    // `before.index`; report the state the query asked for and
                    // let `restore_unexpected_deletions` put the link back,
                    // exactly as the C# processor does.
                    let after_link = storage
                        .get_link(before.index)
                        .unwrap_or_else(|| Link::new(before.index, after.source, after.target));
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

    fn links_matching_definition(
        &self,
        storage: &mut impl NamedTypeLinks,
        definition: &ResolvedLink,
    ) -> Result<Vec<Link>> {
        Ok(storage
            .all_links()
            .into_iter()
            .filter(|link| {
                (definition.index == 0
                    || Self::is_any(definition.index)
                    || link.index == definition.index)
                    && (Self::is_any(definition.source) || link.source == definition.source)
                    && (Self::is_any(definition.target) || link.target == definition.target)
            })
            .collect())
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

    /// Resolves a half a query left unspecified against the half already
    /// stored, the way C# resolves its `any` constant on the way into the
    /// store.
    ///
    /// The C# processor marks an unbound substitution variable — and a `*` — with
    /// `links.Constants.Any`, which is a value the *store* understands:
    /// `Update` leaves a half substituted with `any` exactly as it was, and a
    /// link created from one gets `null` there. This processor marks the same
    /// thing with [`u32::MAX`], which the store underneath does not recognise
    /// (its `any` is `2147483644`, the hybrid-aware constant), so `() (($a $a))`
    /// used to store the literal `4294967295` in both halves where C# stores
    /// `(1: 0 0)`. Resolving at the write boundary keeps [`u32::MAX`] as this
    /// crate's single internal marker while writing what C# writes.
    fn resolve_unspecified(value: u32, existing: u32) -> u32 {
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
    fn search_unspecified(
        storage: &mut impl NamedTypeLinks,
        source: u32,
        target: u32,
    ) -> Option<u32> {
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

    fn is_normal_index(value: u32) -> bool {
        value != 0 && !Self::is_any(value)
    }

    fn is_numeric_or_wildcard(identifier: &str) -> bool {
        identifier == "*" || identifier.parse::<u32>().is_ok()
    }

    /// Simplifies the changes list.
    ///
    /// A missing side — the state before a creation, or the state after a
    /// deletion — becomes the null link `(0: 0 0)` on the way in and turns back
    /// into `None` on the way out. C# has no option type here and feeds the
    /// simplifier `default(Link<uint>)` for both, so routing the null states
    /// around the simplifier (as this used to) both reported creations and
    /// deletions in a different order than C# and hid them from the chain
    /// collapsing that is the whole point of the pass.
    fn simplify_changes_list(
        &self,
        changes: &[(Option<Link>, Option<Link>)],
    ) -> Vec<(Option<Link>, Option<Link>)> {
        let to_simplify: Vec<(Link, Link)> = changes
            .iter()
            .map(|(before, after)| {
                (
                    before.unwrap_or_else(Link::null),
                    after.unwrap_or_else(Link::null),
                )
            })
            .collect();

        simplify_changes(to_simplify)
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
            eprintln!("{}", msg);
        }
    }
}
