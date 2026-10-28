use super::*;
use crate::engine::{lsm::LsmOptions, paged::events};
use std::{
    cell::{Cell, RefCell},
    fs, io,
    path::Path,
    rc::Rc,
};

pub(super) fn options(mode: usize) -> FileLsmOptions {
    FileLsmOptions {
        store: LsmStoreOptions {
            limits: LsmOptions {
                max_keys: 32,
                max_bytes: 4096,
                buffer_keys: 2,
                buffer_bytes: 128,
                max_runs: 2,
                max_wal_records: 4,
                max_wal_bytes: 8192,
            },
            max_namespace_entries: 16,
        },
        max_pages: 128,
        mirrored_root: mode & 1 != 0,
        mirrored_banks: mode & 2 != 0,
        mirrored_tables: mode & 4 != 0,
    }
}
pub(super) fn put(store: &mut FileLsmJournalStore, key: &str, value: &str) {
    let keys = vec![key.into()];
    let expected = store.read(&keys).unwrap();
    assert!(store
        .compare_exchange(&keys, &expected, &[Some(vec![value.into()])])
        .unwrap());
}
pub(super) fn seed(path: &Path, mode: usize) {
    let mut store = FileLsmJournalStore::open(path, options(mode)).unwrap();
    put(&mut store, "a", "old");
    put(&mut store, "b", "second");
    store.flush().unwrap();
    put(&mut store, "a", "selected");
    store.flush().unwrap();
    put(&mut store, "c", "buffer");
    store.close().unwrap();
}
pub(super) fn values(store: &mut FileLsmJournalStore) -> Vec<JournalVersion> {
    store.read(&["a".into(), "b".into(), "c".into()]).unwrap()
}
pub(super) fn recover(path: &Path) {
    events::set_failure(None);
    events::release_retained();
    // Explicit external recovery: every facade has already closed/dropped and
    // the harness has closed resources whose ownership was lost at a cut.
    for entry in fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name().into_string().unwrap();
        if name == ".lsm-writer" || name.ends_with(".writer") {
            fs::remove_file(entry.path()).unwrap();
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Event {
    pub name: String,
    pub index: usize,
    pub operation: String,
    pub after: bool,
}
pub(super) struct Trace {
    pub events: Rc<RefCell<Vec<Event>>>,
    pub hit: Rc<Cell<bool>>,
}
impl Trace {
    pub fn new(path: &Path, point: Option<usize>) -> Self {
        let result = Self {
            events: Rc::new(RefCell::new(Vec::new())),
            hit: Rc::new(Cell::new(false)),
        };
        let observed = result.events.clone();
        let hit = result.hit.clone();
        let root = path.to_owned();
        events::set_failure(Some(Box::new(move |path, index, operation, after| {
            let mut trace = observed.borrow_mut();
            assert!(trace.len() < 4096, "finite native trace bound");
            trace.push(Event {
                name: if path == root {
                    "<directory>".into()
                } else {
                    path.file_name().unwrap().to_string_lossy().into()
                },
                index,
                operation: operation.into(),
                after,
            });
            if point == Some(trace.len() - 1) {
                hit.set(true);
                return Err(io::Error::other("native LSM operation cut"));
            }
            Ok(())
        })));
        result
    }
    pub fn stop(&self) {
        events::set_failure(None);
    }
    pub fn created_ids(&self) -> Vec<u64> {
        self.events
            .borrow()
            .iter()
            .filter(|event| event.operation == "open")
            .filter_map(|event| {
                event
                    .name
                    .strip_prefix("table-")?
                    .strip_suffix(".writer")?
                    .parse()
                    .ok()
            })
            .collect()
    }
}
impl Drop for Trace {
    fn drop(&mut self) {
        events::set_failure(None);
        events::release_retained();
    }
}
