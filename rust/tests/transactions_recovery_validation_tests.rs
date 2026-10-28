//! A recognized but contradictory journal cannot choose an arbitrary outcome.
use link_cli::{
    CommitMode, GenericTransactionsDecorator, LinkError, LinkStorage, LogRetentionPolicy,
    Transition, TransitionLogStore,
};

#[derive(Default)]
struct Journal {
    entries: Vec<String>,
}
impl TransitionLogStore for Journal {
    fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
        self.entries.push(entry.into());
        Ok(())
    }
    fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
        Ok(self.entries.clone())
    }
    fn flush_log(&mut self) -> Result<(), LinkError> {
        Ok(())
    }
}
fn record(owner: u128, sequence: i64, after: u32) -> String {
    format!("__transactions:transition:v1|{owner:032x}|{sequence}|0|1|1,11,12|1,{after},22")
}
fn commit(owner: u128) -> String {
    format!("__transactions:commit:{owner:032x}")
}
fn abort(owner: u128) -> String {
    format!("__transactions:rollback:{owner:032x}")
}
fn invalid(case: &str) -> Vec<String> {
    let transition = record(1, 1, 21);
    match case {
        "commit-abort" => vec![transition, commit(1), abort(1)],
        "abort-commit" => vec![transition, abort(1), commit(1)],
        "owners" => vec![transition, commit(1), record(2, 1, 31), commit(2)],
        "payloads" => vec![transition, record(1, 1, 31), commit(1)],
        "timestamp" => vec![
            transition.clone(),
            transition.replace("|1|0|1|", "|1|1|1|"),
            commit(1),
        ],
        "zero-sequence" => vec![record(1, 0, 21), commit(1)],
        "negative-sequence" => vec![record(1, -1, 21), commit(1)],
        "zero-owner" => vec![record(0, 1, 21), commit(0)],
        "zero-commit" => vec![commit(0)],
        "zero-abort" => vec![abort(0)],
        "retained-contradiction" => vec![commit(1), abort(1)],
        "malformed-transition" => vec!["__transactions:transition:invalid".into()],
        "malformed-commit" => vec!["__transactions:commit:invalid".into()],
        "malformed-applied" => vec!["__transactions:applied:0".into()],
        _ => panic!("unknown case {case}"),
    }
}
fn reject(case: &str) {
    let mut rows = LinkStorage::in_memory(false);
    let row = rows.create(11, 12);
    let mut tx = GenericTransactionsDecorator::<u32, _, _>::new(
        rows,
        Journal::default(),
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        true,
    )
    .unwrap();
    // Malformed input must preserve populated live indexes as well as rows.
    if case.starts_with("malformed-") {
        tx.update(row, 31, 32).unwrap();
        tx.update(row, 11, 12).unwrap();
        assert!(!tx.log().is_empty());
    }
    let previous = tx.log().to_vec();
    let previous_sequence = tx.last_logged_sequence();
    let entries = invalid(case);
    tx.log_store_mut().entries = entries.clone();
    for _ in 0..2 {
        assert!(
            tx.recover().is_err(),
            "{case}: contradictory journal accepted"
        );
        let state = tx.get(row).unwrap();
        assert_eq!((state.source, state.target), (11, 12), "{case}");
        assert_eq!(tx.log_store().entries, entries, "{case}");
        assert_eq!(
            tx.log(),
            previous.as_slice(),
            "rejected parse replaced live indexes"
        );
        assert_eq!(tx.last_logged_sequence(), previous_sequence);
        assert!(
            tx.begin_transaction().is_err(),
            "failed recovery released admission"
        );
        assert!(tx.create(41, 42).is_err());
        assert!(tx.update(row, 41, 42).is_err());
        assert!(tx.delete(row).is_err());
        assert!(tx.ensure_created(20).is_err());
        assert!(tx.get_or_create(41, 42).is_err());
        let change = Transition::parse(
            record(1, 1, 21)
                .strip_prefix("__transactions:transition:")
                .unwrap(),
        )
        .unwrap();
        assert!(tx.apply_transition(&change).is_err());
        assert!(tx.revert_transition(&change).is_err());
        assert!(tx.save().is_err());
        assert!(tx.commit().is_err());
        assert!(tx.rollback().is_err());
        let state = tx.get(row).unwrap();
        assert_eq!((state.source, state.target), (11, 12));
        assert_eq!(tx.log_store().entries, entries);
    }
    // The owner is reachable: repair the external journal, then retry recovery.
    tx.log_store_mut().entries.clear();
    tx.recover().unwrap();
    tx.update(row, 41, 42).unwrap();
    for _ in 0..2 {
        tx.recover().unwrap();
        assert_eq!(tx.get(row).unwrap().source, 41);
    }
}
macro_rules! case {
    ($name:ident, $label:literal) => {
        #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
        #[cfg_attr(not(target_arch = "wasm32"), test)]
        fn $name() {
            reject($label);
        }
    };
}
case!(reject_commit_followed_by_abort, "commit-abort");
case!(reject_abort_followed_by_commit, "abort-commit");
case!(reject_two_owners_of_one_sequence, "owners");
case!(reject_two_payloads_for_one_sequence, "payloads");
case!(reject_conflicting_timestamps, "timestamp");
case!(reject_zero_sequence, "zero-sequence");
case!(reject_negative_sequence, "negative-sequence");
case!(reject_zero_owner, "zero-owner");
case!(reject_zero_commit_owner, "zero-commit");
case!(reject_zero_abort_owner, "zero-abort");
case!(
    reject_retained_contradictory_decisions,
    "retained-contradiction"
);
case!(
    retain_owner_after_malformed_transition,
    "malformed-transition"
);
case!(retain_owner_after_malformed_commit, "malformed-commit");
case!(
    retain_owner_after_malformed_applied_marker,
    "malformed-applied"
);

