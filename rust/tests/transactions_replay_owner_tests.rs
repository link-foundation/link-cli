//! Public unlogged replay cannot bypass a live transaction owner.
use link_cli::{
    engine::MemoryLog, CommitMode, GenericTransactionsDecorator, Link, LinkError, LinkStorage,
    LinksStorage, LinksStorageRef, LogRetentionPolicy,
};
use std::{cell::Cell, rc::Rc};

struct Store {
    rows: LinkStorage,
    fail: Rc<Cell<bool>>,
}
impl LinksStorage<u32> for Store {
    fn create_link(&mut self, source: u32, target: u32) -> Result<u32, LinkError> {
        Ok(self.rows.create(source, target))
    }
    fn get_link(&self, id: u32) -> Option<Link> {
        self.rows.get(id).copied()
    }
    fn ensure_link_created(&mut self, id: u32) -> Result<u32, LinkError> {
        LinksStorage::ensure_link_created(&mut self.rows, id)
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
        if self.fail.replace(false) {
            Err(LinkError::StorageError("injected checkpoint".into()))
        } else {
            Ok(())
        }
    }
}

impl LinksStorageRef<u32> for Store {
    fn get_link_ref(&self, id: u32) -> Option<&Link> {
        self.rows.get(id)
    }
    fn all_link_refs(&self) -> Vec<&Link> {
        self.rows.all()
    }
}

fn verify(revert: bool, phase: &str) {
    let fail = Rc::new(Cell::new(false));
    let mut tx = GenericTransactionsDecorator::new(
        Store {
            rows: LinkStorage::in_memory(false),
            fail: fail.clone(),
        },
        MemoryLog::default(),
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap();
    let id = tx.create_and_update(11, 12).unwrap();
    tx.update(id, 21, 22).unwrap();
    let previous = *tx.log().last().unwrap();
    tx.begin_transaction().unwrap();
    tx.update(id, 31, 32).unwrap();
    if phase != "pending" {
        fail.set(true);
        let outcome = if phase == "commit" {
            tx.commit()
        } else {
            tx.rollback()
        };
        assert!(outcome
            .unwrap_err()
            .to_string()
            .contains("injected checkpoint"));
    }
    let sequence = tx.last_logged_sequence();
    let source = tx.get(id).unwrap().source;
    let outcome = if revert {
        tx.revert_transition(&previous)
    } else {
        tx.apply_transition(&previous)
    };
    assert!(outcome.is_err(), "unlogged replay bypassed the live owner");
    assert_eq!(tx.get(id).unwrap().source, source);
    assert_eq!(tx.last_logged_sequence(), sequence);
    assert!(tx.begin_transaction().is_err(), "replay released the owner");
    if phase == "commit" {
        tx.commit().unwrap();
    } else {
        tx.rollback().unwrap();
    }
    assert_eq!(
        tx.get(id).unwrap().source,
        if phase == "commit" { 31 } else { 21 }
    );
    // Version control still needs public replay when no owner is live.
    tx.revert_transition(&previous).unwrap();
    assert_eq!(tx.get(id).unwrap().source, 11);
    tx.apply_transition(&previous).unwrap();
    assert_eq!(tx.get(id).unwrap().source, 21);
    assert_eq!(tx.last_logged_sequence(), sequence);
    tx.update(id, 41, 42).unwrap();
    assert_eq!(tx.get(id).unwrap().source, 41);
}
macro_rules! case {
    ($name:ident, $revert:expr, $phase:expr) => {
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        fn $name() {
            verify($revert, $phase);
        }
    };
}
case!(apply_rejects_live_owner, false, "pending");
case!(revert_rejects_live_owner, true, "pending");
case!(apply_rejects_failed_commit_owner, false, "commit");
case!(revert_rejects_failed_commit_owner, true, "commit");
case!(apply_rejects_failed_rollback_owner, false, "rollback");
case!(revert_rejects_failed_rollback_owner, true, "rollback");
