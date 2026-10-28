//! Optional pre-effect image ownership and idempotent logical restoration.
use std::collections::{BTreeMap, BTreeSet};

use super::undo_image::UndoImage;
use super::*;

impl<T, S, L> GenericTransactionsDecorator<T, S, L>
where
    T: LinkReference,
    S: LinksStorage<T>,
    L: TransitionLogStore,
{
    /// Select row undo while idle. Recovery always understands mandatory images,
    /// even if subsequent owners use the default observation policy.
    pub fn with_row_undo(mut self, policy: RowUndoPolicy) -> Result<Self, LinkError> {
        self.set_row_undo_policy(policy)?;
        Ok(self)
    }

    /// Change policy while idle, preserving the caller's owner on rejection.
    pub fn set_row_undo_policy(&mut self, policy: RowUndoPolicy) -> Result<(), LinkError> {
        if self.has_open_transaction() {
            return Err(LinkError::Transaction(
                "Select row undo before opening a transaction.".into(),
            ));
        }
        self.row_undo = policy;
        Ok(())
    }

    pub fn row_undo_policy(&self) -> RowUndoPolicy {
        self.row_undo
    }

    /// Complete the logical journal before publishing a successful write.
    /// Images protect aborts even before callbacks; these correction records
    /// also make committed redo and version replay include unreported effects.
    pub(super) fn complete_image_observations(&mut self, owns: bool) -> Result<(), LinkError> {
        let Some(owner) = self.current.as_ref().filter(|owner| owner.image.is_some()) else {
            return Ok(());
        };
        let image = owner.image.as_ref().expect("selected image");
        let mut predicted = image
            .rows
            .iter()
            .map(|row| (row.index, *row))
            .collect::<BTreeMap<_, _>>();
        let mut names = image.names.clone();
        for record in &owner.transitions {
            if !record.name_only {
                match record.kind {
                    TransitionKind::Delete => {
                        predicted.remove(&record.before.index);
                    }
                    _ => {
                        predicted.insert(record.after.index, record.after.into());
                    }
                }
            }
            if let Some(names) = names.as_mut() {
                for change in &record.names {
                    names.remove(&change.index);
                }
                for change in &record.names {
                    if let Some(name) = &change.after {
                        names.insert(change.index, name.clone());
                    }
                }
            }
        }
        let result = (|| {
            let mut actual = BTreeMap::new();
            for row in self.inner.all_links() {
                if row.index == T::from_byte(0) || actual.insert(row.index, row).is_some() {
                    return Err(LinkError::StorageError(
                        "invalid row enumeration after a write".into(),
                    ));
                }
            }
            let changes = predicted
                .keys()
                .chain(actual.keys())
                .copied()
                .collect::<BTreeSet<_>>()
                .into_iter()
                .filter_map(|index| {
                    let before = predicted.get(&index);
                    let after = actual.get(&index);
                    if before == after {
                        return None;
                    }
                    let kind = if before.is_none() {
                        TransitionKind::Create
                    } else if after.is_none() {
                        TransitionKind::Delete
                    } else {
                        TransitionKind::Update
                    };
                    Some((
                        kind,
                        before
                            .map(GenericDoubletLink::from_link)
                            .unwrap_or_else(GenericDoubletLink::empty),
                        after
                            .map(GenericDoubletLink::from_link)
                            .unwrap_or_else(GenericDoubletLink::empty),
                    ))
                })
                .collect::<Vec<_>>();
            self.record_transitions(changes.into_iter(), owns, names.as_ref())
        })();
        if result.is_err() {
            self.fence_failed_image();
            if owns && self.current.is_some() {
                self.rollback()?;
            }
        }
        result
    }

    pub(super) fn fence_failed_image(&mut self) {
        if let Some(owner) = self.current.as_mut().filter(|owner| owner.image.is_some()) {
            owner.rollback_only = true;
        }
    }

    pub(super) fn prepare_row_undo(&mut self) -> Result<(), LinkError> {
        if self.row_undo != RowUndoPolicy::WriteAheadImage
            || self
                .current
                .as_ref()
                .is_some_and(|owner| owner.image.is_some())
        {
            return Ok(());
        }
        let owner = self.current.as_ref().expect("open transaction").id;
        let mut rows = self.inner.all_links();
        rows.sort_unstable_by_key(|row| row.index);
        let image = UndoImage {
            owner,
            base: self.sequence_counter,
            rows,
            names: self.inner.capture_names()?,
        };
        let payload = image.serialize();
        // Validate before any append, including adapters returning duplicate rows.
        UndoImage::<T>::parse(&payload)?;
        self.current.as_mut().expect("open transaction").image = Some(image);
        self.log_store
            .append_log_entry(&format!("{TRANSITION_NAME_PREFIX}{payload}"))?;
        self.log_store.flush_log()?;
        if self.trace {
            crate::diagnostics::stderr!("[Transactions] Prepared row undo image tx={owner:032x}.");
        }
        Ok(())
    }

    pub(super) fn restore_image(&mut self, image: &UndoImage<T>) -> Result<(), LinkError> {
        let zero = T::from_byte(0);
        // Break references and uniqueness conflicts before deleting allocations.
        // Replay-compatible adapters must permit the same empty placeholders as
        // ordinary transition replay. Never bypass their logical storage API.
        for row in self.inner.all_links() {
            if row.source != zero || row.target != zero {
                self.inner.update_link(row.index, zero, zero)?;
            }
        }
        let wanted = image
            .rows
            .iter()
            .map(|row| row.index)
            .collect::<BTreeSet<_>>();
        for row in self.inner.all_links() {
            if !wanted.contains(&row.index) {
                self.inner.delete_link(row.index)?;
            }
        }
        for row in &image.rows {
            self.inner.ensure_link_created(row.index)?;
        }
        // Dense allocators may have filled holes while reserving old addresses.
        for row in self.inner.all_links() {
            if !wanted.contains(&row.index) {
                self.inner.delete_link(row.index)?;
            }
        }
        for row in &image.rows {
            self.inner.update_link(row.index, row.source, row.target)?;
        }
        if let Some(names) = &image.names {
            let current = self.inner.capture_names()?.ok_or_else(|| {
                LinkError::StorageError("undo image requires a named storage adapter".into())
            })?;
            for index in current.keys() {
                self.inner.set_link_name(*index, None)?;
            }
            for (index, name) in names {
                self.inner.set_link_name(*index, Some(name))?;
            }
        }
        // A decorator with unsupported replay effects must fail closed rather
        // than acknowledge a partial restoration as a successful rollback.
        let mut actual = self.inner.all_links();
        actual.sort_unstable_by_key(|row| row.index);
        if actual != image.rows
            || (image.names.is_some() && self.inner.capture_names()? != image.names)
        {
            return Err(LinkError::StorageError(
                "storage did not restore the complete undo image".into(),
            ));
        }
        Ok(())
    }
}
