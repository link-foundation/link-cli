//! Finite faults with independent row/name/journal persistence images.
use link_cli::transactions::JournalRecord;
use link_cli::{
    CommitMode, GenericTransactionsDecorator, Link, LinkError, LinkStorage, LinksStorage,
    LogRetentionPolicy, NamedTypesDecorator, TransitionLogStore,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Default)]
struct Image {
    rows: Vec<Link>,
    names: Vec<Link>,
    journal: Vec<String>,
}
#[derive(Default)]
struct Control {
    point: Option<&'static str>,
    skip: usize,
    image: Image,
    failed_image: Option<Image>,
    update_name: Option<&'static str>,
    fail_capture_after_update: bool,
}
type Shared = Rc<RefCell<Control>>;
fn check(control: &Shared, point: &str) -> Result<(), LinkError> {
    let mut control = control.borrow_mut();
    if control.point == Some(point) {
        if control.skip > 0 {
            control.skip -= 1;
            return Ok(());
        }
        control.point = None;
        control.failed_image = Some(control.image.clone());
        Err(LinkError::StorageError(format!("injected {point}")))
    } else {
        Ok(())
    }
}
fn rows(image: &[Link]) -> LinkStorage {
    let mut store = LinkStorage::in_memory(false);
    for row in image {
        store.ensure_created(row.index);
    }
    for row in image {
        store.update(row.index, row.source, row.target).unwrap();
    }
    store
}
struct Store {
    inner: NamedTypesDecorator,
    control: Shared,
}
impl LinksStorage<u32> for Store {
    fn create_link(&mut self, source: u32, target: u32) -> Result<u32, LinkError> {
        self.inner.create_link(source, target)
    }
    fn ensure_link_created(&mut self, index: u32) -> Result<u32, LinkError> {
        self.inner.ensure_link_created(index)
    }
    fn get_link(&self, index: u32) -> Option<Link> {
        self.inner.get_link(index)
    }
    fn update_link(&mut self, index: u32, source: u32, target: u32) -> Result<Link, LinkError> {
        let before = self.inner.update_link(index, source, target)?;
        let name = self.control.borrow_mut().update_name.take();
        if let Some(name) = name {
            self.inner.set_link_name(index, Some(name))?;
        }
        if std::mem::take(&mut self.control.borrow_mut().fail_capture_after_update) {
            self.control.borrow_mut().point = Some("capture");
        }
        Ok(before)
    }
    fn delete_link(&mut self, index: u32) -> Result<Link, LinkError> {
        self.inner.delete_link(index)
    }
    fn all_links(&self) -> Vec<Link> {
        self.inner.all_links()
    }
    fn capture_names(
        &mut self,
    ) -> Result<Option<std::collections::BTreeMap<u32, String>>, LinkError> {
        check(&self.control, "capture")?;
        self.inner.capture_names()
    }
    fn set_link_name(&mut self, index: u32, name: Option<&str>) -> Result<u32, LinkError> {
        check(&self.control, "name-before")?;
        let result = self.inner.set_link_name(index, name)?;
        // Model mappings that reach disk before an explicit data checkpoint.
        self.control.borrow_mut().image.names = self.inner.names_links().all_links();
        check(&self.control, "name-after")?;
        Ok(result)
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        check(&self.control, "data-before")?;
        let mut control = self.control.borrow_mut();
        control.image.rows = self.inner.all_links();
        control.image.names = self.inner.names_links().all_links();
        drop(control);
        check(&self.control, "data-after")
    }
}
struct Log {
    entries: Vec<String>,
    control: Shared,
}
impl TransitionLogStore for Log {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        check(&self.control, "append-before")?;
        self.entries.push(entry.into());
        if self.control.borrow().point == Some("append-after") && self.control.borrow().skip == 0 {
            self.control.borrow_mut().image.journal = self.entries.clone();
        }
        check(&self.control, "append-after")
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.entries.clone())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        check(&self.control, "log-before")?;
        self.control.borrow_mut().image.journal = self.entries.clone();
        check(&self.control, "log-after")
    }
}
type Tx = GenericTransactionsDecorator<u32, Store, Log>;
fn open(image: Image) -> (Tx, Shared) {
    let control = Rc::new(RefCell::new(Control {
        point: None,
        skip: 0,
        image: image.clone(),
        failed_image: None,
        update_name: None,
        fail_capture_after_update: false,
    }));
    let tx = GenericTransactionsDecorator::new(
        Store {
            inner: NamedTypesDecorator::from_link_storages(rows(&image.rows), rows(&image.names)),
            control: control.clone(),
        },
        Log {
            entries: image.journal,
            control: control.clone(),
        },
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        true,
    )
    .unwrap();
    (tx, control)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn name_failures_reopen_independent_images_and_release_cleanup_only_after_retry() {
    for point in [
        "append-before",
        "append-after",
        "log-before",
        "log-after",
        "name-before",
        "name-after",
        "data-before",
        "data-after",
        "commit-append-before",
        "commit-append-after",
        "commit-log-before",
        "commit-log-after",
    ] {
        for explicit in [false, true] {
            let (mut tx, control) = open(Image::default());
            let id = tx.create(11, 12).unwrap();
            tx.set_name(id, "original").unwrap();
            if explicit {
                tx.begin_transaction().unwrap();
            }
            {
                let mut control = control.borrow_mut();
                control.point = Some(point.strip_prefix("commit-").unwrap_or(point));
                control.skip = usize::from(point.starts_with("commit-"));
            }
            let result = tx.set_name(id, "changed 雪|\0\n").and_then(|_| tx.commit());
            assert!(result.is_err(), "{point} explicit={explicit}");
            assert_eq!(control.borrow().point, None, "fault must actually fire");
            let owner = tx.journal().last().unwrap().transaction_id;
            let persisted = control.borrow().failed_image.clone().expect("fault image");
            let committed = persisted
                .journal
                .contains(&format!("__transactions:commit:{owner:032x}"));
            let expected = if committed {
                "changed 雪|\0\n"
            } else {
                "original"
            };
            let mut image = persisted;
            for _ in 0..2 {
                let (mut reopened, reopened_control) = open(image);
                assert_eq!(
                    reopened
                        .inner_mut()
                        .capture_names()
                        .unwrap()
                        .unwrap()
                        .get(&id)
                        .map(String::as_str),
                    Some(expected),
                    "{point}"
                );
                assert_eq!(reopened.inner().get_link(id), Some(Link::new(id, 11, 12)));
                reopened.set_name(77, "following").unwrap();
                image = reopened_control.borrow().image.clone();
            }
            if explicit || committed || point.starts_with("commit-") {
                assert!(
                    tx.set_name(id, "blocked").is_err(),
                    "retained owner {point}"
                );
                assert!(tx.begin_transaction().is_err());
            }
            tx.rollback().unwrap();
            tx.rollback().unwrap();
            tx.set_name(77, "following").unwrap();
            assert_eq!(
                tx.inner_mut()
                    .capture_names()
                    .unwrap()
                    .unwrap()
                    .get(&id)
                    .map(String::as_str),
                // Cleanup resolves an attempted commit even if the captured
                // failure image did not yet contain its durable decision.
                Some(if point.starts_with("commit-") {
                    "changed 雪|\0\n"
                } else {
                    expected
                })
            );
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_or_conflicting_name_records_fail_before_rows_names_or_history_change() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/transaction-name-vectors.json"
    ))
    .unwrap();
    let first = vectors["records"][0]["wire"].as_str().unwrap();
    let second = first.replace("1,-,", "1,-,0078");
    let mut bad: Vec<Vec<String>> = vectors["invalid"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| vec![format!("__transactions:transition:{}", v.as_str().unwrap())])
        .collect();
    bad.push(vec![
        format!("__transactions:transition:{first}"),
        format!("__transactions:transition:{second}"),
    ]);
    for entries in bad {
        let (mut tx, _) = open(Image::default());
        let id = tx.create(11, 12).unwrap();
        tx.set_name(id, "original").unwrap();
        let history = tx.journal();
        let before = tx.inner_mut().capture_names().unwrap();
        tx.log_store_mut().entries = entries.clone();
        for _ in 0..2 {
            assert!(tx.recover().is_err());
            assert_eq!(tx.journal(), history);
            assert_eq!(tx.inner().get_link(id), Some(Link::new(id, 11, 12)));
            assert_eq!(tx.inner_mut().capture_names().unwrap(), before);
            assert_eq!(tx.log_store().entries, entries);
            assert!(tx.set_name(id, "blocked").is_err());
        }
    }
    // Equal duplicate complete records collapse without losing name payloads.
    let image = Image {
        journal: vec![
            format!("__transactions:transition:{first}"),
            format!("__transactions:transition:{first}"),
            "__transactions:commit:00000000000000000000000000000001".into(),
        ],
        ..Image::default()
    };
    let (mut tx, _) = open(image);
    assert_eq!(
        tx.journal(),
        vec![JournalRecord::<u32>::parse(first).unwrap()]
    );
    assert_eq!(
        tx.inner_mut()
            .capture_names()
            .unwrap()
            .unwrap()
            .get(&1)
            .map(String::as_str),
        Some("")
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unnamed_adapters_reject_complete_named_replay_before_row_effects() {
    let control = Rc::new(RefCell::new(Control::default()));
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/transaction-name-vectors.json"
    ))
    .unwrap();
    let wire = vectors["records"][5]["wire"].as_str().unwrap();
    let record = JournalRecord::<u32>::parse(wire).unwrap();
    let mut tx = GenericTransactionsDecorator::new(
        LinkStorage::in_memory(false),
        Log {
            entries: Vec::new(),
            control: control.clone(),
        },
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap();
    let id = tx.create(11, 12).unwrap();
    let before = tx.inner().get_link(id);
    assert!(tx.apply_record(&record).is_err());
    assert!(tx.revert_record(&record).is_err());
    assert_eq!(tx.inner().get_link(id), before);
    let log = Log {
        entries: vec![format!("__transactions:transition:{wire}")],
        control,
    };
    assert!(GenericTransactionsDecorator::new(
        LinkStorage::in_memory(false),
        log,
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false
    )
    .is_err());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn row_records_replay_the_actual_name_after_an_inner_decorator_changes_it() {
    let (mut tx, control) = open(Image::default());
    let id = tx.create(11, 12).unwrap();
    tx.set_name(id, "original").unwrap();
    control.borrow_mut().update_name = Some("updated 雪");
    tx.update(id, 21, 22).unwrap();
    let record = tx.journal().last().unwrap().clone();
    assert_eq!(
        tx.inner_mut().capture_names().unwrap().unwrap()[&id],
        "updated 雪"
    );
    tx.revert_record(&record).unwrap();
    assert_eq!(
        tx.inner_mut().capture_names().unwrap().unwrap()[&id],
        "original"
    );
    tx.apply_record(&record).unwrap();
    assert_eq!(
        tx.inner_mut().capture_names().unwrap().unwrap()[&id],
        "updated 雪"
    );
    assert_eq!(tx.inner().get_link(id), Some(Link::new(id, 21, 22)));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unobserved_name_owner_eviction_is_in_rollback_and_complete_history() {
    for explicit in [false, true] {
        let (mut tx, control) = open(Image::default());
        let first = tx.create(11, 12).unwrap();
        let second = tx.create(13, 14).unwrap();
        tx.set_name(first, "original").unwrap();
        tx.set_name(second, "borrowed").unwrap();
        let before = tx.inner_mut().capture_names().unwrap();
        let start = tx.journal().len();
        if explicit {
            tx.begin_transaction().unwrap();
        }
        control.borrow_mut().update_name = Some("borrowed");
        tx.update(first, 21, 22).unwrap();
        let after = tx.inner_mut().capture_names().unwrap();
        assert!(!after.as_ref().unwrap().contains_key(&second));
        if explicit {
            tx.rollback().unwrap();
        } else {
            let records = tx.journal()[start..].to_vec();
            for record in records.iter().rev() {
                tx.revert_record(record).unwrap();
            }
            assert_eq!(tx.inner_mut().capture_names().unwrap(), before);
            for record in &records {
                tx.apply_record(record).unwrap();
            }
            assert_eq!(tx.inner_mut().capture_names().unwrap(), after);
            for record in records.iter().rev() {
                tx.revert_record(record).unwrap();
            }
        }
        assert_eq!(tx.inner_mut().capture_names().unwrap(), before);
        assert_eq!(tx.inner().get_link(first), Some(Link::new(first, 11, 12)));
        tx.update(first, 31, 32).unwrap();
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn failed_post_effect_name_capture_keeps_row_undo_and_retry_ownership() {
    for explicit in [false, true] {
        let (mut tx, control) = open(Image::default());
        let id = tx.create(11, 12).unwrap();
        tx.set_name(id, "original").unwrap();
        if explicit {
            tx.begin_transaction().unwrap();
        }
        control.borrow_mut().fail_capture_after_update = true;
        assert!(tx.update(id, 21, 22).is_err());
        if explicit {
            assert!(tx.set_name(id, "blocked").is_err());
        }
        tx.rollback().unwrap();
        tx.rollback().unwrap();
        assert_eq!(tx.inner().get_link(id), Some(Link::new(id, 11, 12)));
        assert_eq!(
            tx.inner_mut().capture_names().unwrap().unwrap()[&id],
            "original"
        );
        tx.update(id, 31, 32).unwrap();
    }
}
