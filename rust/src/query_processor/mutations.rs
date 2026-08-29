//! Write-side operations for [`QueryProcessor`].
//!
//! Extracted from `query_processor.rs` for issue #100: resolving unspecified
//! substitution halves pushed the file past the 1000-line limit enforced by
//! `rust/scripts/check-file-size.rs`. These are the methods that actually
//! mutate the store — deletion, creation, update and the restore pass that
//! undoes cascades a query did not ask for — mirroring the C# split into
//! `AdvancedMixedQueryProcessor.Mutations.cs`.

use anyhow::Result;

use crate::error::LinkError;
use crate::link::Link;
use crate::lino_link::LinoLink;
use crate::named_type_links::NamedTypeLinks;
use crate::query_types::ResolvedLink;

use super::QueryProcessor;

impl QueryProcessor {
    /// Deletes `id` and appends every resulting change to `changes`.
    ///
    /// A delete cascades into the links that still used the deleted one, and
    /// each of those removals is reported too. It is the direct analogue of
    /// C#'s `RemoveLinks`, which passes the changes handler straight to the
    /// store so the decorator stack reports the cascade:
    ///
    /// ```csharp
    /// links.Delete(link, (before, after) =>
    ///     options.ChangesHandler?.Invoke(before, after) ?? links.Constants.Continue);
    /// ```
    pub(super) fn delete_observed(
        &self,
        storage: &mut impl NamedTypeLinks,
        id: u32,
        changes: &mut Vec<(Option<Link>, Option<Link>)>,
    ) -> Result<Link> {
        let mut observed = Vec::new();
        let deleted = storage.delete_observed(id, &mut |before, after| {
            observed.push((
                (!before.is_null()).then_some(before),
                (!after.is_null()).then_some(after),
            ));
        })?;
        changes.append(&mut observed);
        Ok(deleted)
    }

    /// Final state every planned operation asks for, keyed by link address
    /// and kept in the order the operations were planned.
    ///
    /// `None` marks a link the query deliberately deletes, so a cascade that
    /// removes it is expected rather than a side effect. Mirrors the
    /// `intendedFinalStates` dictionary the C# processor builds before
    /// applying its planned operations.
    pub(super) fn intended_final_states(
        operations: &[(Option<ResolvedLink>, Option<ResolvedLink>)],
    ) -> Vec<(u32, Option<ResolvedLink>)> {
        let mut states: Vec<(u32, Option<ResolvedLink>)> = Vec::new();
        let mut set = |index: u32, state: Option<ResolvedLink>| match states
            .iter_mut()
            .find(|(existing, _)| *existing == index)
        {
            Some(entry) => entry.1 = state,
            None => states.push((index, state)),
        };
        for (before, after) in operations {
            match (before, after) {
                (_, Some(after)) if Self::is_normal_index(after.index) => {
                    set(after.index, Some(after.clone()))
                }
                (Some(before), None) if Self::is_normal_index(before.index) => {
                    set(before.index, None)
                }
                _ => {}
            }
        }
        states
    }

    /// Recreates links that a resolved write removed as a side effect.
    ///
    /// Mirrors `RestoreUnexpectedLinkDeletions` in the C# processor. The
    /// uniqueness resolver merges a link into an existing duplicate by
    /// deleting it, and the usages resolver cascades through the links that
    /// reference it. When the query itself asked for such a link to exist,
    /// the deletion is a side effect of the resolution order and has to be
    /// undone — otherwise a query like
    /// `((($index: $source $target)) (($index: $target $source)))` would lose
    /// half of the links it swaps, because the first swap temporarily
    /// duplicates a link that the second swap would have made unique again.
    pub(super) fn restore_unexpected_deletions(
        &self,
        storage: &mut impl NamedTypeLinks,
        intended_final_states: &[(u32, Option<ResolvedLink>)],
        changes: &mut Vec<(Option<Link>, Option<Link>)>,
    ) -> Result<()> {
        for (index, intended) in intended_final_states {
            let Some(intended) = intended else {
                self.trace_msg(&format!(
                    "[RestoreUnexpectedLinkDeletions] Link {index} was intended-deletion => skip restore."
                ));
                continue;
            };
            if storage.exists(*index) {
                continue;
            }
            self.trace_msg(&format!(
                "[RestoreUnexpectedLinkDeletions] Recreating link {index} => was unexpected deletion."
            ));
            let (before, restored) = self.create_or_update_resolved_link(storage, intended)?;
            changes.push((before, Some(restored)));
        }
        Ok(())
    }

