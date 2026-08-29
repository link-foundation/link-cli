use anyhow::Result;
use std::collections::HashSet;

use crate::error::LinkError;
use crate::link::Link;
use crate::lino_link::LinoLink;
use crate::named_type_links::NamedTypeLinks;

pub(crate) struct LinkReferenceValidator {
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
    pub(crate) fn new(trace: bool, auto_create_missing_references: bool) -> Self {
        Self {
            trace,
            auto_create_missing_references,
        }
    }

    pub(crate) fn validate_links_exist_or_will_be_created(
        &self,
        storage: &mut impl NamedTypeLinks,
        restriction_patterns: &[LinoLink],
        substitution_patterns: &[LinoLink],
    ) -> Result<Vec<(Link, Link)>> {
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
        )?;
        self.collect_missing_references(
            storage,
            &mut plan,
            substitution_patterns,
            true,
            "substitution",
        )?;

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

        let created = self.auto_create_missing_references(storage, &plan)?;
        self.trace_msg("[ValidateLinksExistOrWillBeCreated] Validation completed");
        Ok(created)
    }

    fn build_link_reference_plan(
        &self,
        storage: &mut impl NamedTypeLinks,
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

        for pattern in substitution_patterns {
            Self::collect_composite_pairs(pattern, &mut plan);
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

    fn collect_composite_pairs(pattern: &LinoLink, plan: &mut LinkReferencePlan) {
        if Self::is_composite_lino(pattern)
            && Self::concrete_identifier(pattern.id.as_deref()).is_some()
        {
            if let Some(values) = &pattern.values {
                if let (Some(source), Some(target)) = (
                    Self::concrete_numeric_identifier(values[0].id.as_deref()),
                    Self::concrete_numeric_identifier(values[1].id.as_deref()),
                ) {
                    plan.composite_pairs_to_be_created.insert((source, target));
                }
            }
        }

        if let Some(values) = &pattern.values {
            for sub_pattern in values {
                Self::collect_composite_pairs(sub_pattern, plan);
            }
        }
    }

    fn collect_implicit_definitions(
        &self,
        storage: &mut impl NamedTypeLinks,
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

    fn next_available_link_id(
        storage: &mut impl NamedTypeLinks,
        reserved_numeric_ids: &HashSet<u32>,
    ) -> u32 {
        let mut next_id = 1;
        while storage.exists(next_id) || reserved_numeric_ids.contains(&next_id) {
            next_id += 1;
        }
        next_id
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

    /// Creates every missing reference and reports the `(before, after)` state
    /// of each one.
    ///
    /// The before state is the placeholder the reference is turned into a point
    /// link *from*, never `null`: both branches of the C# original create the
    /// link silently — `EnsureCreated`, and `CreateAndUpdate(Null, Null)` with
    /// no handler — and only pass the changes handler to the `Update` that
    /// makes it a point link:
    ///
    /// ```csharp
    /// links.Update(
    ///   new DoubletLink(linkId, links.Constants.Null, links.Constants.Null),
    ///   new DoubletLink(linkId, linkId, linkId),
    ///   (beforeState, afterState) =>
    ///       options.ChangesHandler?.Invoke(beforeState, afterState) ?? links.Constants.Continue
    /// );
    /// ```
    ///
    /// So `--changes` shows `((2: 0 0)) ((2: 2 2))`, not `() ((2: 2 2))`.
    fn auto_create_missing_references(
        &self,
        storage: &mut impl NamedTypeLinks,
        plan: &LinkReferencePlan,
    ) -> Result<Vec<(Link, Link)>> {
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
            if plan
                .composite_pairs_to_be_created
                .contains(&(link_id, link_id))
            {
                self.trace_msg(&format!(
                    "[ValidateLinksExistOrWillBeCreated] Link {link_id} exists as a placeholder because ({link_id}, {link_id}) is defined by the substitution."
                ));
                continue;
            }
            let before = storage
                .get_link(link_id)
                .unwrap_or_else(|| Link::new(link_id, 0, 0));
            storage.update(link_id, link_id, link_id)?;
            if let Some(after) = storage.get_link(link_id) {
                created.push((before, after));
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
            if storage.get_by_name(&name)?.is_some() {
                continue;
            }

            self.trace_msg(&format!(
                "[ValidateLinksExistOrWillBeCreated] Auto-creating missing named reference '{name}' as point link."
            ));
            let link_id = storage.get_or_create_named(&name)?;
            if let Some(after) = storage.get_link(link_id) {
                created.push((Link::new(link_id, 0, 0), after));
            }
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
