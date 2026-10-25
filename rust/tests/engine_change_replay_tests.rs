//! Independently authored incremental/mixed journal streams, before implementation.
use link_cli::engine::{Engine, EngineOptions, HistoryPolicy, MemoryLog, StateStructure};
use link_cli::{GenericLink, LinkError, LinksStorage, TransitionLogStore};
use std::{cell::RefCell, rc::Rc};

fn vectors() -> serde_json::Value {
    serde_json::from_str(include_str!(
        "../../docs/protocol/engine-change-vectors.json"
    ))
    .unwrap()
}
fn records(values: &serde_json::Value) -> Vec<String> {
    values
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_change_and_mixed_records_recover_exact_names_rows_history_and_allocator() {
    let fixtures = vectors();
    for structure in [StateStructure::Flat, StateStructure::Shared] {
        for kind in ["valid", "mixed"] {
            let entries = records(&fixtures[kind]);
            let options = EngineOptions {
                state_structure: structure,
                history: HistoryPolicy::All,
                ..Default::default()
            };
            let db = Engine::<u32, _>::open(
                MemoryLog {
                    entries: entries.clone(),
                },
                options.clone(),
            )
            .unwrap();
            let first = db.snapshot_at(1).unwrap();
            assert_eq!(first.get(1).unwrap().name.as_deref(), Some("雪;|😀\0\n"));
            assert_eq!(first.get(2).unwrap().name.as_deref(), Some(""));
            let second = db.snapshot_at(2).unwrap();
            assert!(second.get(2).is_none());
            assert_eq!(second.get(1).unwrap().link, GenericLink::new(1, 2, 0));
            assert!(second.get(1).unwrap().name.is_none());
            let current = db.snapshot().unwrap();
            assert_eq!(current.revision, 4);
            assert_eq!(current.links().len(), 2);
            assert_eq!(current.get(u32::MAX).unwrap().name.as_deref(), Some(""));
            assert_eq!(current.get(u32::MAX).unwrap().link.target, u32::MAX);
            assert_eq!(db.with_log(|log| Ok(log.entries.clone())).unwrap(), entries);
            let mut following = db.begin().unwrap();
            following.set_name(1, Some("following".into())).unwrap();
            assert_eq!(following.commit().unwrap(), 5);
            assert_eq!(first.get(1).unwrap().name.as_deref(), Some("雪;|😀\0\n"));
            let log = db.with_log(|log| Ok(log.clone())).unwrap();
            assert!(log
                .entries
                .last()
                .unwrap()
                .starts_with("engine1|5|4294967297|"));
            let reopened = Engine::<u32, _>::open(log, options).unwrap();
            assert_eq!(
                reopened.snapshot().unwrap().get(1).unwrap().name.as_deref(),
                Some("following")
            );
            let mut exhausted = reopened.begin().unwrap();
            assert!(exhausted.create_link(0, 0).is_err());
            exhausted.rollback();
            assert_eq!(reopened.snapshot().unwrap().revision, 5);
        }
    }
}

#[derive(Clone)]
struct ReadOnlyLog(Rc<RefCell<Vec<String>>>);
impl TransitionLogStore for ReadOnlyLog {
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.0.borrow().clone())
    }
    fn append_log_entry(&mut self, _: &str) -> Result<(), LinkError> {
        panic!("recovery wrote to invalid journal")
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        panic!("recovery flushed invalid journal")
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn malformed_change_streams_reject_without_rewriting_the_supplied_journal() {
    for fixture in vectors()["invalidU32"].as_array().unwrap() {
        for structure in [StateStructure::Flat, StateStructure::Shared] {
            let expected = records(&fixture["records"]);
            let saved = Rc::new(RefCell::new(expected.clone()));
            let result = Engine::<u32, _>::open(
                ReadOnlyLog(saved.clone()),
                EngineOptions {
                    state_structure: structure,
                    ..Default::default()
                },
            );
            assert!(
                result.is_err(),
                "accepted {} with {structure:?}",
                fixture["case"]
            );
            assert_eq!(*saved.borrow(), expected);
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn physical_delta_decorator_preserves_change_records_and_resets_snapshot_compression_bases() {
    use link_cli::engine::DeltaLog;
    let fixtures = vectors();
    let logical = records(&fixtures["physicalMixed"]["logical"]);
    let physical = records(&fixtures["physicalMixed"]["physical"]);
    let mut log = DeltaLog::new(MemoryLog::default(), 128).unwrap();
    for entry in &logical {
        log.append_log_entry(entry).unwrap();
    }
    assert_eq!(log.read_log_entries().unwrap(), logical);
    assert_eq!(log.into_inner().entries, physical);
    let db = Engine::<u32, _>::open(
        DeltaLog::new(MemoryLog { entries: physical }, 128).unwrap(),
        EngineOptions::default(),
    )
    .unwrap();
    let current = db.snapshot().unwrap();
    assert_eq!(current.revision, 5);
    assert_eq!(current.links().len(), 31);
    assert!(current.get(2).is_none());
    assert_eq!(current.get(1).unwrap().link.source, 4);
    for fixture in fixtures["invalidPhysical"].as_array().unwrap() {
        let entries = records(&fixture["records"]);
        let mut invalid = DeltaLog::new(
            MemoryLog {
                entries: entries.clone(),
            },
            128,
        )
        .unwrap();
        assert!(invalid.read_log_entries().is_err(), "{}", fixture["case"]);
        let mut appending = DeltaLog::new(MemoryLog::default(), 128).unwrap();
        for record in &entries[..entries.len() - 1] {
            appending.append_log_entry(record).unwrap();
        }
        assert!(appending.append_log_entry(entries.last().unwrap()).is_err());
        assert_eq!(appending.into_inner().entries, entries[..entries.len() - 1]);
    }
}
