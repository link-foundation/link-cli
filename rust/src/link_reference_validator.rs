//! Checking that every link a query refers to exists, and creating the ones
//! that do not when the caller asked for that.
//!
//! Ported from the C# `LinkReferenceValidator`, and public for the same reason
//! the query processor is: a custom front end that resolves references its own
//! way needs to be able to reuse, or replace, exactly this step.

use anyhow::Result;
use std::collections::HashSet;

use crate::error::LinkError;
use crate::link::Link;
use crate::lino_link::LinoLink;
use crate::named_type_links::NamedTypeLinks;
use crate::query_processor::Changes;

pub struct LinkReferenceValidator {
    trace: bool,
    auto_create_missing_references: bool,
}

#[derive(Debug, Default)]
struct LinkReferencePlan {
    numeric_ids_to_be_created: HashSet<u32>,
    names_to_be_created: HashSet<String>,
    /// `(source, target)` pairs the substitution itself defines.
    ///
    /// A missing numeric reference whose own point pair `(id, id)` appears
    /// here is left as a `(id: 0 0)` placeholder instead of being turned into
    /// a point link, so that the substitution which is about to write that
    /// exact pair does not collide with it under uniqueness resolution.
    composite_pairs_to_be_created: HashSet<(u32, u32)>,
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
    pub fn new(trace: bool, auto_create_missing_references: bool) -> Self {
        Self {
            trace,
            auto_create_missing_references,
        }
    }

    pub fn validate_links_exist_or_will_be_created(
        &self,
        storage: &mut impl NamedTypeLinks,
        restriction_patterns: &[LinoLink],
        substitution_patterns: &[LinoLink],
    ) -> Result<Changes> {
        self.trace_msg("[ValidateLinksExistOrWillBeCreated] Starting validation");

        let mut created = Vec::new();
        loop {
            let plan =
                self.plan_references(storage, restriction_patterns, substitution_patterns)?;
            if plan.missing_references.is_empty() {
                self.trace_msg("[ValidateLinksExistOrWillBeCreated] Validation completed");
                return Ok(created);
            }

            if !self.auto_create_missing_references {
                let missing = &plan.missing_references[0];
                return Err(LinkError::QueryError(format!(
                    "Invalid reference to non-existent link '{}' in {} pattern. Link '{}' does not exist and will not be created by this operation. Use --auto-create-missing-references to create missing references as point links.",
                    missing.identifier, missing.pattern_type, missing.identifier
                ))
                .into());
            }

            // Creating a reference frees addresses on the way, which changes
            // the ones the query's new links get, so the plan is made again
            // until every reference is accounted for.
            created.extend(self.auto_create_missing_references(storage, &plan)?);
        }
    }

