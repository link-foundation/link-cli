//! Execute engine bindings and synchronous JavaScript log faults inside WASM.
#![cfg(target_arch = "wasm32")]

use clink_wasm::StagedEngine;
use serde_json::Value;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_test::*;

#[wasm_bindgen(inline_js = r#"
export function make_test_log(mode) {
    return {
        mode, entries: [],
        get supportsDurableFlush() {
            if (this.mode === 'durable-throw') throw 'capability failed';
            if (this.mode === 'durable-string') return 'true';
            return this.mode !== 'volatile';
        },
        read() {
            if (this.mode === 'read-string') throw 'read failed';
            if (this.mode === 'read-number') throw 42;
            if (this.mode === 'read-json') return 'not json';
            if (this.mode === 'read-null') return 'null';
            return JSON.stringify(this.entries);
        },
        append(record) {
            if (this.mode === 'append-string') throw 'append failed';
            if (this.mode === 'append-number') throw 42;
            if (this.mode === 'append-value') return false;
            this.entries.push(record);
        },
        flush() {
            if (this.mode === 'flush-string') throw 'flush failed';
            if (this.mode === 'flush-number') throw 42;
            if (this.mode === 'flush-value') return Promise.resolve();
        },
    };
}
export function set_test_log_mode(log, mode) { log.mode = mode; }
"#)]
extern "C" {
    fn make_test_log(mode: &str) -> JsValue;
    fn set_test_log_mode(log: &JsValue, mode: &str);
}

fn snapshot(engine: &StagedEngine, revision: Option<u64>) -> Value {
    serde_json::from_str(&engine.snapshot(revision).unwrap()).unwrap()
}

#[wasm_bindgen_test]
fn validates_every_binding_option_and_journal_before_opening() {
    assert!(StagedEngine::with_log("[]", make_test_log("ok").unchecked_into()).is_err());
    for options in [
        "invalid",
        "[]",
        r#"{"unknown":true}"#,
        r#"{"concurrency":"invalid"}"#,
        r#"{"stateStructure":"mutable"}"#,
        r#"{"stateStructure":42}"#,
        r#"{"revisionEncoding":"unknown"}"#,
        r#"{"revisionEncoding":42}"#,
        r#"{"isolation":"invalid"}"#,
        r#"{"serializableValidation":"invalid"}"#,
        r#"{"serializableValidation":42}"#,
        r#"{"isolation":"snapshot","serializableValidation":"read-set"}"#,
        r#"{"history":0}"#,
        r#"{"history":-1}"#,
        r#"{"history":1.5}"#,
        r#"{"history":4294967296}"#,
        r#"{"history":"last"}"#,
        r#"{"history":true}"#,
        r#"{"constraints":["invalid"]}"#,
        r#"{"constraintValidation":"invalid"}"#,
        r#"{"constraintValidation":42}"#,
        r#"{"requireDurableFlush":true}"#,
    ] {
        assert!(StagedEngine::new(options, "[]").is_err(), "{options}");
    }
    for journal in ["invalid", "null", "[null]", "[\"corrupt\"]"] {
        assert!(StagedEngine::new("{}", journal).is_err(), "{journal}");
    }
    for options in [
        "{}",
        r#"{"stateStructure":"flat"}"#,
        r#"{"constraintValidation":"scan"}"#,
        r#"{"constraintValidation":"indexed","constraints":["references","unique-pairs","unique-names"]}"#,
        r#"{"stateStructure":"shared"}"#,
        r#"{"stateStructure":"sorted-runs"}"#,
        r#"{"revisionEncoding":"snapshot"}"#,
        r#"{"revisionEncoding":"changes","stateStructure":"shared"}"#,
        r#"{"history":null}"#,
        r#"{"serializableValidation":"revision"}"#,
        r#"{"serializableValidation":"read-set"}"#,
        r#"{"concurrency":"exclusive","serializableValidation":"read-set"}"#,
        r#"{"history":2}"#,
        r#"{"history":"all","isolation":"snapshot","concurrency":"optimistic"}"#,
        r#"{"isolation":"read-committed","concurrency":"exclusive","trace":true}"#,
        r#"{"isolation":"snapshot","concurrency":"pessimistic"}"#,
        r#"{"concurrency":"pessimistic","serializableValidation":"read-set"}"#,
        r#"{"constraints":["references","unique-pairs","unique-names"]}"#,
    ] {
        assert!(StagedEngine::new(options, "[]").is_ok(), "{options}");
    }
}

