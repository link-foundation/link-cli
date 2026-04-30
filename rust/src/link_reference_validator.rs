use anyhow::Result;
use std::collections::HashSet;

use crate::error::LinkError;
use crate::link::Link;
use crate::link_storage::LinkStorage;
use crate::lino_link::LinoLink;

pub(crate) struct LinkReferenceValidator {
    trace: bool,
    auto_create_missing_references: bool,
}

#[derive(Debug, Default)]
struct LinkReferencePlan {
    numeric_ids_to_be_created: HashSet<u32>,
    names_to_be_created: HashSet<String>,
    missing_references: Vec<MissingLinkReference>,
    missing_reference_keys: HashSet<String>,
}

impl LinkReferencePlan {
    fn add_missing_reference(&mut self, reference: MissingLinkReference) {
        let key = reference.key();
        if self.missing_reference_keys.insert(key) {
            self.missing_references.push(reference);
        }
    }
}

#[derive(Debug, Clone)]
struct MissingLinkReference {
    identifier: String,
    pattern_type: &'static str,
    numeric_id: Option<u32>,
}

impl MissingLinkReference {
    fn key(&self) -> String {
        self.numeric_id
            .map(|id| format!("id:{id}"))
            .unwrap_or_else(|| format!("name:{}", self.identifier))
    }
}

impl LinkReferenceValidator {
    pub(crate) fn new(trace: bool, auto_create_missing_references: bool) -> Self {
        Self {
            trace,
            auto_create_missing_references,
        }
    }

    pub(crate) fn validate_links_exist_or_will_be_created(
        &self,
        storage: &mut LinkStorage,
        restriction_patterns: &[LinoLink],
        substitution_patterns: &[LinoLink],
    ) -> Result<Vec<Link>> {
        self.trace_msg("[ValidateLinksExistOrWillBeCreated] Starting validation");

        let mut plan = self.build_link_reference_plan(storage, substitution_patterns);
        self.trace_msg(&format!(
            "[ValidateLinksExistOrWillBeCreated] Numeric links to be created: {:?}",
            plan.numeric_ids_to_be_created
        ));
        self.trace_msg(&format!(
            "[ValidateLinksExistOrWillBeCreated] Named links to be created: {:?}",
            plan.names_to_be_created
        ));

        self.collect_missing_references(
            storage,
            &mut plan,
            restriction_patterns,
            false,
            "restriction",
        );
        self.collect_missing_references(
            storage,
            &mut plan,
            substitution_patterns,
            true,
            "substitution",
        );

        if plan.missing_references.is_empty() {
            self.trace_msg("[ValidateLinksExistOrWillBeCreated] Validation completed");
            return Ok(vec![]);
        }

        if !self.auto_create_missing_references {
            let missing = &plan.missing_references[0];
            return Err(LinkError::QueryError(format!(
                "Invalid reference to non-existent link '{}' in {} pattern. Link '{}' does not exist and will not be created by this operation. Use --auto-create-missing-references to create missing references as point links.",
                missing.identifier, missing.pattern_type, missing.identifier
            ))
            .into());
        }

        let created = self.auto_create_missing_references(storage, &plan.missing_references)?;
        self.trace_msg("[ValidateLinksExistOrWillBeCreated] Validation completed");
        Ok(created)
    }

    fn build_link_reference_plan(
        &self,
        storage: &LinkStorage,
        substitution_patterns: &[LinoLink],
    ) -> LinkReferencePlan {
        let mut plan = LinkReferencePlan::default();
        let mut reserved_numeric_ids = HashSet::new();

        for pattern in substitution_patterns {
            self.collect_explicit_definitions(pattern, &mut plan, &mut reserved_numeric_ids);
        }

        for pattern in substitution_patterns {
            self.collect_implicit_definitions(
                storage,
                pattern,
                &mut plan,
                &mut reserved_numeric_ids,
            );
        }

        plan
    }

    fn collect_explicit_definitions(
        &self,
        pattern: &LinoLink,
        plan: &mut LinkReferencePlan,
        reserved_numeric_ids: &mut HashSet<u32>,
    ) {
        if Self::is_composite_lino(pattern) {
            if let Some(identifier) = Self::concrete_identifier(pattern.id.as_deref()) {
                if let Ok(link_id) = identifier.parse::<u32>() {
                    plan.numeric_ids_to_be_created.insert(link_id);
                    reserved_numeric_ids.insert(link_id);
                } else {
                    plan.names_to_be_created.insert(identifier);
                }
            }
        }

        if let Some(values) = &pattern.values {
            for sub_pattern in values {
                self.collect_explicit_definitions(sub_pattern, plan, reserved_numeric_ids);
            }
        }
    }

    fn collect_implicit_definitions(
        &self,
        storage: &LinkStorage,
        pattern: &LinoLink,
        plan: &mut LinkReferencePlan,
        reserved_numeric_ids: &mut HashSet<u32>,
    ) {
        if let Some(values) = &pattern.values {
            for sub_pattern in values {
                self.collect_implicit_definitions(storage, sub_pattern, plan, reserved_numeric_ids);
            }
        }

        if Self::is_composite_lino(pattern)
            && Self::concrete_identifier(pattern.id.as_deref()).is_none()
        {
            let next_id = Self::next_available_link_id(storage, reserved_numeric_ids);
            reserved_numeric_ids.insert(next_id);
            plan.numeric_ids_to_be_created.insert(next_id);
        }
    }