    fn plan_references(
        &self,
        storage: &mut impl NamedTypeLinks,
        restriction_patterns: &[LinoLink],
        substitution_patterns: &[LinoLink],
    ) -> Result<LinkReferencePlan> {
        let mut plan = self.build_link_reference_plan(storage, substitution_patterns)?;
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
        )?;
        self.collect_missing_references(
            storage,
            &mut plan,
            substitution_patterns,
            true,
            "substitution",
        )?;
        Ok(plan)
    }

    fn build_link_reference_plan(
        &self,
        storage: &mut impl NamedTypeLinks,
        substitution_patterns: &[LinoLink],
    ) -> Result<LinkReferencePlan> {
        let mut plan = LinkReferencePlan::default();
        let mut anonymous_links = 0;
        for pattern in substitution_patterns {
            Self::collect_definitions(pattern, &mut plan, &mut anonymous_links);
        }
        plan.numeric_ids_to_be_created
            .extend(Self::next_created_addresses(storage, anonymous_links)?);
        Ok(plan)
    }

    /// Collects the ids and names the substitution defines, the `(source,
    /// target)` pairs it writes under them, and how many links it writes
    /// without an id.
    fn collect_definitions(
        pattern: &LinoLink,
        plan: &mut LinkReferencePlan,
        anonymous_links: &mut usize,
    ) {
        if let Some(values) = &pattern.values {
            for sub_pattern in values {
                Self::collect_definitions(sub_pattern, plan, anonymous_links);
            }
        }
        if !Self::is_composite_lino(pattern) {
            return;
        }
        let Some(identifier) = Self::concrete_identifier(pattern.id.as_deref()) else {
            *anonymous_links += 1;
            return;
        };
        match identifier.parse::<u32>() {
            Ok(link_id) => plan.numeric_ids_to_be_created.insert(link_id),
            Err(_) => plan.names_to_be_created.insert(identifier),
        };
        let values = pattern.values.as_deref().unwrap_or_default();
        if let (Some(source), Some(target)) = (
            Self::concrete_numeric_identifier(values[0].id.as_deref()),
            Self::concrete_numeric_identifier(values[1].id.as_deref()),
        ) {
            plan.composite_pairs_to_be_created.insert((source, target));
        }
    }

    /// The addresses the next `count` links created in `storage` get.
    ///
    /// A store reuses the address freed last first, so the lowest free one is
    /// not necessarily next. Creating the links and deleting them again in
    /// reverse leaves the store exactly as it was, and is the one way to ask
    /// any store, a remote one included.
    fn next_created_addresses(storage: &mut impl NamedTypeLinks, count: usize) -> Result<Vec<u32>> {
        let addresses: Vec<u32> = (0..count).map(|_| storage.create(0, 0)).collect();
        for &address in addresses.iter().rev() {
            storage.delete(address)?;
        }
        Ok(addresses)
    }

    fn collect_missing_references(
        &self,
        storage: &mut impl NamedTypeLinks,
        plan: &mut LinkReferencePlan,
        patterns: &[LinoLink],
        is_substitution: bool,
        pattern_type: &'static str,
    ) -> Result<()> {
        for pattern in patterns {
            self.collect_missing_references_in_pattern(
                storage,
                plan,
                pattern,
                is_substitution,
                pattern_type,
            )?;
        }
        Ok(())
    }

    fn collect_missing_references_in_pattern(
        &self,
        storage: &mut impl NamedTypeLinks,
        plan: &mut LinkReferencePlan,
        pattern: &LinoLink,
        is_substitution: bool,
        pattern_type: &'static str,
    ) -> Result<()> {
        let pattern_id_is_definition = is_substitution
            && Self::is_composite_lino(pattern)
            && Self::concrete_identifier(pattern.id.as_deref()).is_some();

        if !pattern_id_is_definition {
            if let Some(identifier) = Self::concrete_identifier(pattern.id.as_deref()) {
                self.validate_reference_identifier(storage, plan, &identifier, pattern_type)?;
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
                )?;
            }
        }
        Ok(())
    }

    fn validate_reference_identifier(
        &self,
        storage: &mut impl NamedTypeLinks,
        plan: &mut LinkReferencePlan,
        identifier: &str,
        pattern_type: &'static str,
    ) -> Result<()> {
        if let Ok(link_id) = identifier.parse::<u32>() {
            if !storage.exists(link_id) && !plan.numeric_ids_to_be_created.contains(&link_id) {
                plan.add_missing_reference(MissingLinkReference {
                    identifier: identifier.to_string(),
                    pattern_type,
                    numeric_id: Some(link_id),
                });
                return Ok(());
            }
            self.trace_msg(&format!(
                "[ValidateReferencesInPattern] Link {link_id} reference validated in {pattern_type} pattern"
            ));
            return Ok(());
        }

        if storage.get_by_name(identifier)?.is_none()
            && !plan.names_to_be_created.contains(identifier)
        {
            plan.add_missing_reference(MissingLinkReference {
                identifier: identifier.to_string(),
                pattern_type,
                numeric_id: None,
            });
            return Ok(());
        }

        self.trace_msg(&format!(
            "[ValidateReferencesInPattern] Named link '{identifier}' reference validated in {pattern_type} pattern"
        ));
        Ok(())
    }

    /// Creates every missing reference as a point link and reports each
    /// creation the way it happens: a numeric reference is first filled in as
    /// the empty `(id: 0 0)` and then pointed at itself, a named one is created
    /// as `(name: name name)` directly. The simplified `--changes` report of
    /// both is the creation of the point link, `() ((2: 2 2))`.
    ///
    /// A numeric reference the substitution defines as `(id: id id)` itself
    /// is only filled in: the query writes the point link.
    fn auto_create_missing_references(
        &self,
        storage: &mut impl NamedTypeLinks,
        plan: &LinkReferencePlan,
    ) -> Result<Changes> {
        let missing_references = &plan.missing_references;
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
                "[ValidateLinksExistOrWillBeCreated] Auto-creating missing numeric reference {link_id}."
            ));
            storage.try_ensure_created(link_id)?;
            let placeholder = Link::new(link_id, 0, 0);
            created.push((None, Some(placeholder)));
            if plan
                .composite_pairs_to_be_created
                .contains(&(link_id, link_id))
            {
                self.trace_msg(&format!(
                    "[ValidateLinksExistOrWillBeCreated] Link {link_id} exists as a placeholder because ({link_id}, {link_id}) is defined by the substitution."
                ));
                continue;
            }
            storage.update(link_id, link_id, link_id)?;
            created.push((Some(placeholder), storage.get_link(link_id)));
        }

        let mut named_references = missing_references
            .iter()
            .filter(|reference| reference.numeric_id.is_none())
            .map(|reference| reference.identifier.clone())
            .collect::<Vec<_>>();
        named_references.sort();
        named_references.dedup();

        for name in named_references {
            if storage.get_by_name(&name)?.is_some() {
                continue;
            }

            self.trace_msg(&format!(
                "[ValidateLinksExistOrWillBeCreated] Auto-creating missing named reference '{name}' as point link."
            ));
            let link_id = storage.get_or_create_named(&name)?;
            created.push((None, storage.get_link(link_id)));
        }

        Ok(created)
    }

    fn is_composite_lino(lino_link: &LinoLink) -> bool {
        lino_link.values_count() == 2
    }

    fn concrete_numeric_identifier(id: Option<&str>) -> Option<u32> {
        Self::concrete_identifier(id).and_then(|identifier| identifier.parse::<u32>().ok())
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
