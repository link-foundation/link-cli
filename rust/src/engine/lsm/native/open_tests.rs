use super::{test_support::*, *};
use std::{collections::BTreeMap, fs, path::Path};
fn snapshot(path: &Path) -> BTreeMap<String, Vec<u8>> {
    fs::read_dir(path)
        .unwrap()
        .filter_map(|entry| {
            let entry = entry.unwrap();
            let name = entry.file_name().into_string().unwrap();
            if name == ".lsm-writer" || name.ends_with(".writer") {
                None
            } else {
                Some((name, fs::read(entry.path()).unwrap()))
            }
        })
        .collect()
}
fn opening_cuts(group: usize) {
    for mode in [0, 7] {
        let baseline = tempfile::tempdir().unwrap();
        seed(baseline.path(), mode);
        let trace = Trace::new(baseline.path(), None);
        let mut store = FileLsmJournalStore::open(baseline.path(), options(mode)).unwrap();
        trace.stop();
        store.close().unwrap();
        let boundaries = trace.events.borrow().clone();
        assert!(
            (100..=1024).contains(&boundaries.len()),
            "{} boundaries",
            boundaries.len()
        );
        for operation in [
            "enumeration-open",
            "enumeration-next",
            "enumeration-close",
            "open",
            "close",
        ] {
            assert!(boundaries.iter().any(|event| event.operation == operation));
        }
        for point in (group..boundaries.len()).step_by(8) {
            let directory = tempfile::tempdir().unwrap();
            seed(directory.path(), mode);
            let before = snapshot(directory.path());
            let trace = Trace::new(directory.path(), Some(point));
            let error = FileLsmJournalStore::open(directory.path(), options(mode))
                .err()
                .expect("opening cut must fail");
            assert!(trace.hit.get(), "mode {mode} point {point}: {error}");
            assert!(error.to_string().contains("native LSM operation cut"));
            assert_eq!(trace.events.borrow()[point], boundaries[point]);
            trace.stop();
            assert_eq!(snapshot(directory.path()), before);
            recover(directory.path());
            let mut store = FileLsmJournalStore::open(directory.path(), options(mode)).unwrap();
            assert_eq!(
                values(&mut store)
                    .iter()
                    .map(|v| v.entries[0].as_str())
                    .collect::<Vec<_>>(),
                ["selected", "second", "buffer"]
            );
            store.close().unwrap();
        }
        eprintln!(
            "native Rust opening mode {mode} group {group}/8: {} before/after boundaries",
            boundaries.len()
        );
    }
}
macro_rules! cuts { ($($name:ident: $group:expr),*) => { $(#[test] fn $name() { opening_cuts($group); })* }; }
cuts!(native_opening_cuts_0:0, native_opening_cuts_1:1, native_opening_cuts_2:2, native_opening_cuts_3:3,
      native_opening_cuts_4:4, native_opening_cuts_5:5, native_opening_cuts_6:6, native_opening_cuts_7:7);