fn duplicate_control(decision: &str) {
    let mut rows = LinkStorage::in_memory(false);
    let row = if decision == "abort" {
        rows.create(11, 12)
    } else {
        rows.create(21, 22)
    };
    let mut entries = vec![record(1, 3, 21), record(1, 3, 21)];
    // Keep the public legacy parser's accepted extra fields and numeric forms.
    entries.push(record(1, 3, 21).replace("|3|0|", "|03|00|") + "|legacy-extension");
    entries.push("unrelated application name".into());
    if decision != "pending" {
        let marker = if decision == "abort" {
            abort(1)
        } else {
            commit(1)
        };
        entries.extend([marker.clone(), marker]);
    }
    let mut tx = GenericTransactionsDecorator::<u32, _, _>::new(
        rows,
        Journal { entries },
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap();
    assert_eq!(
        tx.log().len(),
        1,
        "one logical sequence despite idempotent retries"
    );
    let expected = if decision == "commit" {
        (21, 22)
    } else {
        (11, 12)
    };
    for _ in 0..2 {
        tx.recover().unwrap();
        let state = tx.get(row).unwrap();
        assert_eq!((state.source, state.target), expected);
    }
    tx.update(row, 41, 42).unwrap();
    assert_eq!(tx.last_logged_sequence(), 4);
    tx.recover().unwrap();
    assert_eq!(tx.get(row).unwrap().source, 41);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn repeated_identical_commits_and_records_are_idempotent() {
    duplicate_control("commit");
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn repeated_identical_aborts_and_records_are_idempotent() {
    duplicate_control("abort");
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn repeated_pending_records_are_undone_once() {
    duplicate_control("pending");
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn retained_applied_checkpoints_reserve_gaps_without_payloads() {
    let mut rows = LinkStorage::in_memory(false);
    let row = rows.create(21, 22);
    let mut tx = GenericTransactionsDecorator::<u32, _, _>::new(
        rows,
        Journal {
            entries: vec![
                commit(1),
                commit(1),
                "__transactions:applied:42".into(),
                "__transactions:applied:42".into(),
            ],
        },
        LogRetentionPolicy::Infinite,
        CommitMode::Sync,
        false,
    )
    .unwrap();
    assert_eq!(tx.applied_sequence(), 42);
    assert_eq!(tx.last_logged_sequence(), 42);
    tx.update(row, 41, 42).unwrap();
    assert_eq!(tx.last_logged_sequence(), 43);
    tx.recover().unwrap();
    assert_eq!(tx.get(row).unwrap().source, 41);
}
