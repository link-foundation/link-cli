//! Binding ownership and synchronous JS journal failures on an actual WASM VM.
#![cfg(target_arch = "wasm32")]
use clink_wasm::StagedEngine;
use serde_json::{json, Value};
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_test::*;

#[wasm_bindgen(inline_js = r#"
export function group_log(phase, index, after, writeThrough) {
  return {
    supportsDurableFlush: true, entries: [], stable: [], appends: 0, flushes: 0,
    read() { return JSON.stringify(this.entries); },
    append(record) {
      const fail = phase === 'append' && ++this.appends === index;
      if (fail && !after) throw 'before append';
      this.entries.push(record);
      if (writeThrough) this.stable = [...this.entries];
      if (fail) throw 'after append';
    },
    flush() {
      this.flushes++;
      if (phase === 'flush' && !after) throw 'before flush';
      this.stable = [...this.entries];
      if (phase === 'flush') throw 'after flush';
    },
  };
}
export function group_log_stats(log) {
  return JSON.stringify({entries: log.entries, stable: log.stable, flushes: log.flushes});
}
"#)]
extern "C" {
    fn group_log(phase: &str, index: u32, after: bool, write_through: bool) -> JsValue;
    fn group_log_stats(log: &JsValue) -> String;
}

fn parse(text: &str) -> Value {
    serde_json::from_str(text).unwrap()
}

#[wasm_bindgen_test]
fn groups_keep_revisions_history_and_known_aborts_with_one_js_flush() {
    let log = group_log("ok", 1, false, false);
    let engine = StagedEngine::with_log(r#"{"isolation":"snapshot","history":"all","constraints":["references"],"requireDurableFlush":true,"trace":true}"#, log.clone().unchecked_into()).unwrap();
    let mut first = engine.begin().unwrap();
    first.request("(create: (0 0))");
    let mut bad = engine.begin().unwrap();
    bad.request("(create: (999 0))");
    let mut last = engine.begin().unwrap();
    last.request("(create: (1 0))");
    let mut group = engine.group();
    group.add(first).unwrap();
    group.add(bad).unwrap();
    group.add(last).unwrap();
    assert_eq!(parse(&engine.snapshot(None).unwrap())["revision"], "0");
    let result = parse(&group.commit().unwrap());
    assert_eq!(result[0], json!({"status":"committed","revision":"1"}));
    assert_eq!(result[1]["status"], "aborted");
    assert_eq!(result[2], json!({"status":"committed","revision":"2"}));
    assert_eq!(parse(&group_log_stats(&log))["flushes"], 1);
    assert_eq!(
        parse(&engine.snapshot(Some(1)).unwrap())["rows"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        parse(&engine.snapshot(None).unwrap())["rows"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(group.commit().unwrap_err().contains("closed"));
    assert!(group
        .add(engine.begin().unwrap())
        .unwrap_err()
        .contains("closed"));
    let reopened = StagedEngine::new("{}", &engine.journal().unwrap()).unwrap();
    assert_eq!(reopened.begin().unwrap().count(), 2);
    let mut empty = engine.group();
    assert_eq!(empty.commit().unwrap(), "[]");
    assert_eq!(parse(&group_log_stats(&log))["flushes"], 1);
}

#[wasm_bindgen_test]
fn freeing_or_rejecting_group_handles_releases_all_owned_sessions() {
    let engine = StagedEngine::new(r#"{"concurrency":"exclusive"}"#, "[]").unwrap();
    let mut abandoned = engine.group();
    abandoned.add(engine.begin().unwrap()).unwrap();
    assert!(engine.begin().is_err());
    drop(abandoned);
    assert!(engine.begin().is_ok());
    let other = StagedEngine::new(r#"{"concurrency":"exclusive"}"#, "[]").unwrap();
    let mut mixed = engine.group();
    mixed.add(engine.begin().unwrap()).unwrap();
    mixed.add(other.begin().unwrap()).unwrap();
    assert!(mixed.commit().unwrap_err().contains("same engine"));
    assert!(engine.begin().is_ok());
    assert!(other.begin().is_ok());
    assert!(mixed.add(engine.begin().unwrap()).is_err());
    assert!(engine.begin().is_ok());
    let mut held = engine.group();
    held.add(engine.begin().unwrap()).unwrap();
    drop(engine);
    assert_eq!(
        parse(&held.commit().unwrap())[0],
        json!({"status":"committed","revision":"0"})
    );
}

#[wasm_bindgen_test]
fn javascript_faults_return_uncertain_members_and_recover_each_durable_prefix() {
    for phase in ["append", "flush"] {
        for index in 1..=2 {
            for after in [false, true] {
                for write_through in [false, true] {
                    if phase == "flush" && index != 1 {
                        continue;
                    }
                    let log = group_log(phase, index, after, write_through);
                    let engine = StagedEngine::with_log(
                        r#"{"isolation":"read-committed","constraints":["references"]}"#,
                        log.clone().unchecked_into(),
                    )
                    .unwrap();
                    let observer = engine.begin().unwrap();
                    let mut first = engine.begin().unwrap();
                    first.request("(create: (0 0))");
                    let mut bad = engine.begin().unwrap();
                    bad.request("(create: (999 0))");
                    let mut last = engine.begin().unwrap();
                    last.request("(create: (1 0))");
                    let mut group = engine.group();
                    group.add(first).unwrap();
                    group.add(bad).unwrap();
                    group.add(last).unwrap();
                    group.add(engine.begin().unwrap()).unwrap();
                    let outcomes = parse(&group.commit().unwrap());
                    assert_eq!(outcomes[1]["status"], "aborted");
                    for i in [0, 2, 3] {
                        assert_eq!(outcomes[i]["status"], "indeterminate");
                        assert!(outcomes[i]["error"].as_str().unwrap().contains("reopen"));
                    }
                    assert_eq!(observer.count(), 0);
                    assert!(engine.begin().is_err());
                    assert!(engine.snapshot(None).is_err());
                    assert!(engine.group().commit().is_err());
                    let stats = parse(&group_log_stats(&log));
                    let expected = if phase == "flush" && after {
                        2
                    } else if write_through {
                        if phase == "flush" {
                            2
                        } else {
                            index - 1 + u32::from(after)
                        }
                    } else {
                        0
                    };
                    assert_eq!(stats["stable"].as_array().unwrap().len(), expected as usize);
                    let reopened = StagedEngine::new("{}", &stats["stable"].to_string()).unwrap();
                    assert_eq!(reopened.begin().unwrap().count(), expected as usize);
                    let mut next = reopened.begin().unwrap();
                    next.request("(create: (0 0))");
                    assert_eq!(next.commit().unwrap(), u64::from(expected) + 1);
                }
            }
        }
    }
}
