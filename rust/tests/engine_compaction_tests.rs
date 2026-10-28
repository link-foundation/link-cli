//! Before-feature control: publish the initial checkpoint through the engine.
use link_cli::engine::{Engine, EngineOptions, MemoryLog};
use link_cli::{LinkError, ReplaceableTransitionLogStore, TransitionLogStore};
#[derive(Default)]
struct ReplacingLog(MemoryLog);
impl TransitionLogStore for ReplacingLog {
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.0.flush_log()
    }
    fn append_log_entry(&mut self, row: &str) -> Result<(), LinkError> {
        self.0.append_log_entry(row)
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.0.read_log_entries()
    }
}
impl ReplaceableTransitionLogStore for ReplacingLog {
    fn replace_log_entries(&mut self, rows: &[String]) -> Result<bool, LinkError> {
        let changed = self.0.entries != rows;
        self.0.entries = rows.to_vec();
        Ok(changed)
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn compact_publishes_an_initial_checkpoint_and_reports_identical_repetition() {
    let engine = Engine::<u32, _>::open(ReplacingLog::default(), EngineOptions::default()).unwrap();
    let expected = engine.export_checkpoint().unwrap();
    let changed = engine.with_log(|log| log.read_log_entries()).unwrap() != expected;
    assert_eq!(engine.compact_journal().unwrap(), changed);
    assert_eq!(
        engine.with_log(|log| log.read_log_entries()).unwrap(),
        expected
    );
    assert!(!engine.compact_journal().unwrap());
}

mod contracts {
    // Engine contracts share independent retained-checkpoint/delta vectors and
    // finite durable images with separately recorded before-feature controls.
    use link_cli::engine::{
        DeltaLog, Engine, EngineOptions, GroupCommitOutcome, HistoryPolicy, IsolationLevel,
        MemoryLog, RevisionEncoding, StateStructure,
    };
    use link_cli::{LinkError, LinksStorage, ReplaceableTransitionLogStore, TransitionLogStore};
    use std::{cell::RefCell, rc::Rc};

    #[derive(Default)]
    struct Image {
        live: Vec<String>,
        durable: Vec<String>,
        fault: Option<bool>,
        replacements: usize,
    }
    #[derive(Clone)]
    struct ReplacingLog {
        image: Rc<RefCell<Image>>,
        supported: bool,
    }
    impl Default for ReplacingLog {
        fn default() -> Self {
            Self {
                image: Rc::new(RefCell::new(Image::default())),
                supported: true,
            }
        }
    }
    impl ReplacingLog {
        fn recovered(&self) -> Self {
            let durable = self.image.borrow().durable.clone();
            Self {
                image: Rc::new(RefCell::new(Image {
                    live: durable.clone(),
                    durable,
                    ..Default::default()
                })),
                supported: self.supported,
            }
        }
    }
    impl TransitionLogStore for ReplacingLog {
        fn supports_durable_flush(&self) -> bool {
            true
        }
        fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
            Ok(self.image.borrow().live.clone())
        }
        fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
            self.image.borrow_mut().live.push(entry.into());
            Ok(())
        }
        fn flush_log(&mut self) -> Result<(), LinkError> {
            let mut s = self.image.borrow_mut();
            s.durable = s.live.clone();
            Ok(())
        }
    }
    impl ReplaceableTransitionLogStore for ReplacingLog {
        fn supports_replacement(&self) -> bool {
            self.supported
        }
        fn replace_log_entries(&mut self, entries: &[String]) -> Result<bool, LinkError> {
            let mut s = self.image.borrow_mut();
            s.replacements += 1;
            if s.fault == Some(false) {
                return Err(LinkError::StorageError("before publication".into()));
            }
            let changed = s.live != entries;
            s.live = entries.to_vec();
            s.durable = entries.to_vec();
            if s.fault == Some(true) {
                return Err(LinkError::StorageError("after publication".into()));
            }
            Ok(changed)
        }
    }
    fn exercise<L: ReplaceableTransitionLogStore>(log: L, options: EngineOptions) {
        let engine = Engine::<u32, _>::open(log, options.clone()).unwrap();
        assert!(engine.compact_journal().unwrap());
        assert!(!engine.compact_journal().unwrap());
        let mut pin = None;
        for revision in 1..=6 {
            let mut tx = engine.begin().unwrap();
            if revision == 1 {
                tx.create_link(0, 0).unwrap();
            }
            if revision == 3 {
                tx.ensure_link_created(7).unwrap();
                tx.delete_link(7).unwrap();
            }
            tx.set_name(1, Some(format!("雪 {} {revision}", "x".repeat(256))))
                .unwrap();
            tx.commit().unwrap();
            if revision == 1 {
                pin = Some(engine.snapshot().unwrap());
            }
        }
        let old_history: Vec<_> = (0..=6)
            .map(|revision| engine.snapshot_at(revision).ok())
            .collect();
        let mut pending = engine.begin().unwrap();
        pending.ensure_link_created(99).unwrap();
        let expected = engine.export_checkpoint().unwrap();
        let changed = engine.with_log(|log| log.read_log_entries()).unwrap() != expected;
        assert_eq!(engine.compact_journal().unwrap(), changed);
        assert_eq!(
            engine.with_log(|log| log.read_log_entries()).unwrap(),
            expected
        );
        assert!(!engine.compact_journal().unwrap());
        for (revision, before) in old_history.iter().enumerate() {
            match (before, engine.snapshot_at(revision as u64)) {
                (Some(before), Ok(after)) => assert_eq!(before.rows(), after.rows()),
                (None, Err(_)) => {}
                _ => panic!("compaction changed retained revision {revision}"),
            }
        }
        let copy = Engine::<u32, _>::open(
            MemoryLog { entries: expected },
            EngineOptions {
                require_durable_flush: false,
                ..options.clone()
            },
        )
        .unwrap();
        let mut copy_tx = copy.begin().unwrap();
        assert_eq!(copy_tx.create_link(0, 0).unwrap(), 8);
        copy_tx.rollback();
        assert_eq!(engine.snapshot().unwrap().revision, 6);
        assert!(engine.snapshot().unwrap().get(99).is_none());
        pending.commit().unwrap();
        drop(pending);
        let mut first = engine.begin().unwrap();
        let mut second = engine.begin().unwrap();
        assert_eq!(first.create_link(1, 0).unwrap(), 100);
        assert_eq!(second.create_link(0, 1).unwrap(), 101);
        assert!(engine
            .commit_group(vec![first, second])
            .unwrap()
            .iter()
            .all(|result| matches!(result, GroupCommitOutcome::Committed(_))));
        assert_eq!(engine.snapshot().unwrap().revision, 9);
        assert!(pin.as_ref().unwrap().get(99).is_none());
        assert!(pin
            .as_ref()
            .unwrap()
            .get(1)
            .unwrap()
            .name
            .as_ref()
            .unwrap()
            .ends_with(" 1"));
        let expected = engine.export_checkpoint().unwrap();
        let changed = engine.with_log(|log| log.read_log_entries()).unwrap() != expected;
        assert_eq!(engine.compact_journal().unwrap(), changed);
        assert!(!engine.compact_journal().unwrap());
        let records = engine.with_log(|log| log.read_log_entries()).unwrap();
        let restored = Engine::<u32, _>::open(
            MemoryLog { entries: records },
            EngineOptions {
                require_durable_flush: false,
                ..options
            },
        )
        .unwrap();
        assert_eq!(
            restored.snapshot().unwrap().rows(),
            engine.snapshot().unwrap().rows()
        );
        assert_eq!(restored.snapshot().unwrap().revision, 9);
        assert_eq!(restored.begin().unwrap().create_link(0, 0).unwrap(), 102);
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn replacement_retains_history_private_reservations_pins_and_following_groups() {
        for structure in [
            StateStructure::Flat,
            StateStructure::Shared,
            StateStructure::SortedRuns,
        ] {
            for encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
                for history in [
                    HistoryPolicy::None,
                    HistoryPolicy::Last(2),
                    HistoryPolicy::All,
                ] {
                    let options = EngineOptions {
                        isolation: IsolationLevel::Snapshot,
                        state_structure: structure,
                        revision_encoding: encoding,
                        history,
                        require_durable_flush: true,
                        ..Default::default()
                    };
                    exercise(ReplacingLog::default(), options.clone());
                    exercise(
                        DeltaLog::new(ReplacingLog::default(), 128).unwrap(),
                        options,
                    );
                }
            }
        }
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn unsupported_replacement_leaves_engine_available_for_following_commits() {
        let log = ReplacingLog {
            supported: false,
            ..Default::default()
        };
        let observer = log.clone();
        let engine = Engine::<u32, _>::open(log, EngineOptions::default()).unwrap();
        assert!(engine.compact_journal().is_err());
        assert_eq!(observer.image.borrow().replacements, 0);
        let mut tx = engine.begin().unwrap();
        tx.create_link(0, 0).unwrap();
        tx.commit().unwrap();
        assert_eq!(engine.snapshot().unwrap().revision, 1);
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn errors_before_and_after_publication_fence_engine_and_live_sessions_until_reopen() {
        for after in [false, true] {
            let log = ReplacingLog::default();
            let observer = log.clone();
            let engine = Engine::<u32, _>::open(log, EngineOptions::default()).unwrap();
            {
                let mut tx = engine.begin().unwrap();
                tx.create_link(0, 0).unwrap();
                tx.commit().unwrap();
            }
            let pin = engine.snapshot().unwrap();
            let mut pending = engine.begin().unwrap();
            pending.create_link(0, 0).unwrap();
            observer.image.borrow_mut().fault = Some(after);
            assert!(engine.compact_journal().is_err());
            assert!(engine.snapshot().is_err());
            assert!(engine.begin().is_err());
            assert!(engine.export_checkpoint().is_err());
            assert!(engine.compact_journal().is_err());
            assert!(pending.commit().is_err());
            assert_eq!(pin.revision, 1);
            assert_eq!(pin.rows().len(), 1);
            drop(pending);
            drop(engine);
            let restored =
                Engine::<u32, _>::open(observer.recovered(), EngineOptions::default()).unwrap();
            assert_eq!(restored.snapshot().unwrap().rows(), pin.rows());
            assert_eq!(
                observer.image.borrow().durable[0].starts_with("engine3|"),
                after
            );
            let mut tx = restored.begin().unwrap();
            assert_eq!(tx.create_link(0, 0).unwrap(), 2);
            tx.commit().unwrap();
            drop(tx);
            assert!(restored.compact_journal().unwrap());
            assert!(!restored.compact_journal().unwrap());
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn delta_replacement_prepares_exact_vectors_validates_before_io_and_resumes() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../../docs/protocol/engine-checkpoint-vectors.json"
        ))
        .unwrap();
        let logical: Vec<String> =
            serde_json::from_value(vectors["physicalDelta"]["logical"].clone()).unwrap();
        let physical: Vec<String> =
            serde_json::from_value(vectors["physicalDelta"]["physical"].clone()).unwrap();
        let inner = ReplacingLog::default();
        let observer = inner.clone();
        let mut log = DeltaLog::new(inner, 128).unwrap();
        assert!(log.supports_replacement());
        assert!(log.replace_log_entries(&logical[..2]).unwrap());
        assert_eq!(observer.image.borrow().live, physical[..2]);
        assert!(!log.replace_log_entries(&logical[..2]).unwrap());
        for entry in &logical[2..] {
            log.append_log_entry(entry).unwrap();
        }
        log.flush_log().unwrap();
        assert_eq!(observer.image.borrow().live, physical);
        assert_eq!(log.read_log_entries().unwrap(), logical);
        let calls = observer.image.borrow().replacements;
        for invalid in [
            vec!["bad".to_string()],
            vec![logical[0].clone(), "bad".into()],
            vec![logical[0].clone(), logical[0].clone()],
        ] {
            assert!(log.replace_log_entries(&invalid).is_err());
            assert_eq!(observer.image.borrow().replacements, calls);
            assert_eq!(log.read_log_entries().unwrap(), logical);
        }
        assert!(log.replace_log_entries(&[]).unwrap());
        assert!(!log.replace_log_entries(&[]).unwrap());
        assert!(log.replace_log_entries(&logical).unwrap());
        assert_eq!(observer.image.borrow().live, physical);
        let inner = ReplacingLog {
            supported: false,
            ..Default::default()
        };
        let observer = inner.clone();
        let mut unsupported = DeltaLog::new(inner, 128).unwrap();
        assert!(!unsupported.supports_replacement());
        assert!(unsupported.replace_log_entries(&[]).is_err());
        assert_eq!(observer.image.borrow().replacements, 0);
        for entry in &logical {
            unsupported.append_log_entry(entry).unwrap();
        }
        assert_eq!(unsupported.read_log_entries().unwrap(), logical);
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn delta_replacement_errors_fence_codec_until_reopen_complete_old_or_new_stream() {
        let vectors: serde_json::Value = serde_json::from_str(include_str!(
            "../../docs/protocol/engine-checkpoint-vectors.json"
        ))
        .unwrap();
        let logical: Vec<String> =
            serde_json::from_value(vectors["physicalDelta"]["logical"].clone()).unwrap();
        for after in [false, true] {
            let inner = ReplacingLog::default();
            let observer = inner.clone();
            let mut log = DeltaLog::new(inner, 128).unwrap();
            log.replace_log_entries(&logical[..1]).unwrap();
            observer.image.borrow_mut().fault = Some(after);
            assert!(log.replace_log_entries(&logical).is_err());
            let calls = observer.image.borrow().replacements;
            assert!(log.read_log_entries().is_err());
            assert!(log.append_log_entry(&logical[1]).is_err());
            assert!(log.flush_log().is_err());
            assert!(log.replace_log_entries(&[]).is_err());
            assert_eq!(observer.image.borrow().replacements, calls);
            let mut reopened = DeltaLog::new(observer.recovered(), 128).unwrap();
            assert_eq!(
                reopened.read_log_entries().unwrap(),
                if after {
                    logical.clone()
                } else {
                    logical[..1].to_vec()
                }
            );
            assert_eq!(reopened.replace_log_entries(&logical).unwrap(), !after);
        }
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn compaction_preserves_pessimistic_ownership_exclusive_admission_and_stale_dependencies() {
        use link_cli::engine::{ConcurrencyControl, SerializableValidation};
        for concurrency in [
            ConcurrencyControl::Optimistic,
            ConcurrencyControl::Pessimistic,
            ConcurrencyControl::Exclusive,
        ] {
            for isolation in [
                IsolationLevel::ReadCommitted,
                IsolationLevel::Snapshot,
                IsolationLevel::Serializable,
            ] {
                let options = EngineOptions {
                    concurrency,
                    isolation,
                    ..Default::default()
                };
                let engine = Engine::<u32, _>::open(ReplacingLog::default(), options).unwrap();
                {
                    let mut seed = engine.begin().unwrap();
                    seed.create_link(0, 0).unwrap();
                    seed.commit().unwrap();
                }
                let mut first = engine.begin().unwrap();
                first.set_name(1, Some("held".into())).unwrap();
                engine.compact_journal().unwrap();
                if concurrency == ConcurrencyControl::Exclusive {
                    assert!(engine.begin().is_err());
                } else {
                    let mut second = engine.begin().unwrap();
                    let edit = second.set_name(1, Some("contender".into()));
                    if concurrency == ConcurrencyControl::Pessimistic {
                        assert!(edit.is_err());
                    } else {
                        edit.unwrap();
                    }
                    first.commit().unwrap();
                    engine.compact_journal().unwrap();
                    if concurrency == ConcurrencyControl::Optimistic
                        && isolation != IsolationLevel::ReadCommitted
                    {
                        assert!(second.commit().is_err());
                    }
                    second.rollback();
                }
                if concurrency == ConcurrencyControl::Exclusive {
                    first.commit().unwrap();
                }
                drop(first);
                let mut following = engine.begin().unwrap();
                following.set_name(1, Some("following".into())).unwrap();
                following.commit().unwrap();
                assert_eq!(
                    engine.snapshot().unwrap().get(1).unwrap().name.as_deref(),
                    Some("following")
                );
            }
        }
        for concurrency in [
            ConcurrencyControl::Optimistic,
            ConcurrencyControl::Pessimistic,
        ] {
            let engine = Engine::<u32, _>::open(
                ReplacingLog::default(),
                EngineOptions {
                    concurrency,
                    serializable_validation: SerializableValidation::ReadSet,
                    ..Default::default()
                },
            )
            .unwrap();
            {
                let mut seed = engine.begin().unwrap();
                seed.create_link(0, 0).unwrap();
                seed.commit().unwrap();
            }
            let mut stale = engine.begin().unwrap();
            assert_eq!(stale.get_name(1), None);
            {
                let mut writer = engine.begin().unwrap();
                writer.set_name(1, Some("changed".into())).unwrap();
                writer.commit().unwrap();
            }
            engine.compact_journal().unwrap();
            stale.create_link(0, 0).unwrap();
            assert!(stale.commit().is_err());
        }
    }
    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn compacted_parent_and_branch_resume_exact_retained_descriptor_and_merge_allocator() {
        for history in [HistoryPolicy::All, HistoryPolicy::Last(2)] {
            for delta in [false, true] {
                let parent =
                    Engine::<u32, _>::open(ReplacingLog::default(), EngineOptions::default())
                        .unwrap();
                {
                    let mut seed = parent.begin().unwrap();
                    seed.create_link(0, 0).unwrap();
                    seed.commit().unwrap();
                }
                let options = EngineOptions {
                    history,
                    ..Default::default()
                };
                fn branch<L: ReplaceableTransitionLogStore>(
                    parent: &Engine<u32, ReplacingLog>,
                    log: L,
                    options: EngineOptions,
                ) {
                    let branch = parent.fork(log, options.clone()).unwrap();
                    let descriptor = branch.merge_base_record();
                    {
                        let mut edit = branch.engine().begin().unwrap();
                        edit.ensure_link_created(7).unwrap();
                        edit.delete_link(7).unwrap();
                        edit.set_name(1, Some("compacted branch 雪".into()))
                            .unwrap();
                        edit.commit().unwrap();
                    }
                    branch.engine().compact_journal().unwrap();
                    parent.compact_journal().unwrap();
                    assert_eq!(descriptor, branch.merge_base_record());
                    let log = branch.into_engine().into_log().ok().unwrap();
                    let mut resumed = parent.resume_branch(log, options, &descriptor).unwrap();
                    assert_eq!(resumed.merge().unwrap(), 2);
                    assert_eq!(
                        parent.snapshot().unwrap().get(1).unwrap().name.as_deref(),
                        Some("compacted branch 雪")
                    );
                    {
                        let mut following = parent.begin().unwrap();
                        assert_eq!(following.create_link(0, 0).unwrap(), 8);
                        following.commit().unwrap();
                    }
                    parent.compact_journal().unwrap();
                    assert!(!parent.compact_journal().unwrap());
                }
                if delta {
                    branch(
                        &parent,
                        DeltaLog::new(ReplacingLog::default(), 128).unwrap(),
                        options,
                    );
                } else {
                    branch(&parent, ReplacingLog::default(), options);
                }
            }
        }
    }
}