    /// Creates or updates the link a resolved definition asks for and reports
    /// the states a `--changes` listener would see.
    ///
    /// The returned pair is `(before, after)`, where `before` is `None` only
    /// when the link genuinely had to be allocated from nothing. Everything
    /// else — an address that had to be filled in with
    /// [`try_ensure_created`](NamedTypeLinks::try_ensure_created), a definition
    /// that already matches its stored state, a duplicate of an existing
    /// doublet — reports the state that was there before, mirroring
    /// `CreateOrUpdateLink` in the C# processor:
    ///
    /// ```csharp
    /// if (existingDoublet.Source != linkDefinition.Source || existingDoublet.Target != linkDefinition.Target)
    /// { ... links.Update(...); }
    /// else
    /// { options.ChangesHandler?.Invoke(existingDoublet, existingDoublet); }
    /// ```
    ///
    /// Skipping the update when nothing changes is not only a reporting
    /// detail: a redundant write shows up in the transitions log and — far
    /// worse — counts as a write for the persistent transformation decorator,
    /// which would replay every stored trigger for a query that changed
    /// nothing.
    pub(super) fn create_or_update_resolved_link(
        &self,
        storage: &mut impl NamedTypeLinks,
        definition: &ResolvedLink,
    ) -> Result<(Option<Link>, Link)> {
        let (before, id) = if Self::is_normal_index(definition.index) {
            storage.try_ensure_created(definition.index)?;
            let existing = storage
                .get_link(definition.index)
                .unwrap_or_else(|| Link::new(definition.index, 0, 0));
            let source = Self::resolve_unspecified(definition.source, existing.source);
            let target = Self::resolve_unspecified(definition.target, existing.target);
            if existing.source != source || existing.target != target {
                self.trace_msg(&format!(
                    "[CreateOrUpdateLink] Updating link {}: {}->{source}, {}->{target}.",
                    definition.index, existing.source, existing.target
                ));
                storage.update(definition.index, source, target)?;
            } else {
                self.trace_msg(&format!(
                    "[CreateOrUpdateLink] Link {} is already S={source}, T={target} => no change.",
                    definition.index
                ));
            }
            (Some(existing), definition.index)
        } else if let Some(existing_id) =
            Self::search_unspecified(storage, definition.source, definition.target)
        {
            self.trace_msg(&format!(
                "[CreateOrUpdateLink] Link already found => ID={existing_id}, no changes."
            ));
            let existing = storage
                .get_link(existing_id)
                .unwrap_or_else(|| Link::new(existing_id, definition.source, definition.target));
            (Some(existing), existing_id)
        } else {
            let source = Self::resolve_unspecified(definition.source, 0);
            let target = Self::resolve_unspecified(definition.target, 0);
            self.trace_msg(&format!(
                "[CreateOrUpdateLink] Creating new link => (S={source},T={target})."
            ));
            (None, storage.create(source, target))
        };

        if let Some(name) = &definition.name {
            storage.set_name(id, name)?;
        }

        let after = storage
            .get_link(id)
            .unwrap_or_else(|| Link::new(id, definition.source, definition.target));
        Ok((before, after))
    }

