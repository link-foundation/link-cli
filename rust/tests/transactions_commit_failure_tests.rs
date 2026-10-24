//! Retry durable decisions before admitting another transaction or write.
//! The volatile fault stores model separate persisted data and journal images;
//! the real-filesystem version owner contract is tested separately.
use link_cli::{
    CommitMode, GenericTransactionsDecorator, Link, LinkError, LinkStorage, LinksStorage,
    LogRetentionPolicy, TransitionLogStore,
};
use std::{cell::RefCell, rc::Rc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Point {
    DataBefore,
    DataAfter,
    MarkerBefore,
    MarkerAfter,
    LogBefore,
    LogAfter,
}
const POINTS: [Point; 6] = [
    Point::DataBefore,
    Point::DataAfter,
    Point::MarkerBefore,
    Point::MarkerAfter,
    Point::LogBefore,
    Point::LogAfter,
];
#[derive(Default)]
struct Fault {
    point: Option<Point>,
    flushes: usize,
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
    fault: Control,
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
        self.fault.borrow_mut().check(Point::DataBefore)?;
        self.durable = self.all_links();
        self.fault.borrow_mut().check(Point::DataAfter)
    }
}
struct Log {
    entries: Vec<String>,
    durable: Vec<String>,
    fault: Control,
}
impl TransitionLogStore for Log {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        let applied = entry.starts_with("__transactions:applied:");
        if applied {
            self.fault.borrow_mut().check(Point::MarkerBefore)?;
        }
        self.entries.push(entry.into());
        if applied {
            self.fault.borrow_mut().check(Point::MarkerAfter)?;
        }
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.entries.clone())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        let checkpoint = {
            let mut fault = self.fault.borrow_mut();
            fault.flushes += 1;
            fault.flushes > 1
        };
        if checkpoint {
            self.fault.borrow_mut().check(Point::LogBefore)?;
        }
        self.durable = self.entries.clone();
        if checkpoint {
            self.fault.borrow_mut().check(Point::LogAfter)?;
        }
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
            fault: fault.clone(),
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
    *fault.borrow_mut() = Fault {
        point: Some(point),
        flushes: 0,
    };
    let outcome = if automatic {
        tx.update(id, 21, 22).map(|_| ())
    } else {
        tx.commit()
    };
    assert!(outcome.is_err(), "{point:?}, automatic={automatic}");
    let committed = tx
        .log_store()
        .durable
        .iter()
        .filter(|e| e.starts_with("__transactions:commit:"))
        .count();
    assert_eq!(
        committed, 2,
        "the failed call has a persisted commit decision"
    );
    assert_eq!(tx.inner().get_link(id).unwrap().source, 21);
    (tx, id)
}

fn cleanup(operation: &str) {
    for point in POINTS {
        for automatic in [false, true] {
            let (mut tx, id) = failure(point, automatic);
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
                "known commit must not be undone"
            );
            tx.begin_transaction().unwrap();
            tx.update(id, 31, 32).unwrap();
            tx.commit().unwrap();
            let (data, log) = tx.into_inner();
            let recovered = open(&data.durable, log.durable, Control::default());
            let row = recovered.inner().get_link(id).unwrap();
            assert_eq!(
                (row.source, row.target),
                (31, 32),
                "recovery cannot replay an older pending checkpoint over a later commit"
            );
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
    commit_retries_every_committed_checkpoint_phase,
    cleanup,
    "commit"
);
contract!(
    rollback_finishes_every_committed_checkpoint_phase,
    cleanup,
    "rollback"
);
contract!(
    save_finishes_every_committed_checkpoint_phase,
    cleanup,
    "save"
);
contract!(create_waits_for_committed_checkpoint, write_fence, "create");
contract!(
    composite_waits_for_committed_checkpoint,
    write_fence,
    "composite"
);
contract!(update_waits_for_committed_checkpoint, write_fence, "update");
contract!(delete_waits_for_committed_checkpoint, write_fence, "delete");
contract!(ensure_waits_for_committed_checkpoint, write_fence, "ensure");
contract!(
    get_or_create_waits_for_committed_checkpoint,
    write_fence,
    "get_or_create"
);