#[wasm_bindgen_test]
fn sessions_preserve_visibility_history_and_closed_state() {
    let engine = StagedEngine::new(r#"{"history":"all"}"#, "[]").unwrap();
    let mut first = engine.begin().unwrap();
    assert!(!first.request("(create: (0 0))").contains("error:"));
    assert_eq!(first.count(), 1);
    assert_eq!(engine.begin().unwrap().count(), 0);
    assert_eq!(first.commit().unwrap(), 1);
    assert!(first.commit().is_err());
    assert!(first.request("(create: (0 0))").contains("error:"));
    assert!(first.request("(").contains("error:"));
    first.rollback();
    assert_eq!(snapshot(&engine, None)["revision"], "1");
    assert_eq!(snapshot(&engine, Some(0))["rows"], serde_json::json!([]));
    assert!(engine.snapshot(Some(99)).is_err());
    let reopened = StagedEngine::new("{}", &engine.journal().unwrap()).unwrap();
    assert_eq!(reopened.begin().unwrap().count(), 1);
    let mut discarded = engine.begin().unwrap();
    discarded.request("(create: (1 1))");
    discarded.rollback();
    discarded.rollback();
    assert!(discarded.commit().is_err());
    assert_eq!(engine.begin().unwrap().count(), 1);
}

#[wasm_bindgen_test]
fn checkpoint_exports_retained_snapshots_without_log_effects_and_rejects_poison() {
    let log = make_test_log("ok");
    let engine = StagedEngine::with_log(r#"{"history":2}"#, log.clone().unchecked_into()).unwrap();
    for _ in 0..3 {
        let mut tx = engine.begin().unwrap();
        assert!(!tx.request("(create: (0 0))").contains("error:"));
        tx.commit().unwrap();
    }
    let original = engine.journal().unwrap();
    let exported = engine.export_checkpoint().unwrap();
    assert_eq!(
        serde_json::from_str::<Vec<String>>(&exported)
            .unwrap()
            .len(),
        2
    );
    assert_eq!(engine.journal().unwrap(), original);
    let copy = StagedEngine::new(r#"{"history":"all"}"#, &exported).unwrap();
    assert_eq!(snapshot(&copy, None), snapshot(&engine, None));
    assert_eq!(snapshot(&copy, Some(2)), snapshot(&engine, Some(2)));
    assert!(copy.snapshot(Some(0)).is_err());
    set_test_log_mode(&log, "flush-string");
    let mut tx = engine.begin().unwrap();
    tx.request("(create: (0 0))");
    assert!(tx.commit().is_err());
    assert!(engine.export_checkpoint().is_err());
}

#[wasm_bindgen_test]
fn admission_and_conflict_errors_cross_the_wasm_boundary() {
    let exclusive = StagedEngine::new(r#"{"concurrency":"exclusive"}"#, "[]").unwrap();
    let mut held = exclusive.begin().unwrap();
    assert!(exclusive.begin().is_err());
    held.rollback();
    assert!(exclusive.begin().is_ok());

    let engine = StagedEngine::new("{}", "[]").unwrap();
    let mut first = engine.begin().unwrap();
    let mut stale = engine.begin().unwrap();
    first.request("(create: (0 0))");
    stale.request("(create: (0 0))");
    first.commit().unwrap();
    assert!(stale.commit().unwrap_err().contains("conflict"));
    assert_eq!(engine.begin().unwrap().count(), 1);
}

#[wasm_bindgen_test]
fn independent_branches_merge_and_resume_with_a_saved_base() {
    let engine = StagedEngine::new(r#"{"history":"all"}"#, "[]").unwrap();
    let mut initial = engine.begin().unwrap();
    initial.request("(create: (0 0))");
    initial.commit().unwrap();
    let mut branch = engine.fork("{}", "[]").unwrap();
    let child = branch.engine();
    let base = branch.merge_base_record();
    let mut changed = child.begin().unwrap();
    changed.request("(create: (1 1))");
    changed.commit().unwrap();
    assert_eq!(engine.begin().unwrap().count(), 1);
    let mut resumed = engine
        .resume("{}", &child.journal().unwrap(), &base)
        .unwrap();
    assert_eq!(resumed.merge().unwrap(), 2);
    assert_eq!(resumed.merge().unwrap(), 2);
    assert_eq!(engine.begin().unwrap().count(), 2);
    assert!(branch.merge().is_ok());
    assert!(engine.fork("invalid", "[]").is_err());
    assert!(engine.fork("{}", &child.journal().unwrap()).is_err());
    assert!(engine.fork("{}", "invalid").is_err());
    assert!(engine.resume("{}", "invalid", &base).is_err());
    assert!(engine.resume("invalid", "[]", &base).is_err());
    assert!(engine.resume("{}", "[]", "invalid").is_err());
}

#[wasm_bindgen_test]
fn conflicting_branch_merge_does_not_publish_partial_changes() {
    let engine = StagedEngine::new("{}", "[]").unwrap();
    let mut initial = engine.begin().unwrap();
    initial.request("(create: (0 0))");
    initial.commit().unwrap();
    let mut branch = engine.fork("{}", "[]").unwrap();
    let mut child = branch.engine().begin().unwrap();
    assert!(!child.request("(set-name: (1 'child'))").contains("error:"));
    child.commit().unwrap();
    let mut parent = engine.begin().unwrap();
    assert!(!parent
        .request("(set-name: (1 'parent'))")
        .contains("error:"));
    parent.commit().unwrap();
    assert!(branch.merge().unwrap_err().contains("conflict"));
    assert_eq!(snapshot(&engine, None)["rows"][0]["name"], "parent");
}

#[wasm_bindgen_test]
fn javascript_logs_round_trip_and_back_independent_branches() {
    let log = make_test_log("ok");
    let engine = StagedEngine::with_log(
        r#"{"requireDurableFlush":true}"#,
        log.clone().unchecked_into(),
    )
    .unwrap();
    let mut first = engine.begin().unwrap();
    first.request("(create: (0 0))");
    first.commit().unwrap();
    let reopened = StagedEngine::with_log("{}", log.clone().unchecked_into()).unwrap();
    assert_eq!(reopened.begin().unwrap().count(), 1);
    assert_eq!(engine.journal().unwrap(), reopened.journal().unwrap());

    let child_log = make_test_log("ok");
    let branch = engine
        .fork_with_log("{}", child_log.clone().unchecked_into())
        .unwrap();
    let base = branch.merge_base_record();
    let mut child = branch.engine().begin().unwrap();
    child.request("(create: (1 1))");
    child.commit().unwrap();
    let mut resumed = engine
        .resume_with_log("{}", child_log.unchecked_into(), &base)
        .unwrap();
    resumed.merge().unwrap();
    assert_eq!(engine.begin().unwrap().count(), 2);
    set_test_log_mode(&log, "read-string");
    assert!(engine.journal().is_err());
}

#[wasm_bindgen_test]
fn javascript_log_failures_poison_uncertain_commits_without_publishing() {
    for mode in [
        "append-string",
        "append-number",
        "append-value",
        "flush-string",
        "flush-number",
        "flush-value",
    ] {
        let log = make_test_log("ok");
        let engine = StagedEngine::with_log("{}", log.clone().unchecked_into()).unwrap();
        let mut session = engine.begin().unwrap();
        session.request("(create: (0 0))");
        set_test_log_mode(&log, mode);
        assert!(session.commit().is_err(), "{mode}");
        assert!(engine.begin().is_err(), "{mode}");
        set_test_log_mode(&log, "ok");
        let reopened = StagedEngine::with_log("{}", log.unchecked_into()).unwrap();
        assert_eq!(
            reopened.begin().unwrap().count(),
            usize::from(mode.starts_with("flush"))
        );
    }
    for mode in ["read-string", "read-number", "read-json", "read-null"] {
        assert!(
            StagedEngine::with_log("{}", make_test_log(mode).unchecked_into()).is_err(),
            "{mode}"
        );
    }
    for mode in ["volatile", "durable-throw", "durable-string"] {
        assert!(
            StagedEngine::with_log(
                r#"{"requireDurableFlush":true}"#,
                make_test_log(mode).unchecked_into()
            )
            .is_err(),
            "{mode}"
        );
    }
}

#[wasm_bindgen_test]
fn reclaimed_generations_preserve_aba_and_branch_conflicts() {
    for isolation in ["snapshot", "read-committed"] {
        let options = format!(r#"{{"isolation":"{isolation}"}}"#);
        let engine = StagedEngine::new(&options, "[]").unwrap();
        let mut seed = engine.begin().unwrap();
        assert!(!seed.request("(create: (0 0))").contains("error:"));
        seed.commit().unwrap();
        let mut stale = engine.begin().unwrap();
        stale.request("(set-name: (1 'stale'))");
        let mut writer = engine.begin().unwrap();
        writer.request("(set-name: (1 'temporary'))");
        writer.commit().unwrap();
        let mut restored = engine.begin().unwrap();
        assert!(!restored.request("(remove-name: (1))").contains("error:"));
        restored.commit().unwrap();
        assert!(stale.commit().unwrap_err().contains("conflict"));
        assert_eq!(snapshot(&engine, None)["rows"][0]["name"], Value::Null);

        let mut branch = engine.fork("{}", "[]").unwrap();
        let mut pending = engine.begin().unwrap();
        pending.request("(set-name: (1 'pending'))");
        let mut child = branch.engine().begin().unwrap();
        child.request("(set-name: (1 'child'))");
        child.commit().unwrap();
        branch.merge().unwrap();
        assert!(pending.commit().unwrap_err().contains("conflict"));
        assert_eq!(snapshot(&engine, None)["rows"][0]["name"], "child");
        let reopened = StagedEngine::new(&options, &engine.journal().unwrap()).unwrap();
        let mut next = reopened.begin().unwrap();
        next.request("(set-name: (1 'after reopen'))");
        next.commit().unwrap();
        assert_eq!(snapshot(&reopened, None)["rows"][0]["name"], "after reopen");
    }
}

#[wasm_bindgen_test]
fn read_set_validation_allows_points_and_protects_dependencies_and_scans() {
    let engine = StagedEngine::new(r#"{"serializableValidation":"read-set"}"#, "[]").unwrap();
    let mut seed = engine.begin().unwrap();
    seed.request("(create: (0 0))");
    seed.request("(create: (0 0))");
    seed.commit().unwrap();
    let mut a = engine.begin().unwrap();
    let mut b = engine.begin().unwrap();
    a.request("(set-name: (1 'one'))");
    b.request("(set-name: (2 'two'))");
    assert_eq!(a.commit().unwrap(), 2);
    assert_eq!(b.commit().unwrap(), 3);
    let mut c = engine.begin().unwrap();
    let mut d = engine.begin().unwrap();
    c.request("(get-name: (2))");
    d.request("(get-name: (1))");
    c.request("(remove-name: (1))");
    d.request("(remove-name: (2))");
    c.commit().unwrap();
    assert!(d.commit().unwrap_err().contains("read"));
    let mut scan = engine.begin().unwrap();
    assert_eq!(scan.count(), 2);
    let mut writer = engine.begin().unwrap();
    writer.request("(set-name: (2 'later'))");
    writer.commit().unwrap();
    assert!(scan.commit().unwrap_err().contains("serializable"));
    let mut missing = engine.begin().unwrap();
    missing.request("(get-name: (3))");
    let mut inserted = engine.begin().unwrap();
    inserted.request("(create: (0 0))");
    inserted.commit().unwrap();
    // The next allocator address was absent when the reader observed it.
    assert!(snapshot(&engine, None)["rows"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["index"] == "3"));
    assert!(missing.commit().unwrap_err().contains("read"));
}
