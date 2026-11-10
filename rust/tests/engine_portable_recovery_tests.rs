//! Public recovery and diagnostic-option contracts on native and actual WASM.
use link_cli::engine::{
    ConcurrencyControl, Engine, EngineOptions, HistoryPolicy, IsolationLevel, MemoryLog,
    SerializableValidation,
};
use link_cli::LinksStorage;

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn public_recovery_consumes_every_shared_u32_journal_vector() {
    let vectors: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/protocol/engine-revision-vectors.json"
    ))
    .unwrap();
    let entries = vectors["valid"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap().to_owned())
        .collect();
    let recovered = Engine::<u32, _>::open(
        MemoryLog { entries },
        EngineOptions {
            history: HistoryPolicy::All,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(recovered.snapshot().unwrap().revision, 3);
    assert_eq!(
        recovered
            .snapshot_at(1)
            .unwrap()
            .get(1)
            .unwrap()
            .name
            .as_deref(),
        Some("雪;|😀")
    );
    assert!(recovered.snapshot_at(2).unwrap().links().is_empty());
    let current = recovered.snapshot().unwrap();
    let last = current.get(u32::MAX).unwrap();
    assert_eq!(last.link.target, u32::MAX);
    assert_eq!(last.name.as_deref(), Some(""));
    assert!(recovered.begin().unwrap().create_link(0, 0).is_err());

    for invalid in vectors["invalidU32"].as_array().unwrap() {
        let text = invalid.as_str().unwrap();
        assert!(
            Engine::<u32, _>::open(
                MemoryLog {
                    entries: vec![text.to_owned()]
                },
                EngineOptions::default()
            )
            .is_err(),
            "accepted invalid shared record {text:?}"
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn opt_in_trace_preserves_admission_visibility_and_recovery() {
    for (concurrency, isolation, serializable_validation) in [
        (
            ConcurrencyControl::Exclusive,
            IsolationLevel::Snapshot,
            SerializableValidation::Revision,
        ),
        (
            ConcurrencyControl::Optimistic,
            IsolationLevel::Snapshot,
            SerializableValidation::Revision,
        ),
        (
            ConcurrencyControl::Optimistic,
            IsolationLevel::Serializable,
            SerializableValidation::Revision,
        ),
        (
            ConcurrencyControl::Optimistic,
            IsolationLevel::Serializable,
            SerializableValidation::ReadSet,
        ),
    ] {
        let db = Engine::<u32, _>::open(
            MemoryLog::default(),
            EngineOptions {
                concurrency,
                isolation,
                serializable_validation,
                trace: true,
                ..Default::default()
            },
        )
        .unwrap();
        let mut observer =
            (concurrency == ConcurrencyControl::Optimistic).then(|| db.begin().unwrap());
        let mut writer = db.begin().unwrap();
        if concurrency == ConcurrencyControl::Exclusive {
            assert!(db.begin().is_err());
        }
        let id = writer.create_link(0, 0).unwrap();
        writer.set_name(id, Some("traced 雪".into())).unwrap();
        assert!(db.snapshot().unwrap().links().is_empty());
        assert_eq!(writer.commit().unwrap(), 1);
        drop(writer);
        assert_eq!(
            db.snapshot().unwrap().get(id).unwrap().name.as_deref(),
            Some("traced 雪")
        );
        if let Some(observer) = observer.as_mut() {
            assert_eq!(observer.get_name(id), None);
            observer.rollback();
        }
        drop(observer);
        let log = db.into_log().ok().unwrap();
        let reopened = Engine::<u32, _>::open(log, EngineOptions::default()).unwrap();
        assert_eq!(reopened.snapshot().unwrap().revision, 1);
        assert_eq!(
            reopened.begin().unwrap().get_name(id).as_deref(),
            Some("traced 雪")
        );
    }
}
