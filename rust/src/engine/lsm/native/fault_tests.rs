use super::{test_support::*, *};
use crate::engine::paged::events;
use std::{fs, io};

fn compaction_cuts(group: usize) {
    for mode in [0, 7] {
        let baseline = tempfile::tempdir().unwrap();
        seed(baseline.path(), mode);
        let mut store = FileLsmJournalStore::open(baseline.path(), options(mode)).unwrap();
        let trace = Trace::new(baseline.path(), None);
        store.compact().unwrap();
        trace.stop();
        store.close().unwrap();
        let boundaries = trace.events.borrow().clone();
        assert!(
            (60..=512).contains(&boundaries.len()),
            "{} boundaries",
            boundaries.len()
        );
        for operation in ["open", "write", "truncate", "sync", "close", "delete"] {
            assert!(
                boundaries.iter().any(|event| event.operation == operation),
                "missing {operation}"
            );
        }
        if cfg!(unix) {
            assert!(boundaries
                .iter()
                .any(|event| event.operation == "directory-sync"));
        }
        for point in (group..boundaries.len()).step_by(8) {
            let directory = tempfile::tempdir().unwrap();
            seed(directory.path(), mode);
            let mut store = FileLsmJournalStore::open(directory.path(), options(mode)).unwrap();
            let expected = values(&mut store);
            let trace = Trace::new(directory.path(), Some(point));
            let error = store.compact().unwrap_err();
            assert!(trace.hit.get(), "mode {mode} point {point}: {error}");
            assert!(error.to_string().contains("native LSM operation cut"));
            assert_eq!(trace.events.borrow()[point], boundaries[point]);
            let attempted = trace.created_ids();
            let count = trace.events.borrow().len();
            assert!(store.read(&["a".into()]).is_err());
            assert_eq!(trace.events.borrow().len(), count);
            let first = store.close().map_err(|e| e.to_string());
            let count = trace.events.borrow().len();
            assert_eq!(store.close().map_err(|e| e.to_string()), first);
            drop(store);
            assert_eq!(trace.events.borrow().len(), count);
            trace.stop();
            recover(directory.path());
            let mut reopened = FileLsmJournalStore::open(directory.path(), options(mode)).unwrap();
            assert_eq!(values(&mut reopened), expected, "mode {mode} point {point}");
            let following = Trace::new(directory.path(), None);
            put(&mut reopened, "d", "following");
            reopened.flush().unwrap();
            for next in following.created_ids() {
                for previous in &attempted {
                    assert!(next > *previous, "reserved identity reused");
                }
            }
            following.stop();
            reopened.close().unwrap();
        }
        eprintln!(
            "native Rust compaction mode {mode} group {group}/8: {} before/after boundaries",
            boundaries.len()
        );
    }
}
macro_rules! cuts { ($($name:ident: $group:expr),*) => { $(#[test] fn $name() { compaction_cuts($group); })* }; }
cuts!(native_compaction_cuts_0:0, native_compaction_cuts_1:1, native_compaction_cuts_2:2, native_compaction_cuts_3:3,
      native_compaction_cuts_4:4, native_compaction_cuts_5:5, native_compaction_cuts_6:6, native_compaction_cuts_7:7);

#[test]
fn discovery_enumeration_primary_and_close_errors_are_aggregated_and_fenced() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().to_owned();
    events::set_failure(Some(Box::new(move |_, _, operation, after| {
        if (operation == "enumeration-next" || operation == "enumeration-close") && !after {
            return Err(io::Error::other(operation.to_owned()));
        }
        Ok(())
    })));
    let error = FileLsmJournalStore::open(&path, options(0))
        .err()
        .unwrap()
        .to_string();
    events::set_failure(None);
    assert!(
        error.contains("enumeration-next") && error.contains("enumeration-close"),
        "{error}"
    );
    assert!(path.join(".lsm-writer").exists());
    assert!(FileLsmJournalStore::open(&path, options(0)).is_err());
    recover(&path);
    FileLsmJournalStore::open(&path, options(0))
        .unwrap()
        .close()
        .unwrap();
}

#[test]
fn native_owner_and_raw_close_uncertainty_is_cached_without_retry() {
    for owner in [false, true] {
        for after in [false, true] {
            let directory = tempfile::tempdir().unwrap();
            let mut store = FileLsmJournalStore::open(directory.path(), options(0)).unwrap();
            let calls = std::rc::Rc::new(std::cell::Cell::new(0));
            let observed = calls.clone();
            events::set_failure(Some(Box::new(move |path, _, operation, completed| {
                observed.set(observed.get() + 1);
                if operation == "close"
                    && completed == after
                    && (path.file_name().unwrap() == ".lsm-writer") == owner
                {
                    return Err(io::Error::other("uncertain native close"));
                }
                Ok(())
            })));
            let error = store.close().unwrap_err().to_string();
            assert!(error.contains("uncertain native close"));
            assert!(directory.path().join(".lsm-writer").exists());
            let count = calls.get();
            assert_eq!(store.close().unwrap_err().to_string(), error);
            drop(store);
            assert_eq!(calls.get(), count);
            events::set_failure(None);
            recover(directory.path());
            FileLsmJournalStore::open(directory.path(), options(0))
                .unwrap()
                .close()
                .unwrap();
        }
    }
}

#[test]
fn native_marker_unlink_failure_only_follows_all_known_closes() {
    for after in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let mut store = FileLsmJournalStore::open(directory.path(), options(0)).unwrap();
        let trace = Trace::new(directory.path(), None);
        store.close().unwrap();
        trace.stop();
        let baseline = trace.events.borrow().clone();
        let point = baseline
            .iter()
            .position(|event| {
                event.name == ".lsm-writer" && event.operation == "delete" && event.after == after
            })
            .unwrap();
        let mut store = FileLsmJournalStore::open(directory.path(), options(0)).unwrap();
        let trace = Trace::new(directory.path(), Some(point));
        assert!(store.close().is_err());
        assert!(trace.hit.get());
        assert_eq!(directory.path().join(".lsm-writer").exists(), !after);
        let count = trace.events.borrow().len();
        assert!(store.close().is_err());
        drop(store);
        assert_eq!(trace.events.borrow().len(), count);
        trace.stop();
        recover(directory.path());
        assert!(fs::read_dir(directory.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".writer")));
    }
}
