//! Resolve uncertain commit decisions before admitting another write.
//! The volatile fault stores model separate persisted data and journal images;
//! the real-filesystem version owner contract is tested separately.
use link_cli::{
    CommitMode, GenericTransactionsDecorator, Link, LinkError, LinkStorage, LinksStorage,
    LogRetentionPolicy, TransitionLogStore,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    MarkerBefore,
    MarkerAfter,
    LogBefore,
    LogAfter,
    ReadBefore,
    ReadAfter,
}
const POINTS: [Point; 4] = [
    Point::MarkerBefore,
    Point::MarkerAfter,
    Point::LogBefore,
    Point::LogAfter,
];
#[derive(Default)]
struct Fault {
    point: Option<Point>,
}
impl Fault {
    fn check(&mut self, point: Point) -> Result<(), LinkError> {
        if self.point != Some(point) {
            return Ok(());
        }
        self.point = None;
        Err(LinkError::StorageError(format!("injected {point:?}")))
    }
}
type Control = Rc<RefCell<Fault>>;
struct Store {
    rows: LinkStorage,
    durable: Vec<Link>,
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
        LinksStorage::update_link(&mut self.rows, id, source, target)
    }
    fn delete_link(&mut self, id: u32) -> Result<Link, LinkError> {
        LinksStorage::delete_link(&mut self.rows, id)
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        self.durable = self.all_links();
        Ok(())
    }
}
struct Log {
    entries: Vec<String>,
    durable: Vec<String>,
    fault: Control,
}
impl TransitionLogStore for Log {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        let commit = entry.starts_with("__transactions:commit:");
        if commit {
            self.fault.borrow_mut().check(Point::MarkerBefore)?;
        }
        self.entries.push(entry.into());
        if commit && self.fault.borrow().point == Some(Point::MarkerAfter) {
            self.durable = self.entries.clone();
            self.fault.borrow_mut().check(Point::MarkerAfter)?;
        }
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        self.fault.borrow_mut().check(Point::ReadBefore)?;
        let entries = self.entries.clone();
        self.fault.borrow_mut().check(Point::ReadAfter)?;
        Ok(entries)
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        self.fault.borrow_mut().check(Point::LogBefore)?;
        self.durable = self.entries.clone();
        self.fault.borrow_mut().check(Point::LogAfter)?;
        Ok(())
    }
}
type Tx = GenericTransactionsDecorator<u32, Store, Log>;
fn open(data: &[Link], entries: Vec<String>, fault: Control) -> Tx {
    let mut rows = LinkStorage::in_memory(false);
    for row in data {
        rows.ensure_created(row.index);
    }
    for row in data {
        rows.update(row.index, row.source, row.target).unwrap();
    }
    GenericTransactionsDecorator::new(
        Store {
            rows,
            durable: data.into(),
        },
        Log {
            entries: entries.clone(),
            durable: entries,
            fault,
        },
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap()
}

fn failure(point: Point, automatic: bool) -> (Tx, u32) {
    let fault = Control::default();
    let mut tx = open(&[], vec![], fault.clone());
    let id = tx.create_and_update(11, 12).unwrap();
    if !automatic {
        tx.begin_transaction().unwrap();
        tx.update(id, 21, 22).unwrap();
    }
    *fault.borrow_mut() = Fault { point: Some(point) };
    let outcome = if automatic {
        tx.update(id, 21, 22).map(|_| ())
    } else {
        tx.commit()
    };
    assert!(outcome.is_err(), "{point:?}, automatic={automatic}");
    let committed = tx
        .log_store()
        .entries
        .iter()
        .filter(|e| e.starts_with("__transactions:commit:"))
        .count();
    assert_eq!(
        committed,
        if point == Point::MarkerBefore { 1 } else { 2 },
        "the delivery error may leave a commit marker"
    );
    assert_eq!(tx.inner().get_link(id).unwrap().source, 21);
    (tx, id)
}

fn cleanup(operation: &str) {
    for point in POINTS {
        for automatic in [false, true] {
            let (mut tx, id) = failure(point, automatic);
            tx.log_store_mut().fault.borrow_mut().point = Some(Point::LogBefore);
            let repeat = match operation {
                "commit" => tx.commit(),
                "rollback" => tx.rollback(),
                _ => tx.save(),
            };
            assert!(repeat.unwrap_err().to_string().contains("LogBefore"));
            assert_eq!(
                tx.inner().get_link(id).unwrap().source,
                21,
                "uncertain commit cannot be undone during failed cleanup"
            );
            assert!(tx.begin_transaction().is_err());
            match operation {
                "commit" => tx.commit().unwrap(),
                "rollback" => tx.rollback().unwrap(),
                _ => tx.save().unwrap(),
            }
            assert_eq!(
                tx.log_store()
                    .entries
                    .iter()
                    .filter(|entry| entry.starts_with("__transactions:commit:"))
                    .count(),
                2,
                "retry does not write another commit decision"
            );
            assert_eq!(
                tx.applied_sequence(),
                tx.last_logged_sequence(),
                "{point:?}, automatic={automatic}, {operation}"
            );
            assert_eq!(
                tx.inner().get_link(id).unwrap().source,
                21,
                "possibly durable commit must not be undone"
            );
            reopen_and_follow(tx, id);
        }
    }
}

fn reopen_and_follow(tx: Tx, id: u32) {
    let (data, log) = tx.into_inner();
    let mut recovered = open(&data.durable, log.durable, Control::default());
    let row = recovered.inner().get_link(id).unwrap();
    assert_eq!((row.source, row.target), (21, 22));
    assert_eq!(
        recovered.applied_sequence(),
        recovered.last_logged_sequence()
    );
    recovered.begin_transaction().unwrap();
    recovered.update(id, 31, 32).unwrap();
    recovered.commit().unwrap();
    for _ in 0..2 {
        let (data, log) = recovered.into_inner();
        recovered = open(&data.durable, log.durable, Control::default());
        let row = recovered.inner().get_link(id).unwrap();
        assert_eq!((row.source, row.target), (31, 32));
    }
}

fn reread_failure(operation: &str) {
    for point in POINTS {
        for automatic in [false, true] {
            let (mut tx, id) = failure(point, automatic);
            let read = if operation == "before" {
                Point::ReadBefore
            } else {
                Point::ReadAfter
            };
            tx.log_store_mut().fault.borrow_mut().point = Some(read);
            assert!(tx
                .rollback()
                .unwrap_err()
                .to_string()
                .contains(&format!("{read:?}")));
            assert_eq!(tx.inner().get_link(id).unwrap().source, 21);
            assert!(tx.begin_transaction().is_err());
            assert!(tx.update(id, 51, 52).is_err());
            tx.rollback().unwrap();
            assert_eq!(
                tx.log_store()
                    .entries
                    .iter()
                    .filter(|entry| entry.starts_with("__transactions:commit:"))
                    .count(),
                2
            );
            reopen_and_follow(tx, id);
        }
    }
}

fn write_fence(operation: &str) {
    for point in POINTS {
        for automatic in [false, true] {
            let (mut tx, id) = failure(point, automatic);
            let sequence = tx.last_logged_sequence();
            let outcome = match operation {
                "create" => tx.create(51, 52).map(|_| ()),
                "composite" => tx.create_and_update(51, 52).map(|_| ()),
                "update" => tx.update(id, 51, 52).map(|_| ()),
                "delete" => tx.delete(id).map(|_| ()),
                "ensure" => tx.ensure_created(20).map(|_| ()),
                _ => tx.get_or_create(51, 52).map(|_| ()),
            };
            assert!(
                outcome.is_err(),
                "{point:?}, automatic={automatic}, {operation}"
            );
            assert_eq!(tx.last_logged_sequence(), sequence);
            assert_eq!(tx.inner().get_link(id).unwrap().source, 21);
            assert!(
                tx.begin_transaction().is_err(),
                "checkpoint still owns admission"
            );
            assert!(
                tx.recover().is_err(),
                "recovery cannot replace a pending checkpoint"
            );
            tx.commit().unwrap();
            assert!(tx.create(51, 52).is_ok());
        }
    }
}

macro_rules! contract {
    ($name:ident, $helper:ident, $operation:literal) => {
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        fn $name() {
            $helper($operation);
        }
    };
}
contract!(
    decision_reread_failure_before_preserves_owner,
    reread_failure,
    "before"
);
contract!(
    decision_reread_failure_after_preserves_owner,
    reread_failure,
    "after"
);
contract!(
    commit_retries_every_uncertain_decision_phase,
    cleanup,
    "commit"
);
contract!(
    rollback_finishes_every_uncertain_decision_phase,
    cleanup,
    "rollback"
);
contract!(
    save_finishes_every_uncertain_decision_phase,
    cleanup,
    "save"
);
contract!(create_waits_for_uncertain_decision, write_fence, "create");
contract!(
    composite_waits_for_uncertain_decision,
    write_fence,
    "composite"
);
contract!(update_waits_for_uncertain_decision, write_fence, "update");
contract!(delete_waits_for_uncertain_decision, write_fence, "delete");
contract!(ensure_waits_for_uncertain_decision, write_fence, "ensure");
contract!(
    get_or_create_waits_for_uncertain_decision,
    write_fence,
    "get_or_create"
);
