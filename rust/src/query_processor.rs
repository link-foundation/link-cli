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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_query_processor_create() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let processor = QueryProcessor::new(false);

        // Create a simple link: (() ((1 2)))
        let changes = processor.process_query(&mut storage, "(()((1 2)))")?;

        assert!(!changes.is_empty());
        assert!(changes[0].0.is_none()); // No before (creation)
        assert!(changes[0].1.is_some()); // Has after

        Ok(())
    }

    #[test]
    fn test_query_processor_empty() -> Result<()> {
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let processor = QueryProcessor::new(false);

        let changes = processor.process_query(&mut storage, "")?;
        assert!(changes.is_empty());

        Ok(())
    }

    // ============================================
    // Link Deduplication Tests (Issue #65)
    // ============================================

    #[test]
    fn test_deduplicate_duplicate_pair_with_named_links() -> Result<()> {
        // Issue #65: Test deduplication of (m a) (m a) pattern
        // Query: () (((m a) (m a)))
        // Expected: m, a (named self-refs), link for (m a), link for ((m a) (m a))
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let processor = QueryProcessor::new(false);

        processor.process_query(&mut storage, "(() (((m a) (m a))))")?;

        let all_links = storage.all();
        assert_eq!(all_links.len(), 4);

        // Get the named link IDs
        let m_id = storage.get_by_name("m").expect("m should exist");
        let a_id = storage.get_by_name("a").expect("a should exist");

        // m and a should be self-referencing
        let m_link = storage.get(m_id).unwrap();
        assert_eq!(m_link.source, m_id);
        assert_eq!(m_link.target, m_id);

        let a_link = storage.get(a_id).unwrap();
        assert_eq!(a_link.source, a_id);
        assert_eq!(a_link.target, a_id);

        // Find the (m a) link
        let ma_id = storage.search(m_id, a_id).expect("(m a) link should exist");

        // Find the outer link ((m a) (m a)) - should have same source and target
        let outer_id = storage.search(ma_id, ma_id).expect("((m a) (m a)) link should exist");
        let outer_link = storage.get(outer_id).unwrap();
        assert_eq!(outer_link.source, outer_link.target, "Outer link should reference the same deduplicated sub-link");

        Ok(())
    }

    #[test]
    fn test_deduplicate_duplicate_pair_with_numeric_links() -> Result<()> {
        // Issue #65: Test deduplication with numeric IDs
        // Query: () (((1 2) (1 2)))
        // When using numeric IDs directly, they are treated as references (not creating self-refs)
        // So (1 2) creates link with source=1, target=2
        // The deduplication still works: ((1 2) (1 2)) creates only one (1 2) link
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let processor = QueryProcessor::new(false);

        processor.process_query(&mut storage, "(() (((1 2) (1 2))))")?;

        let all_links = storage.all();

        // Should have 2 links: (1 2) and ((1 2) (1 2))
        assert_eq!(all_links.len(), 2);

        // Link 1 should be (1 2) - the deduplicated sub-link
        let link1 = storage.get(1).expect("Link 1 should exist");
        assert_eq!(link1.source, 1);
        assert_eq!(link1.target, 2);

        // Link 2 should be (1 1) - referencing the same sub-link twice
        let link2 = storage.get(2).expect("Link 2 should exist");
        assert_eq!(link2.source, 1);
        assert_eq!(link2.target, 1);

        Ok(())
    }

    #[test]
    fn test_deduplicate_triple_duplicate_pair() -> Result<()> {
        // Test with three identical pairs using named links: (((a b) ((a b) (a b))))
        // The (a b) should only be created once
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let processor = QueryProcessor::new(false);

        processor.process_query(&mut storage, "(() (((a b) ((a b) (a b)))))")?;

        let all_links = storage.all();
        assert_eq!(all_links.len(), 5);

        let a_id = storage.get_by_name("a").expect("a should exist");
        let b_id = storage.get_by_name("b").expect("b should exist");

        // a and b should be self-referencing
        let a_link = storage.get(a_id).unwrap();
        assert_eq!(a_link.source, a_id);
        assert_eq!(a_link.target, a_id);

        let b_link = storage.get(b_id).unwrap();
        assert_eq!(b_link.source, b_id);
        assert_eq!(b_link.target, b_id);

        // Find (a b) link - the deduplicated sub-link
        let ab_id = storage.search(a_id, b_id).expect("(a b) link should exist");

        // Find ((a b) (a b)) - should reference (a b) twice
        let inner_id = storage.search(ab_id, ab_id).expect("((a b) (a b)) link should exist");

        // Find outer link ((a b) ((a b) (a b)))
        let outer_id = storage.search(ab_id, inner_id).expect("outer link should exist");
        assert!(outer_id > 0);

        Ok(())
    }

    #[test]
    fn test_deduplicate_with_different_pairs() -> Result<()> {
        // Test that different pairs are NOT deduplicated
        // Query: () (((a b) (b a))) - using named links
        // (a b) and (b a) are different and should both be created
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let processor = QueryProcessor::new(false);

        processor.process_query(&mut storage, "(() (((a b) (b a))))")?;

        let all_links = storage.all();
        assert_eq!(all_links.len(), 5);

        let a_id = storage.get_by_name("a").expect("a should exist");
        let b_id = storage.get_by_name("b").expect("b should exist");

        // a and b should be self-referencing
        let a_link = storage.get(a_id).unwrap();
        assert_eq!(a_link.source, a_id);
        assert_eq!(a_link.target, a_id);

        let b_link = storage.get(b_id).unwrap();
        assert_eq!(b_link.source, b_id);
        assert_eq!(b_link.target, b_id);

        // Find (a b) link
        let ab_id = storage.search(a_id, b_id).expect("(a b) link should exist");

        // Find (b a) link
        let ba_id = storage.search(b_id, a_id).expect("(b a) link should exist");

        // Find outer link ((a b) (b a)) - should have different source and target
        let outer_id = storage.search(ab_id, ba_id).expect("outer link should exist");
        let outer_link = storage.get(outer_id).unwrap();
        assert_ne!(outer_link.source, outer_link.target);

        Ok(())
    }

    #[test]
    fn test_deduplicate_nested_duplicates() -> Result<()> {
        // Test deeply nested deduplication using named links
        // Query: () ((((x y) (x y)) ((x y) (x y))))
        // (x y) is duplicated at multiple levels
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let processor = QueryProcessor::new(false);

        processor.process_query(&mut storage, "(() ((((x y) (x y)) ((x y) (x y)))))")?;

        let all_links = storage.all();
        assert_eq!(all_links.len(), 5);

        let x_id = storage.get_by_name("x").expect("x should exist");
        let y_id = storage.get_by_name("y").expect("y should exist");

        // x and y should be self-referencing
        let x_link = storage.get(x_id).unwrap();
        assert_eq!(x_link.source, x_id);
        assert_eq!(x_link.target, x_id);

        let y_link = storage.get(y_id).unwrap();
        assert_eq!(y_link.source, y_id);
        assert_eq!(y_link.target, y_id);

        // Find (x y) - the base link
        let xy_id = storage.search(x_id, y_id).expect("(x y) link should exist");

        // Find ((x y) (x y)) - references (x y) twice (deduplicated)
        let level1_id = storage.search(xy_id, xy_id).expect("((x y) (x y)) link should exist");

        // Find (((x y) (x y)) ((x y) (x y))) - references level1 twice (deduplicated)
        let level2_id = storage.search(level1_id, level1_id).expect("outer link should exist");
        assert!(level2_id > 0);

        Ok(())
    }

    #[test]
    fn test_deduplicate_named_links_multiple_queries() -> Result<()> {
        // Issue #65: Verify that named links maintain consistent IDs across operations
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let processor = QueryProcessor::new(false);

        // First create named links
        processor.process_query(&mut storage, "(() ((p: p p)))")?;
        processor.process_query(&mut storage, "(() ((a: a a)))")?;

        let p_id = storage.get_by_name("p").expect("p should exist");
        let a_id = storage.get_by_name("a").expect("a should exist");

        // Now create ((p a) (p a)) - should reuse existing p and a
        processor.process_query(&mut storage, "(() (((p a) (p a))))")?;

        // p and a should still have the same IDs
        assert_eq!(storage.get_by_name("p"), Some(p_id));
        assert_eq!(storage.get_by_name("a"), Some(a_id));

        // Verify p and a are still self-referencing
        let p_link = storage.get(p_id).unwrap();
        assert_eq!(p_link.source, p_id);
        assert_eq!(p_link.target, p_id);

        let a_link = storage.get(a_id).unwrap();
        assert_eq!(a_link.source, a_id);
        assert_eq!(a_link.target, a_id);

        // Find (p a) link
        let pa_id = storage.search(p_id, a_id).expect("(p a) link should exist");

        // Find ((p a) (p a)) link - should reference pa_id twice
        let outer_id = storage.search(pa_id, pa_id).expect("((p a) (p a)) link should exist");
        let outer_link = storage.get(outer_id).unwrap();
        assert_eq!(outer_link.source, pa_id);
        assert_eq!(outer_link.target, pa_id);

        Ok(())
    }

    #[test]
    fn test_deduplicate_mixed_named_and_numeric() -> Result<()> {
        // Test that named links are reused across queries
        let temp_file = NamedTempFile::new()?;
        let db_path = temp_file.path().to_str().unwrap();

        let mut storage = LinkStorage::new(db_path, false)?;
        let processor = QueryProcessor::new(false);

        // First query creates (m a)
        processor.process_query(&mut storage, "(() ((m a)))")?;

        let m_id = storage.get_by_name("m").expect("m should exist");
        let a_id = storage.get_by_name("a").expect("a should exist");

        // Second query should reuse existing m and a links
        processor.process_query(&mut storage, "(() (((m a) (m a))))")?;

        // m and a should still have the same IDs
        assert_eq!(storage.get_by_name("m"), Some(m_id));
        assert_eq!(storage.get_by_name("a"), Some(a_id));

        // Should have 4 links total: m, a, (m a), ((m a) (m a))
        let all_links = storage.all();
        assert_eq!(all_links.len(), 4);

        Ok(())
    }
}
