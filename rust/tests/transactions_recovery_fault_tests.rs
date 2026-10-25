//! Restart recovery from separate durable images after every replay phase.
use link_cli::{
    transactions::DoubletLink, CommitMode, GenericTransactionsDecorator, Link, LinkError,
    LinkStorage, LinksStorage, LogRetentionPolicy, Transition, TransitionKind, TransitionLogStore,
};
use std::{cell::RefCell, rc::Rc};

const POINTS: [&str; 14] = [
    "read-before",
    "read-after",
    "old-before",
    "old-after",
    "new-before",
    "new-after",
    "undo-before",
    "undo-after",
    "data-before",
    "data-after",
    "marker-before",
    "marker-after",
    "log-before",
    "log-after",
];
#[derive(Default)]
struct Control {
    point: Option<&'static str>,
    rows: Vec<Link>,
    entries: Vec<String>,
}
type Shared = Rc<RefCell<Control>>;
fn check(shared: &Shared, point: &str) -> Result<(), LinkError> {
    let mut state = shared.borrow_mut();
    if state.point == Some(point) {
        state.point = None;
        Err(LinkError::StorageError(format!("injected {point}")))
    } else {
        Ok(())
    }
}
struct Store {
    rows: LinkStorage,
    shared: Shared,
    operation: &'static str,
}
impl LinksStorage<u32> for Store {
    fn create_link(&mut self, source: u32, target: u32) -> Result<u32, LinkError> {
        Ok(self.rows.create(source, target))
    }
    fn ensure_link_created(&mut self, id: u32) -> Result<u32, LinkError> {
        Ok(self.rows.ensure_created(id))
    }
    fn get_link(&self, id: u32) -> Option<Link> {
        self.rows.get(id).copied()
    }
    fn all_links(&self) -> Vec<Link> {
        self.rows.all().into_iter().copied().collect()
    }
    fn update_link(&mut self, id: u32, source: u32, target: u32) -> Result<Link, LinkError> {
        let phase = if id == 2 {
            "undo"
        } else if source == 21 {
            "old"
        } else {
            "new"
        };
        check(&self.shared, &format!("{phase}-before"))?;
        let result = LinksStorage::update_link(&mut self.rows, id, source, target)?;
        check(&self.shared, &format!("{phase}-after"))?;
        Ok(result)
    }
    fn delete_link(&mut self, id: u32) -> Result<Link, LinkError> {
        let phase = if self.operation == "recreate" {
            "old"
        } else {
            "new"
        };
        check(&self.shared, &format!("{phase}-before"))?;
        let result = LinksStorage::delete_link(&mut self.rows, id)?;
        check(&self.shared, &format!("{phase}-after"))?;
        Ok(result)
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        check(&self.shared, "data-before")?;
        self.shared.borrow_mut().rows = self.all_links();
        check(&self.shared, "data-after")
    }
    fn reload(&mut self) -> Result<(), LinkError> {
        check(&self.shared, "reload-before")?;
        let rows = self.shared.borrow().rows.clone();
        self.rows = LinkStorage::in_memory(false);
        for row in rows {
            self.rows.ensure_created(row.index);
            self.rows.update(row.index, row.source, row.target).unwrap();
        }
        if self.shared.borrow().point == Some("reload-after") {
            self.rows.update(1, 91, 92).unwrap();
        }
        check(&self.shared, "reload-after")
    }
}
struct Log {
    entries: Vec<String>,
    shared: Shared,
}
impl TransitionLogStore for Log {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        let anchor = entry == "__transactions:applied:2";
        if anchor {
            check(&self.shared, "marker-before")?;
        }
        self.entries.push(entry.into());
        if anchor {
            check(&self.shared, "marker-after")?;
        }
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        check(&self.shared, "read-before")?;
        let entries = self.entries.clone();
        check(&self.shared, "read-after")?;
        Ok(entries)
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        check(&self.shared, "log-before")?;
        self.shared.borrow_mut().entries = self.entries.clone();
        check(&self.shared, "log-after")
    }
}
type Tx = GenericTransactionsDecorator<u32, Store, Log>;
fn open(shared: Shared, operation: &'static str) -> Result<Tx, LinkError> {
    let mut rows = LinkStorage::in_memory(false);
    for row in &shared.borrow().rows {
        rows.ensure_created(row.index);
    }
    for row in &shared.borrow().rows {
        rows.update(row.index, row.source, row.target).unwrap();
    }
    // Loading a sparse durable image must not invent placeholder rows.
    let ids: Vec<_> = rows.all().iter().map(|row| row.index).collect();
    for id in ids {
        if !shared.borrow().rows.iter().any(|row| row.index == id) {
            rows.delete(id).unwrap();
        }
    }
    let entries = shared.borrow().entries.clone();
    GenericTransactionsDecorator::new(
        Store {
            rows,
            shared: shared.clone(),
            operation,
        },
        Log { entries, shared },
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
}
fn fixture(operation: &'static str) -> Shared {
    let state = |source, target| DoubletLink::new(1, source, target);
    let empty = DoubletLink::empty();
    let (older_kind, older_after) = if operation == "recreate" {
        (TransitionKind::Delete, empty)
    } else {
        (TransitionKind::Update, state(21, 22))
    };
    let (newer_kind, newer_before, newer_after) = match operation {
        "delete" => (TransitionKind::Delete, state(21, 22), empty),
        "recreate" => (TransitionKind::Create, empty, state(31, 32)),
        _ => (TransitionKind::Update, state(21, 22), state(31, 32)),
    };
    let mut entries = vec![];
    let changes = [
        (TransitionKind::Create, empty, state(11, 12)),
        (older_kind, state(11, 12), older_after),
        (newer_kind, newer_before, newer_after),
        (
            TransitionKind::Update,
            DoubletLink::new(2, 41, 42),
            DoubletLink::new(2, 51, 52),
        ),
    ];
    for (index, (kind, before, after)) in changes.into_iter().enumerate() {
        let sequence = index as i64 + 1;
        let transition = Transition {
            transaction_id: sequence as u128,
            sequence,
            timestamp_ms: 0,
            kind,
            before,
            after,
        };
        entries.push(format!(
            "__transactions:transition:{}",
            transition.serialize()
        ));
        if sequence != 4 {
            entries.push(format!(
                "__transactions:commit:{:032x}",
                transition.transaction_id
            ));
        }
        if sequence == 1 || sequence == 3 {
            entries.push(format!("__transactions:applied:{sequence}"));
        }
    }
    let mut rows = vec![Link::new(2, 51, 52)];
    if operation != "delete" {
        rows.insert(0, Link::new(1, 31, 32));
    }
    Rc::new(RefCell::new(Control {
        point: None,
        rows,
        entries,
    }))
}
fn verify(operation: &'static str) {
    for point in POINTS {
        // A deleted durable row makes both Rust update/delete replays no-ops.
        // Their row mutation hooks are inapplicable; undo/persistence still run.
        if operation == "delete" && (point.starts_with("old-") || point.starts_with("new-")) {
            continue;
        }
        let shared = fixture(operation);
        shared.borrow_mut().point = Some(point);
        match open(shared.clone(), operation) {
            Ok(_) => panic!("{operation}/{point}: the injected phase must execute"),
            Err(error) => assert!(
                error.to_string().contains(point),
                "{operation}/{point}: {error}"
            ),
        }
        assert!(shared.borrow().point.is_none());
        for _ in 0..2 {
            let recovered = open(shared.clone(), operation).unwrap();
            if operation == "delete" {
                assert!(!recovered.exists(1), "{operation}/{point}");
            } else {
                let row = recovered.inner().get_link(1).expect("latest committed row");
                assert_eq!((row.source, row.target), (31, 32), "{operation}/{point}");
            }
            let undone = recovered.inner().get_link(2).unwrap();
            assert_eq!(
                (undone.source, undone.target),
                (41, 42),
                "{operation}/{point}"
            );
            assert_eq!(recovered.applied_sequence(), 3);
            assert_eq!(recovered.last_logged_sequence(), 4);
            assert!(shared
                .borrow()
                .entries
                .iter()
                .any(|entry| entry == "__transactions:applied:2"));
            assert!(shared
                .borrow()
                .entries
                .iter()
                .any(|entry| entry == "__transactions:rollback:00000000000000000000000000000004"));
        }
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn interrupted_update_recovery_restarts_from_durable_images() {
    verify("update");
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn interrupted_delete_recovery_restarts_from_durable_images() {
    verify("delete");
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn interrupted_recreation_recovery_restarts_from_durable_images() {
    verify("recreate");
}

fn live_verify(operation: &'static str) {
    for point in POINTS {
        if operation == "delete" && (point.starts_with("old-") || point.starts_with("new-")) {
            continue;
        }
        for cleanup in ["recover", "save", "commit", "rollback"] {
            let shared = fixture(operation);
            // Construct an idle reachable wrapper before publishing the recovery input.
            let entries = std::mem::take(&mut shared.borrow_mut().entries);
            let mut tx = open(shared.clone(), operation).unwrap();
            tx.log_store_mut().entries = entries;
            shared.borrow_mut().point = Some(point);
            assert!(tx.recover().unwrap_err().to_string().contains(point));
            assert!(shared.borrow().point.is_none());
            let before = tx.inner().all_links();
            let journal = tx.log_store().entries.clone();
            assert!(
                tx.begin_transaction().is_err(),
                "{operation}/{point}/{cleanup}: failed recovery released admission"
            );
            assert!(tx.update(2, 91, 92).is_err());
            assert!(tx.create(91, 92).is_err());
            assert!(tx.delete(2).is_err());
            assert!(tx.ensure_created(20).is_err());
            let transition = Transition {
                transaction_id: 1,
                sequence: 1,
                timestamp_ms: 0,
                kind: TransitionKind::Update,
                before: DoubletLink::new(2, 41, 42),
                after: DoubletLink::new(2, 91, 92),
            };
            assert!(tx.apply_transition(&transition).is_err());
            assert!(tx.revert_transition(&transition).is_err());
            assert_eq!(tx.inner().all_links(), before);
            assert_eq!(tx.log_store().entries, journal);
            // Even a second failed cleanup cannot abandon the recovery owner.
            shared.borrow_mut().point = Some("read-before");
            let result = match cleanup {
                "save" => tx.save(),
                "commit" => tx.commit(),
                "rollback" => tx.rollback(),
                _ => tx.recover(),
            };
            assert!(result.unwrap_err().to_string().contains("read-before"));
            assert!(tx.begin_transaction().is_err());
            match cleanup {
                "save" => tx.save(),
                "commit" => tx.commit(),
                "rollback" => tx.rollback(),
                _ => tx.recover(),
            }
            .unwrap();
            assert_eq!(tx.inner().get_link(2).unwrap().source, 41);
            if operation == "delete" {
                assert!(!tx.exists(1));
            } else {
                assert_eq!(tx.inner().get_link(1).unwrap().source, 31);
            }
            tx.update(2, 61, 62).unwrap();
            drop(tx);
            for _ in 0..2 {
                let recovered = open(shared.clone(), operation).unwrap();
                assert_eq!(recovered.inner().get_link(2).unwrap().source, 61);
                assert_eq!(recovered.inner().get_link(2).unwrap().target, 62);
                if operation == "delete" {
                    assert!(!recovered.exists(1));
                } else {
                    assert_eq!(recovered.inner().get_link(1).unwrap().source, 31);
                }
            }
        }
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn live_update_recovery_retains_admission_after_every_failure() {
    live_verify("update");
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn live_delete_recovery_retains_admission_after_every_failure() {
    live_verify("delete");
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn live_recreation_recovery_retains_admission_after_every_failure() {
    live_verify("recreate");
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn live_reload_failure_retains_admission_until_recovery() {
    for point in ["reload-before", "reload-after"] {
        let shared = fixture("update");
        let entries = std::mem::take(&mut shared.borrow_mut().entries);
        let mut tx = open(shared.clone(), "update").unwrap();
        tx.log_store_mut().entries = entries;
        shared.borrow_mut().point = Some(point);
        assert!(tx.reload().unwrap_err().to_string().contains(point));
        assert!(
            tx.begin_transaction().is_err(),
            "{point}: failed reload released admission"
        );
        tx.recover().unwrap();
        assert_eq!(tx.inner().get_link(1).unwrap().source, 31);
        assert_eq!(tx.inner().get_link(2).unwrap().source, 41);
        tx.update(2, 61, 62).unwrap();
        drop(tx);
        for _ in 0..2 {
            let recovered = open(shared.clone(), "update").unwrap();
            assert_eq!(recovered.inner().get_link(1).unwrap().source, 31);
            assert_eq!(recovered.inner().get_link(2).unwrap().source, 61);
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn reload_retry_refreshes_applied_external_state_before_releasing_admission() {
    for point in ["reload-before", "reload-after"] {
        for cleanup in ["recover", "save", "commit", "rollback", "reload"] {
            let shared = Rc::new(RefCell::new(Control {
                rows: vec![Link::new(1, 11, 12)],
                ..Control::default()
            }));
            let mut tx = open(shared.clone(), "update").unwrap();
            let transition = Transition {
                transaction_id: 7,
                sequence: 1,
                timestamp_ms: 0,
                kind: TransitionKind::Update,
                before: DoubletLink::new(1, 11, 12),
                after: DoubletLink::new(1, 31, 32),
            };
            tx.log_store_mut().entries = vec![
                format!("__transactions:transition:{}", transition.serialize()),
                "__transactions:commit:00000000000000000000000000000007".into(),
                "__transactions:applied:1".into(),
            ];
            // Another writer has already durably checkpointed this state.
            shared.borrow_mut().rows = vec![Link::new(1, 31, 32)];
            shared.borrow_mut().point = Some(point);
            assert!(tx.reload().is_err());
            let cleanup_result = |tx: &mut Tx| match cleanup {
                "save" => tx.save(),
                "commit" => tx.commit(),
                "rollback" => tx.rollback(),
                "reload" => tx.reload(),
                _ => tx.recover(),
            };
            shared.borrow_mut().point = Some("reload-before");
            assert!(
                cleanup_result(&mut tx).is_err(),
                "{point}/{cleanup}: skipped failed reload"
            );
            assert!(tx.begin_transaction().is_err());
            assert_eq!(shared.borrow().rows[0], Link::new(1, 31, 32));
            cleanup_result(&mut tx).unwrap();
            assert_eq!(tx.inner().get_link(1).unwrap(), Link::new(1, 31, 32));
            tx.update(1, 61, 62).unwrap();
            drop(tx);
            for _ in 0..2 {
                let recovered = open(shared.clone(), "update").unwrap();
                assert_eq!(recovered.inner().get_link(1).unwrap(), Link::new(1, 61, 62));
            }
        }
    }
}
