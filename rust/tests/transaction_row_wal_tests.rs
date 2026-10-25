//! Independent durable images and finite real exits before any row observation.
use link_cli::{
    CommitMode, GenericTransactionsDecorator, Link, LinkError, LinkStorage, LinksStorage,
    LogRetentionPolicy, NamedTypesDecorator, TransitionLogStore,
};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Clone, Default)]
struct Image {
    rows: Vec<Link>,
    names: Vec<Link>,
    journal: Vec<String>,
}
#[derive(Default)]
struct Control {
    image: Image,
    failed: Option<Image>,
    armed: bool,
    exit: bool,
    point: Option<&'static str>,
    skip: usize,
    unobserved: bool,
    hidden_on_name: bool,
    fail_after_unobserved: bool,
    capture_failure: bool,
}
type Shared = Rc<RefCell<Control>>;
fn check(control: &Shared, point: &str) -> Result<(), LinkError> {
    let mut state = control.borrow_mut();
    if state.point != Some(point) {
        return Ok(());
    }
    if state.skip > 0 {
        state.skip -= 1;
        return Ok(());
    }
    state.point = None;
    state.failed = Some(state.image.clone());
    Err(LinkError::StorageError(format!("injected {point}")))
}
struct Store {
    inner: NamedTypesDecorator,
    control: Shared,
}
impl Store {
    fn hidden_effects(&mut self) -> Result<(), LinkError> {
        self.inner.delete_link(2)?;
        let created = self.inner.create_link(51, 52)?;
        self.inner.set_link_name(created, Some("hidden"))?;
        self.inner.set_link_name(0, Some("absent"))?;
        let fail = std::mem::take(&mut self.control.borrow_mut().fail_after_unobserved);
        self.control.borrow_mut().capture_failure = fail;
        Ok(())
    }
    fn after_effect(&mut self) -> Result<(), LinkError> {
        if !std::mem::take(&mut self.control.borrow_mut().armed) {
            return Ok(());
        }
        self.flush()?;
        let image = self.control.borrow().image.clone();
        self.control.borrow_mut().failed = Some(image);
        if self.control.borrow().exit {
            std::process::exit(71);
        }
        Err(LinkError::StorageError(
            "effect persisted before observer".into(),
        ))
    }
}
impl LinksStorage<u32> for Store {
    fn create_link(&mut self, source: u32, target: u32) -> Result<u32, LinkError> {
        let result = self.inner.create_link(source, target)?;
        self.after_effect()?;
        Ok(result)
    }
    fn ensure_link_created(&mut self, index: u32) -> Result<u32, LinkError> {
        let result = self.inner.ensure_link_created(index)?;
        self.after_effect()?;
        Ok(result)
    }
    fn update_link(&mut self, index: u32, source: u32, target: u32) -> Result<Link, LinkError> {
        check(&self.control, "row-before")?;
        let result = self.inner.update_link(index, source, target)?;
        if std::mem::take(&mut self.control.borrow_mut().unobserved) {
            self.hidden_effects()?;
        }
        if self.control.borrow().point == Some("row-after") {
            self.flush()?;
        }
        check(&self.control, "row-after")?;
        self.after_effect()?;
        Ok(result)
    }
    fn delete_link(&mut self, index: u32) -> Result<Link, LinkError> {
        let result = self.inner.delete_link(index)?;
        self.after_effect()?;
        Ok(result)
    }
    fn get_link(&self, index: u32) -> Option<Link> {
        self.inner.get_link(index)
    }
    fn all_links(&self) -> Vec<Link> {
        self.inner.all_links()
    }
    fn capture_names(&mut self) -> Result<Option<BTreeMap<u32, String>>, LinkError> {
        if std::mem::take(&mut self.control.borrow_mut().capture_failure) {
            self.flush()?;
            let mut state = self.control.borrow_mut();
            state.failed = Some(state.image.clone());
            return Err(LinkError::StorageError("injected resulting capture".into()));
        }
        self.inner.capture_names()
    }
    fn set_link_name(&mut self, index: u32, name: Option<&str>) -> Result<u32, LinkError> {
        check(&self.control, "name-before")?;
        let result = self.inner.set_link_name(index, name)?;
        if std::mem::take(&mut self.control.borrow_mut().hidden_on_name) {
            self.hidden_effects()?;
        }
        self.control.borrow_mut().image.names = self.inner.names_links().all_links();
        check(&self.control, "name-after")?;
        Ok(result)
    }
    fn flush(&mut self) -> Result<(), LinkError> {
        check(&self.control, "data-before")?;
        self.inner.flush()?;
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
#[path = "common/transaction_row_wal_faults.rs"]
mod faults;
#[path = "common/transaction_row_wal_recovery.rs"]
mod recovery;
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
fn open(image: Image) -> (Tx, Shared) {
    let control = Rc::new(RefCell::new(Control {
        image: image.clone(),
        ..Default::default()
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
fn capture(tx: &mut Tx) -> String {
    let mut rows = tx.inner().all_links();
    rows.sort_by_key(|row| row.index);
    format!(
        "{rows:?}\n{:?}",
        tx.inner_mut().capture_names().unwrap().unwrap()
    )
}
fn seed(tx: &mut Tx) -> (u32, u32) {
    let first = tx.create_and_update(11, 12).unwrap();
    tx.set_name(first, "original 雪").unwrap();
    let dependent = tx.create_and_update(first, first).unwrap();
    tx.set_name(dependent, "dependent").unwrap();
    tx.set_name(0, "zero").unwrap();
    tx.set_name(77, "absent").unwrap();
    (first, dependent)
}
fn effect(tx: &mut Tx, operation: &str, first: u32, dependent: u32) -> Result<(), LinkError> {
    match operation {
        "create" => tx.create(21, 22).map(|_| ()),
        "update" => tx.update(first, 21, 22).map(|_| ()),
        "delete" => tx.delete(dependent).map(|_| ()),
        "cascade" => tx.delete(first).map(|_| ()),
        "ensure" => tx.ensure_created(5).map(|_| ()),
        _ => panic!("unknown operation"),
    }
}
fn verify(operation: &str) {
    for explicit in [false, true] {
        let (tx, control) = open(Image::default());
        let mut tx = tx
            .with_row_undo(link_cli::transactions::RowUndoPolicy::WriteAheadImage)
            .unwrap();
        let (first, dependent) = seed(&mut tx);
        let mut expected = capture(&mut tx);
        let original = expected.clone();
        if explicit {
            tx.begin_transaction().unwrap();
        }
        control.borrow_mut().armed = true;
        assert!(effect(&mut tx, operation, first, dependent).is_err());
        let mut image = control.borrow().failed.clone().expect("fault must fire");
        // Recovery uses the image taken at failure, before live cleanup.
        for reopen in 0..3 {
            let (mut reopened, state) = open(image);
            assert_eq!(
                capture(&mut reopened),
                expected,
                "{operation}/{explicit}/{reopen}"
            );
            if reopen < 2 {
                let id = reopened.create_and_update(31, 32).unwrap();
                reopened
                    .set_name(id, &format!("following {reopen}"))
                    .unwrap();
                expected = capture(&mut reopened);
            }
            image = state.borrow().image.clone();
        }
        tx.rollback().unwrap();
        tx.rollback().unwrap();
        assert_eq!(capture(&mut tx), original);
        tx.create_and_update(41, 42).unwrap();
    }
}
macro_rules! contract {
    ($name:ident, $operation:literal) => {
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        fn $name() {
            verify($operation);
        }
    };
}
contract!(create_before_observer_recovers_rows_and_names, "create");
contract!(update_before_observer_recovers_rows_and_names, "update");
contract!(delete_before_observer_recovers_rows_and_names, "delete");
contract!(cascade_before_observer_recovers_rows_and_names, "cascade");
contract!(ensure_before_observer_recovers_rows_and_names, "ensure");

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn canonical_undo_images_recover_shared_vectors_and_reserve_sequence_floors() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/transaction-undo-vectors.json"
    ))
    .unwrap();
    for vector in vectors["records"].as_array().unwrap() {
        let wire = vector["wire"].as_str().unwrap();
        assert!(link_cli::transactions::JournalRecord::<u32>::parse(wire).is_err());
        let (mut tx, state) = open(Image {
            rows: vec![Link::new(9, 91, 92)],
            names: Vec::new(),
            journal: vec![format!("__transactions:transition:{wire}")],
        });
        if vector["name"] == "holes and external names" {
            assert_eq!(
                tx.inner().all_links(),
                vec![Link::new(1, 11, 12), Link::new(3, 1, 3)]
            );
            assert_eq!(
                tx.inner_mut().capture_names().unwrap().unwrap(),
                BTreeMap::from([(0, "zero".into()), (1, "雪|\0".into()), (77, "".into())])
            );
        } else {
            assert!(tx.inner().all_links().is_empty());
        }
        assert_eq!(
            state
                .borrow()
                .image
                .journal
                .iter()
                .filter(|entry| entry.starts_with("__transactions:rollback:"))
                .count(),
            1
        );
        let image = state.borrow().image.clone();
        let (mut reopened, _) = open(image);
        assert_eq!(capture(&mut tx), capture(&mut reopened));
        if vector["name"] == "last sequence" {
            assert_eq!(tx.last_logged_sequence(), i64::MAX);
            assert!(tx.create(1, 2).is_err());
        } else {
            tx.create(31, 32).unwrap();
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn invalid_undo_images_preserve_live_rows_names_history_and_retry_admission() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/transaction-undo-vectors.json"
    ))
    .unwrap();
    let mut invalid = vectors["invalid"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    invalid.push(vectors["wide"].as_str().unwrap());
    invalid.push(vectors["unpairedUtf16"].as_str().unwrap());
    for wire in invalid {
        let (mut tx, _) = open(Image::default());
        seed(&mut tx);
        let before = capture(&mut tx);
        let history = tx.journal();
        tx.log_store_mut()
            .entries
            .push(format!("__transactions:transition:{wire}"));
        for _ in 0..2 {
            assert!(tx.recover().is_err(), "accepted invalid image {wire}");
            assert_eq!(capture(&mut tx), before);
            assert_eq!(tx.journal(), history);
            assert!(tx.create(21, 22).is_err());
        }
        tx.log_store_mut().entries.pop();
        tx.recover().unwrap();
        tx.create(31, 32).unwrap();
        tx.recover().unwrap();
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::*;
    use std::{
        path::Path,
        process::{Command, Stdio},
        time::{Duration, Instant},
    };
    fn open_files(
        directory: &Path,
        control: Shared,
    ) -> GenericTransactionsDecorator<u32, Store, link_cli::FileTransitionLog> {
        GenericTransactionsDecorator::new(
            Store {
                inner: NamedTypesDecorator::new(directory.join("data.links"), false).unwrap(),
                control,
            },
            link_cli::FileTransitionLog::open(directory.join("journal.log")).unwrap(),
            LogRetentionPolicy::Infinite,
            CommitMode::Sync,
            true,
        )
        .unwrap()
    }
    #[test]
    fn real_exit_before_observer_recovers_every_operation() {
        for operation in ["create", "update", "delete", "cascade", "ensure"] {
            for explicit in [false, true] {
                let directory = tempfile::tempdir().unwrap();
                let mut child = Command::new(std::env::current_exe().unwrap())
                    .args(["--exact", "native::exit_child", "--ignored", "--nocapture"])
                    .env("ROW_WAL_DIRECTORY", directory.path())
                    .env("ROW_WAL_OPERATION", operation)
                    .env("ROW_WAL_EXPLICIT", explicit.to_string())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap();
                let deadline = Instant::now() + Duration::from_secs(10);
                let status = loop {
                    if let Some(status) = child.try_wait().unwrap() {
                        break status;
                    }
                    if Instant::now() >= deadline {
                        child.kill().unwrap();
                        child.wait().unwrap();
                        panic!("finite row-WAL child expired");
                    }
                    std::thread::sleep(Duration::from_millis(10));
                };
                assert_eq!(status.code(), Some(71), "{operation}/{explicit}");
                let mut expected =
                    std::fs::read_to_string(directory.path().join("expected")).unwrap();
                for reopen in 0..3 {
                    let mut tx = open_files(directory.path(), Rc::default());
                    let mut rows = tx.inner().all_links();
                    rows.sort_by_key(|row| row.index);
                    let actual = format!(
                        "{rows:?}\n{:?}",
                        tx.inner_mut().capture_names().unwrap().unwrap()
                    );
                    assert_eq!(actual, expected, "{operation}/{explicit}/{reopen}");
                    if reopen < 2 {
                        let id = tx.create_and_update(31, 32).unwrap();
                        tx.set_name(id, &format!("following {reopen}")).unwrap();
                        let mut rows = tx.inner().all_links();
                        rows.sort_by_key(|row| row.index);
                        expected = format!(
                            "{rows:?}\n{:?}",
                            tx.inner_mut().capture_names().unwrap().unwrap()
                        );
                    }
                }
            }
        }
    }
    #[test]
    #[ignore = "finite process entry point"]
    fn exit_child() {
        let Some(directory) = std::env::var_os("ROW_WAL_DIRECTORY") else {
            return;
        };
        let directory = Path::new(&directory);
        let control: Shared = Rc::default();
        let mut tx = open_files(directory, control.clone())
            .with_row_undo(link_cli::transactions::RowUndoPolicy::WriteAheadImage)
            .unwrap();
        let first = tx.create_and_update(11, 12).unwrap();
        tx.set_name(first, "original 雪").unwrap();
        let dependent = tx.create_and_update(first, first).unwrap();
        tx.set_name(dependent, "dependent").unwrap();
        tx.set_name(0, "zero").unwrap();
        tx.set_name(77, "absent").unwrap();
        let mut rows = tx.inner().all_links();
        rows.sort_by_key(|row| row.index);
        std::fs::write(
            directory.join("expected"),
            format!(
                "{rows:?}\n{:?}",
                tx.inner_mut().capture_names().unwrap().unwrap()
            ),
        )
        .unwrap();
        if std::env::var("ROW_WAL_EXPLICIT").unwrap() == "true" {
            tx.begin_transaction().unwrap();
        }
        control.borrow_mut().armed = true;
        control.borrow_mut().exit = true;
        match std::env::var("ROW_WAL_OPERATION").unwrap().as_str() {
            "create" => {
                tx.create(21, 22).unwrap();
            }
            "update" => {
                tx.update(first, 21, 22).unwrap();
            }
            "delete" => {
                tx.delete(dependent).unwrap();
            }
            "cascade" => {
                tx.delete(first).unwrap();
            }
            "ensure" => {
                tx.ensure_created(5).unwrap();
            }
            _ => panic!("unknown operation"),
        }
        panic!("exit seam was not reached");
    }
}
