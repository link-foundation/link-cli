//! Observed row/name records and retryable pending undo.
use super::*;

impl<T, S, L> GenericTransactionsDecorator<T, S, L>
where
    T: LinkReference,
    S: LinksStorage<T>,
    L: TransitionLogStore,
{
    /// Writes one transition per link a single logical write touched.
    ///
    /// A resolved write is not necessarily a single-link change: the
    /// upstream uniqueness and usages decorators merge duplicates and
    /// cascade through usages, so one `update`/`delete` call can rewrite
    /// or remove several links. Each of them needs its own transition,
    /// otherwise a rollback (or a version-control branch switch, which
    /// replays the same transitions) cannot restore the links the
    /// cascade touched.
    ///
    /// The kind is derived from the observed pair rather than taken from
    /// the outer operation, because a cascade can delete a link during an
    /// `update` — recording that as an `Update` would make the revert a
    /// no-op, since the link no longer exists to be updated back.
    pub(super) fn record_observed_transitions(
        &mut self,
        observed: &[ObservedChange<T>],
        owns: bool,
        names_before: Option<&std::collections::BTreeMap<T, String>>,
    ) -> Result<(), LinkError> {
        let zero = T::from_byte(0);
        let changes = observed.iter().map(|(_, before, after)| {
            let kind = match (before.index != zero, after.index != zero) {
                (false, true) => TransitionKind::Create,
                (true, false) => TransitionKind::Delete,
                _ => TransitionKind::Update,
            };
            (kind, *before, *after)
        });
        self.record_transitions(changes, owns, names_before)
    }

    pub(super) fn record_transition(
        &mut self,
        kind: TransitionKind,
        before: GenericDoubletLink<T>,
        after: GenericDoubletLink<T>,
        owns: bool,
        names_before: Option<&std::collections::BTreeMap<T, String>>,
    ) -> Result<(), LinkError> {
        self.record_transitions(std::iter::once((kind, before, after)), owns, names_before)
    }

    pub(super) fn record_transitions(
        &mut self,
        changes: impl ExactSizeIterator<Item = TransitionChange<T>>,
        owns: bool,
        names_before: Option<&std::collections::BTreeMap<T, String>>,
    ) -> Result<(), LinkError> {
        // Names may be changed by an inner decorator during a row operation.
        // Capture its actual result rather than predicting that every update
        // preserves the old name. Keep all observed undo even if this read fails.
        let captured = if names_before.is_some() {
            self.inner
                .capture_names()
                .and_then(|names| {
                    names.ok_or_else(|| {
                        LinkError::StorageError(
                            "Named storage lost its name-capture capability.".into(),
                        )
                    })
                })
                .map(Some)
        } else {
            Ok(None)
        };
        let (names_after, capture_error) = match captured {
            Ok(names) => (names, None),
            Err(error) => (None, Some(error)),
        };
        let records = changes.map(|(kind, before, after)| {
            let index = if after.index != T::from_byte(0) {
                after.index
            } else {
                before.index
            };
            let old_name = names_before.and_then(|names| names.get(&index)).cloned();
            let new_name = match &names_after {
                Some(names) => names.get(&index).cloned(),
                // A failed capture permits undo only; fence the owner below.
                None if kind != TransitionKind::Delete => old_name.clone(),
                None => None,
            };
            let names = if old_name.is_some() || new_name.is_some() {
                vec![NameChange {
                    index,
                    before: old_name,
                    after: new_name,
                }]
            } else {
                Vec::new()
            };
            JournalRecord {
                transition: GenericTransition {
                    transaction_id: 0,
                    sequence: 0,
                    timestamp_ms: 0,
                    kind,
                    before,
                    after,
                },
                name_only: false,
                names,
            }
        });
        self.record_records(records, owns)?;
        if let Some(error) = capture_error {
            self.current
                .as_mut()
                .expect("open transaction")
                .rollback_only = true;
            if owns {
                self.rollback()?;
            }
            return Err(error);
        }
        Ok(())
    }

    pub(super) fn record_records(
        &mut self,
        records: impl ExactSizeIterator<Item = JournalRecord<T>>,
        owns: bool,
    ) -> Result<(), LinkError> {
        let transaction_id = self.current.as_ref().map(|tx| tx.id).ok_or_else(|| {
            LinkError::Transaction(
                "internal: missing open transaction while recording transition".to_string(),
            )
        })?;
        let exhausted = records.len() as u128 > (i64::MAX - self.sequence_counter) as u128;
        let mut prepared = Vec::with_capacity(records.len());
        for mut transition in records {
            let sequence = if exhausted {
                0
            } else {
                self.sequence_counter += 1;
                self.sequence_counter
            };
            transition.transition.transaction_id = transaction_id;
            transition.transition.sequence = sequence;
            transition.transition.timestamp_ms = now_unix_ms();
            self.current
                .as_mut()
                .expect("open transaction")
                .transitions
                .push(transition.clone());
            if !exhausted {
                self.log.push(transition.clone());
            }
            prepared.push(transition);
        }
        if exhausted {
            // Keep the entire unnumbered undo batch only in pending memory.
            // It must never become invalid serialized journal/history entries.
            self.current
                .as_mut()
                .expect("open transaction")
                .rollback_only = true;
            if owns {
                self.rollback()?;
            }
            return Err(LinkError::Transaction(
                "Transaction sequence space is exhausted.".into(),
            ));
        }
        self.append_transitions(&prepared, owns)
    }

    pub(super) fn append_transitions(
        &mut self,
        transitions: &[JournalRecord<T>],
        owns: bool,
    ) -> Result<(), LinkError> {
        // Save every undo record before the first fallible journal write.
        for transition in transitions {
            if let Err(error) = self.write_transition_to_log(transition) {
                self.current
                    .as_mut()
                    .expect("open transaction")
                    .rollback_only = true;
                if owns {
                    self.rollback()?;
                }
                return Err(error);
            }
            if self.trace {
                crate::diagnostics::stderr!(
                    "[Transactions] Recorded {:?} seq={} tx={:032x}: ({},{},{}) -> ({},{},{}).",
                    transition.kind,
                    transition.sequence,
                    transition.transaction_id,
                    transition.before.index,
                    transition.before.source,
                    transition.before.target,
                    transition.after.index,
                    transition.after.source,
                    transition.after.target,
                );
            }
        }
        Ok(())
    }

    pub(super) fn write_transition_to_log(
        &mut self,
        transition: &JournalRecord<T>,
    ) -> Result<(), LinkError> {
        self.log_store.append_log_entry(&format!(
            "{TRANSITION_NAME_PREFIX}{}",
            transition.serialize()
        ))
    }
}