    /// Ensures a link is created from a LiNo pattern, recursing into its parts.
    ///
    /// Port of `EnsureNestedLinkCreatedRecursively` in the C# processor. Every
    /// doublet it touches — including the nested ones — appends its
    /// `(before, after)` states to `changes`, exactly as the C# version reports
    /// them through `options.ChangesHandler`, so `--changes` lists the same
    /// records in both languages. Leaves report nothing: C#'s `ResolveLeaf`
    /// passes the update handler that ignores the changes handler.
    pub(super) fn ensure_link_created(
        &self,
        storage: &mut impl NamedTypeLinks,
        lino_link: &LinoLink,
        changes: &mut Vec<(Option<Link>, Option<Link>)>,
    ) -> Result<u32> {
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
                return storage.get_or_create_named(id);
            }
            return Ok(0);
        }

        // Handle composite links with 2 values
        if lino_link.values_count() == 2 {
            let values = lino_link.values.as_ref().unwrap();

            // Recursively ensure source and target exist
            let source_id = self.ensure_link_created(storage, &values[0], changes)?;
            let target_id = self.ensure_link_created(storage, &values[1], changes)?;

            // Create or get the composite link
            let link_id = if let Some(ref id) = lino_link.id {
                if let Ok(num) = id.parse::<u32>() {
                    // Specific ID requested.
                    self.ensure_indexed_link(storage, num, source_id, target_id, changes)?
                } else if id == "*" || Self::is_variable(id) {
                    self.ensure_doublet(storage, source_id, target_id, changes)
                } else {
                    // Named link: this repository resolves the address through
                    // the name, where C# resolves it through `(source, target)`
                    // and names the result afterwards. The reported states are
                    // the same either way.
                    let existing = storage.get_by_name(id)?;
                    if let Some(id_num) = existing {
                        self.ensure_indexed_link(storage, id_num, source_id, target_id, changes)?
                    } else {
                        let new_id = storage.create(
                            Self::resolve_unspecified(source_id, 0),
                            Self::resolve_unspecified(target_id, 0),
                        );
                        changes.push((None, storage.get_link(new_id)));
                        storage.set_name(new_id, id)?;
                        new_id
                    }
                }
            } else {
                // Anonymous link
                self.ensure_doublet(storage, source_id, target_id, changes)
            };

            return Ok(link_id);
        }

        Err(LinkError::InvalidFormat("Invalid link structure".to_string()).into())
    }

    /// `EnsureLinkCreated` for a definition that names its own address.
    ///
    /// Fills the address in when the store does not have it yet, then writes
    /// only if the stored doublet really differs — the `else` branch in C#
    /// reports `(existing, existing)` without touching the store:
    ///
    /// ```csharp
    /// TraceIfEnabled(options, $"[EnsureLinkCreated] Link #{link.Index} is already correct => no-op.");
    /// options.ChangesHandler?.Invoke(storedD, storedD);
    /// ```
    ///
    /// The redundant write this avoids is not merely noise: it lands in the
    /// transitions log and, with `--always`/`--once` triggers in play, replays
    /// every stored transformation for a query that changed nothing.
    fn ensure_indexed_link(
        &self,
        storage: &mut impl NamedTypeLinks,
        index: u32,
        source: u32,
        target: u32,
        changes: &mut Vec<(Option<Link>, Option<Link>)>,
    ) -> Result<u32> {
        storage.try_ensure_created(index)?;
        let stored = storage
            .get_link(index)
            .unwrap_or_else(|| Link::new(index, 0, 0));
        let source = Self::resolve_unspecified(source, stored.source);
        let target = Self::resolve_unspecified(target, stored.target);
        if stored.source != source || stored.target != target {
            self.trace_msg(&format!(
                "[EnsureLinkCreated] Updating link {index} => {}->{source}, {}->{target}.",
                stored.source, stored.target
            ));
            storage.update(index, source, target)?;
            let after = storage
                .get_link(index)
                .unwrap_or_else(|| Link::new(index, source, target));
            changes.push((Some(stored), Some(after)));
        } else {
            self.trace_msg(&format!(
                "[EnsureLinkCreated] Link {index} is already correct => no-op."
            ));
            changes.push((Some(stored), Some(stored)));
        }
        Ok(index)
    }

    /// `EnsureLinkCreated` for a definition with no address of its own: the
    /// existing doublet is reused and reported as an unchanged pair, and only a
    /// genuinely new one reports a creation.
    fn ensure_doublet(
        &self,
        storage: &mut impl NamedTypeLinks,
        source: u32,
        target: u32,
        changes: &mut Vec<(Option<Link>, Option<Link>)>,
    ) -> u32 {
        if let Some(existing_id) = Self::search_unspecified(storage, source, target) {
            self.trace_msg(&format!(
                "[EnsureLinkCreated] Link already found => ID={existing_id} => no-op."
            ));
            let existing = storage
                .get_link(existing_id)
                .unwrap_or_else(|| Link::new(existing_id, source, target));
            changes.push((Some(existing), Some(existing)));
            existing_id
        } else {
            let source = Self::resolve_unspecified(source, 0);
            let target = Self::resolve_unspecified(target, 0);
            self.trace_msg(&format!(
                "[EnsureLinkCreated] Creating link for (S={source}, T={target})."
            ));
            let created = storage.create(source, target);
            changes.push((None, storage.get_link(created)));
            created
        }
    }
}
