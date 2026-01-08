//! QueryProcessor - Handles LiNo query parsing and execution
//!
//! This module provides the QueryProcessor for processing LiNo queries.
//! Corresponds to BasicQueryProcessor, MixedQueryProcessor, and AdvancedMixedQueryProcessor in C#

use anyhow::Result;
use std::collections::HashMap;

use crate::changes_simplifier::simplify_changes;
use crate::error::LinkError;
use crate::link::Link;
use crate::link_storage::LinkStorage;
use crate::lino_link::LinoLink;
use crate::parser::Parser;

/// Options for query processing
pub struct QueryOptions {
    pub query: String,
    pub trace: bool,
}

impl QueryOptions {
    pub fn new(query: &str, trace: bool) -> Self {
        Self {
            query: query.to_string(),
            trace,
        }
    }
}

/// QueryProcessor handles LiNo query parsing and execution
/// Corresponds to AdvancedMixedQueryProcessor in C#
pub struct QueryProcessor {
    trace: bool,
}

impl QueryProcessor {
    /// Creates a new QueryProcessor
    pub fn new(trace: bool) -> Self {
        Self { trace }
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

        // We expect something like (( restriction ) ( substitution ))
        let outer_link = &parsed_links[0];
        let outer_values = match &outer_link.values {
            Some(v) if v.len() >= 2 => v,
            _ => {
                self.trace_msg("[ProcessQuery] Outer link has fewer than 2 sub-links, returning.");
                return Ok(vec![]);
            }
        };

        let restriction_link = &outer_values[0];
        let substitution_link = &outer_values[1];

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
        if restriction_link.values_count() == 0 && substitution_link.values_count() == 0 {
            self.trace_msg(
                "[ProcessQuery] Restriction & substitution both empty => no operation, returning.",
            );
            return Ok(vec![]);
        }

        // Creation scenario: no restriction, only substitution
        if restriction_link.values_count() == 0 && substitution_link.values_count() > 0 {
            self.trace_msg(
                "[ProcessQuery] No restriction, but substitution is non-empty => creation scenario.",
            );
            if let Some(values) = &substitution_link.values {
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
        if restriction_link.values_count() > 0 && substitution_link.values_count() == 0 {
            self.trace_msg(
                "[ProcessQuery] Restriction non-empty, substitution empty => deletion scenario.",
            );
            if let Some(values) = &restriction_link.values {
                for link_to_delete in values {
                    let delete_id = self.resolve_link_id(storage, link_to_delete)?;
                    if delete_id != 0 && storage.exists(delete_id) {
                        let before = storage.delete(delete_id)?;
                        changes_list.push((Some(before), None));
                        self.trace_msg(&format!("[ProcessQuery] Deleted link ID #{}.", delete_id));
                    }
                }
            }
            storage.save()?;
            return Ok(changes_list);
        }

        // Update/Mixed scenario: both restriction and substitution have values
        self.trace_msg(
            "[ProcessQuery] Both restriction and substitution non-empty => update/mixed scenario.",
        );

        // Build dictionaries for restriction and substitution links
        let restriction_links = self.build_links_by_id(restriction_link);
        let substitution_links = self.build_links_by_id(substitution_link);

        // Collect variable assignments from restriction links
        let mut variable_assignments: HashMap<String, u32> = HashMap::new();

        // First pass: resolve restriction links to extract variable values
        for lino_link in restriction_links.values() {
            if lino_link.values_count() == 2 {
                if let Some(ref link_id) = lino_link.id {
                    if let Ok(numeric_id) = link_id.parse::<u32>() {
                        if storage.exists(numeric_id) {
                            let actual_link = storage.get(numeric_id).unwrap();
                            if let Some(values) = &lino_link.values {
                                self.assign_variable(
                                    &values[0].id,
                                    actual_link.source,
                                    &mut variable_assignments,
                                );
                                self.assign_variable(
                                    &values[1].id,
                                    actual_link.target,
                                    &mut variable_assignments,
                                );
                            }
                        }
                    }
                }
            }
        }

        // Get all unique IDs
        let mut all_ids: Vec<String> = restriction_links
            .keys()
            .chain(substitution_links.keys())
            .cloned()
            .collect();
        all_ids.sort();
        all_ids.dedup();

        // Process each ID
        for id in &all_ids {
            let has_restriction = restriction_links.contains_key(id);
            let has_substitution = substitution_links.contains_key(id);

            if has_restriction && has_substitution {
                // Update operation
                let restriction_lino = &restriction_links[id];
                let substitution_lino = &substitution_links[id];

                let restriction_doublet =
                    self.to_doublet_link(storage, restriction_lino, &variable_assignments, true)?;
                let substitution_doublet =
                    self.to_doublet_link(storage, substitution_lino, &variable_assignments, false)?;

                if restriction_doublet.index != 0 && storage.exists(restriction_doublet.index) {
                    let before = *storage.get(restriction_doublet.index).unwrap();
                    storage.update(
                        restriction_doublet.index,
                        substitution_doublet.source,
                        substitution_doublet.target,
                    )?;
                    if let Some(after) = storage.get(restriction_doublet.index) {
                        changes_list.push((Some(before), Some(*after)));
                    }
                }
            } else if has_restriction && !has_substitution {
                // Delete operation
                let restriction_lino = &restriction_links[id];
                let restriction_doublet =
                    self.to_doublet_link(storage, restriction_lino, &variable_assignments, true)?;

                if restriction_doublet.index != 0 && storage.exists(restriction_doublet.index) {
                    let before = storage.delete(restriction_doublet.index)?;
                    changes_list.push((Some(before), None));
                }
            } else if !has_restriction && has_substitution {
                // Create operation
                let substitution_lino = &substitution_links[id];
                let created_id = self.ensure_link_created(storage, substitution_lino)?;
                if let Some(link) = storage.get(created_id) {
                    changes_list.push((None, Some(*link)));
                }
            }
        }

        storage.save()?;

        // Simplify changes
        let simplified = self.simplify_changes_list(&changes_list);

        Ok(simplified)
    }

    /// Builds a map of links by their ID
    fn build_links_by_id(&self, lino_link: &LinoLink) -> HashMap<String, LinoLink> {
        let mut result = HashMap::new();

        if let Some(values) = &lino_link.values {
            for value in values {
                if let Some(ref id) = value.id {
                    result.insert(id.clone(), value.clone());
                }
            }
        }

        if let Some(ref id) = lino_link.id {
            result.insert(id.clone(), lino_link.clone());
        }

        result
    }

    /// Assigns a variable value if the identifier is a variable
    fn assign_variable(
        &self,
        id: &Option<String>,
        value: u32,
        assignments: &mut HashMap<String, u32>,
    ) {
        if let Some(ref id) = id {
            if id.starts_with('$') && value != 0 {
                assignments.insert(id.clone(), value);
            }
        }
    }

    /// Converts a LinoLink to a Link
    fn to_doublet_link(
        &self,
        storage: &mut LinkStorage,
        lino_link: &LinoLink,
        variable_assignments: &HashMap<String, u32>,
        use_any_default: bool,
    ) -> Result<Link> {
        let default_value = if use_any_default { u32::MAX } else { 0 };

        let mut index = default_value;
        let mut source = default_value;
        let mut target = default_value;

        // Parse index
        if let Some(ref id) = lino_link.id {
            index = self.resolve_id(storage, id, variable_assignments, default_value)?;
        }

        // Parse source and target
        if let Some(ref values) = lino_link.values {
            if values.len() >= 2 {
                if let Some(ref source_id) = values[0].id {
                    source =
                        self.resolve_id(storage, source_id, variable_assignments, default_value)?;
                }
                if let Some(ref target_id) = values[1].id {
                    target =
                        self.resolve_id(storage, target_id, variable_assignments, default_value)?;
                }
            }
        }

        Ok(Link::new(index, source, target))
    }

    /// Resolves an ID string to a numeric value
    fn resolve_id(
        &self,
        storage: &mut LinkStorage,
        id: &str,
        variable_assignments: &HashMap<String, u32>,
        default_value: u32,
    ) -> Result<u32> {
        if id.is_empty() {
            return Ok(default_value);
        }

        if id == "*" {
            return Ok(u32::MAX); // ANY constant
        }

        // Check if it's a variable
        if id.starts_with('$') {
            if let Some(&value) = variable_assignments.get(id) {
                return Ok(value);
            }
            return Ok(default_value);
        }

        // Try to parse as number
        if let Ok(num) = id.parse::<u32>() {
            return Ok(num);
        }

        // Try to resolve as name
        if let Some(link_id) = storage.get_by_name(id) {
            return Ok(link_id);
        }

        // Create as name if not found
        Ok(storage.get_or_create_named(id))
    }

    /// Resolves the ID of a LinoLink
    fn resolve_link_id(&self, storage: &mut LinkStorage, lino_link: &LinoLink) -> Result<u32> {
        let empty_map = HashMap::new();
        if let Some(ref id) = lino_link.id {
            self.resolve_id(storage, id, &empty_map, 0)
        } else {
            Ok(0)
        }
    }

    /// Ensures a link is created from a LinoLink pattern
    fn ensure_link_created(&self, storage: &mut LinkStorage, lino_link: &LinoLink) -> Result<u32> {
        // Handle leaf nodes (names or numbers)
        if !lino_link.has_values() {
            if let Some(ref id) = lino_link.id {
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