    fn next_available_link_id(storage: &LinkStorage, reserved_numeric_ids: &HashSet<u32>) -> u32 {
        let mut next_id = 1;
        while storage.exists(next_id) || reserved_numeric_ids.contains(&next_id) {
            next_id += 1;
        }
        next_id
    }

    fn collect_missing_references(
        &self,
        storage: &LinkStorage,
        plan: &mut LinkReferencePlan,
        patterns: &[LinoLink],
        is_substitution: bool,
        pattern_type: &'static str,
    ) {
        for pattern in patterns {
            self.collect_missing_references_in_pattern(
                storage,
                plan,
                pattern,
                is_substitution,
                pattern_type,
            );
        }
    }

    fn collect_missing_references_in_pattern(
        &self,
        storage: &LinkStorage,
        plan: &mut LinkReferencePlan,
        pattern: &LinoLink,
        is_substitution: bool,
        pattern_type: &'static str,
    ) {
        let pattern_id_is_definition = is_substitution
            && Self::is_composite_lino(pattern)
            && Self::concrete_identifier(pattern.id.as_deref()).is_some();

        if !pattern_id_is_definition {
            if let Some(identifier) = Self::concrete_identifier(pattern.id.as_deref()) {
                self.validate_reference_identifier(storage, plan, &identifier, pattern_type);
            }
        }

        if let Some(values) = &pattern.values {
            for sub_pattern in values {
                self.collect_missing_references_in_pattern(
                    storage,
                    plan,
                    sub_pattern,
                    is_substitution,
                    pattern_type,
                );
            }
        }
    }

    fn validate_reference_identifier(
        &self,
        storage: &LinkStorage,
        plan: &mut LinkReferencePlan,
        identifier: &str,
        pattern_type: &'static str,
    ) {
        if let Ok(link_id) = identifier.parse::<u32>() {
            if !storage.exists(link_id) && !plan.numeric_ids_to_be_created.contains(&link_id) {
                plan.add_missing_reference(MissingLinkReference {
                    identifier: identifier.to_string(),
                    pattern_type,
                    numeric_id: Some(link_id),
                });
                return;
            }
            self.trace_msg(&format!(
                "[ValidateReferencesInPattern] Link {link_id} reference validated in {pattern_type} pattern"
            ));
            return;
        }

        if storage.get_by_name(identifier).is_none()
            && !plan.names_to_be_created.contains(identifier)
        {
            plan.add_missing_reference(MissingLinkReference {
                identifier: identifier.to_string(),
                pattern_type,
                numeric_id: None,
            });
            return;
        }

        self.trace_msg(&format!(
            "[ValidateReferencesInPattern] Named link '{identifier}' reference validated in {pattern_type} pattern"
        ));
    }

    fn auto_create_missing_references(
        &self,
        storage: &mut LinkStorage,
        missing_references: &[MissingLinkReference],
    ) -> Result<Vec<Link>> {
        let mut created = Vec::new();
        let mut numeric_references = missing_references
            .iter()
            .filter_map(|reference| reference.numeric_id)
            .collect::<Vec<_>>();
        numeric_references.sort_unstable();
        numeric_references.dedup();

        for link_id in numeric_references {
            if storage.exists(link_id) {
                continue;
            }

            self.trace_msg(&format!(
                "[ValidateLinksExistOrWillBeCreated] Auto-creating missing numeric reference {link_id} as point link."
            ));
            storage.ensure_created(link_id);
            storage.update(link_id, link_id, link_id)?;
            if let Some(link) = storage.get(link_id) {
                created.push(*link);
            }
        }

        let mut named_references = missing_references
            .iter()
            .filter(|reference| reference.numeric_id.is_none())
            .map(|reference| reference.identifier.clone())
            .collect::<Vec<_>>();
        named_references.sort();
        named_references.dedup();

        for name in named_references {
            if storage.get_by_name(&name).is_some() {
                continue;
            }

            self.trace_msg(&format!(
                "[ValidateLinksExistOrWillBeCreated] Auto-creating missing named reference '{name}' as point link."
            ));
            let link_id = storage.get_or_create_named(&name);
            if let Some(link) = storage.get(link_id) {
                created.push(*link);
            }
        }

        Ok(created)
    }

    fn is_composite_lino(lino_link: &LinoLink) -> bool {
        lino_link.values_count() == 2
    }

    fn concrete_identifier(id: Option<&str>) -> Option<String> {
        let identifier = id?.trim_end_matches(':');
        if identifier.is_empty() || identifier == "*" || identifier.starts_with('$') {
            None
        } else {
            Some(identifier.to_string())
        }
    }

    fn trace_msg(&self, msg: &str) {
        if self.trace {
            eprintln!("{}", msg);
        }
    }
}
