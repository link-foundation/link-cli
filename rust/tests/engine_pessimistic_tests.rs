//! Engine-local pessimistic point-write ownership; also runs in actual WASM.
use link_cli::engine::{
    ConcurrencyControl, Constraint, Engine, EngineOptions, IsolationLevel, MemoryLog,
    SerializableValidation,
};
use link_cli::LinksStorage;

fn options() -> EngineOptions {
    EngineOptions {
        concurrency: ConcurrencyControl::Pessimistic,
        isolation: IsolationLevel::Snapshot,
        ..Default::default()
    }
}
fn seeded(options: EngineOptions) -> Engine<u32, MemoryLog> {
    let db = Engine::open(MemoryLog::default(), options).unwrap();
    let mut seed = db.begin().unwrap();
    seed.ensure_link_created(1).unwrap();
    seed.ensure_link_created(2).unwrap();
    seed.commit().unwrap();
    db
}
fn busy<T: std::fmt::Debug>(result: Result<T, link_cli::LinkError>) {
    assert!(result.unwrap_err().to_string().contains("busy"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn disjoint_writes_commit_and_busy_operations_preserve_prior_staged_writes() {
    let db = seeded(options());
    let retained = db.snapshot().unwrap();
    let mut first = db.begin().unwrap();
    let mut second = db.begin().unwrap();
    first.update_link(1, 1, 0).unwrap();
    first.set_name(1, Some("first".into())).unwrap();
    second.set_name(2, Some("second".into())).unwrap();
    busy(second.update_link(1, 0, 1));
    busy(second.delete_link(1));
    busy(second.set_name(1, None));
    assert_eq!(second.get_link(1).unwrap().source, 0);
    assert_eq!(second.get_name(2).as_deref(), Some("second"));
    // Ensuring an already visible row is a no-op and takes no write ownership.
    assert_eq!(second.ensure_link_created(1).unwrap(), 1);
    assert_eq!(db.snapshot().unwrap().get(1).unwrap().link.source, 0);
    assert_eq!(first.commit().unwrap(), 2);
    assert_eq!(second.commit().unwrap(), 3);
    assert_eq!(retained.get(1).unwrap().link.source, 0);
    assert_eq!(
        db.snapshot().unwrap().get(1).unwrap().name.as_deref(),
        Some("first")
    );
    assert_eq!(
        db.snapshot().unwrap().get(2).unwrap().name.as_deref(),
        Some("second")
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn allocated_and_explicit_missing_addresses_share_ownership_until_rollback() {
    let db = seeded(options());
    let mut first = db.begin().unwrap();
    let mut second = db.begin().unwrap();
    let reserved = first.create_link(1, 2).unwrap();
    let other = second.create_link(2, 1).unwrap();
    assert_ne!(reserved, other);
    busy(second.ensure_link_created(reserved));
    first.ensure_link_created(90).unwrap();
    busy(second.ensure_link_created(90));
    first.rollback();
    second.ensure_link_created(reserved).unwrap();
    second.ensure_link_created(90).unwrap();
    second.commit().unwrap();
    let mut fresh = db.begin().unwrap();
    assert!(fresh.create_link(0, 0).unwrap() > 90);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn reverse_key_order_fails_immediately_without_waiting_or_discarding_sessions() {
    let db = seeded(options());
    let mut first = db.begin().unwrap();
    let mut second = db.begin().unwrap();
    first.set_name(1, Some("one".into())).unwrap();
    second.set_name(2, Some("two".into())).unwrap();
    busy(first.set_name(2, None));
    busy(second.set_name(1, None));
    first.commit().unwrap();
    second.commit().unwrap();
    for finish in 0..2 {
        let mut owner = db.begin().unwrap();
        let mut retry = db.begin().unwrap();
        owner.set_name(1, Some("private".into())).unwrap();
        busy(retry.set_name(1, None));
        if finish == 0 {
            owner.rollback();
        } else {
            drop(owner);
        }
        retry.set_name(1, None).unwrap();
        retry.commit().unwrap();
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn stale_writers_are_rejected_before_staging_even_when_aba_restores_the_row() {
    for isolation in [
        IsolationLevel::ReadCommitted,
        IsolationLevel::Snapshot,
        IsolationLevel::Serializable,
    ] {
        let db = seeded(EngineOptions {
            isolation,
            ..options()
        });
        let mut stale = db.begin().unwrap();
        let original = stale.get_link(1).unwrap();
        let mut deleted = db.begin().unwrap();
        deleted.delete_link(1).unwrap();
        deleted.commit().unwrap();
        let mut recreated = db.begin().unwrap();
        recreated.ensure_link_created(1).unwrap();
        recreated.commit().unwrap();
        assert_eq!(db.snapshot().unwrap().get(1).unwrap().link, original);
        assert!(stale
            .update_link(1, 1, 1)
            .unwrap_err()
            .to_string()
            .contains("conflict"));
        assert_eq!(stale.get_link(1).unwrap(), original);
        // A failed acquisition does not leave ownership behind.
        let mut fresh = db.begin().unwrap();
        fresh.set_name(1, Some("fresh".into())).unwrap();
        fresh.commit().unwrap();
        stale.rollback();
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn serializable_validation_remains_independent_of_write_ownership() {
    for validation in [
        SerializableValidation::Revision,
        SerializableValidation::ReadSet,
    ] {
        let db = seeded(EngineOptions {
            isolation: IsolationLevel::Serializable,
            serializable_validation: validation,
            ..options()
        });
        let mut first = db.begin().unwrap();
        let mut second = db.begin().unwrap();
        first.set_name(1, Some("one".into())).unwrap();
        second.set_name(2, Some("two".into())).unwrap();
        first.commit().unwrap();
        assert_eq!(
            second.commit().is_ok(),
            validation == SerializableValidation::ReadSet
        );
        let mut fresh = db.begin().unwrap();
        fresh.set_name(2, None).unwrap();
        fresh.commit().unwrap();
    }
    let db = seeded(EngineOptions {
        isolation: IsolationLevel::Serializable,
        serializable_validation: SerializableValidation::ReadSet,
        ..options()
    });
    let mut first = db.begin().unwrap();
    let mut second = db.begin().unwrap();
    first.get_name(2);
    second.get_name(1);
    first.set_name(1, Some("one".into())).unwrap();
    second.set_name(2, Some("two".into())).unwrap();
    first.commit().unwrap();
    assert!(second.commit().unwrap_err().to_string().contains("read"));
    let mut scan = db.begin().unwrap();
    scan.all_links();
    let mut writer = db.begin().unwrap();
    writer.set_name(2, Some("after".into())).unwrap();
    writer.commit().unwrap();
    assert!(scan
        .commit()
        .unwrap_err()
        .to_string()
        .contains("serializable"));
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn branch_merges_respect_owned_addresses_but_allow_disjoint_publication() {
    let db = seeded(options());
    let mut branch = db
        .fork(MemoryLog::default(), EngineOptions::default())
        .unwrap();
    let mut owner = db.begin().unwrap();
    owner.set_name(1, Some("private".into())).unwrap();
    let mut incoming = branch.engine().begin().unwrap();
    incoming.set_name(2, Some("other".into())).unwrap();
    incoming.commit().unwrap();
    branch.merge().unwrap();
    let mut incoming = branch.engine().begin().unwrap();
    incoming.set_name(1, Some("branch".into())).unwrap();
    incoming.commit().unwrap();
    busy(branch.merge());
    assert_eq!(db.snapshot().unwrap().get(1).unwrap().name, None);
    owner.rollback();
    branch.merge().unwrap();
    assert_eq!(
        db.snapshot().unwrap().get(1).unwrap().name.as_deref(),
        Some("branch")
    );
    assert_eq!(
        db.snapshot().unwrap().get(2).unwrap().name.as_deref(),
        Some("other")
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn constraint_failure_releases_ownership_before_a_fresh_retry() {
    let db = seeded(EngineOptions {
        constraints: vec![Constraint::UniqueNames],
        ..options()
    });
    let mut first = db.begin().unwrap();
    let mut rejected = db.begin().unwrap();
    let mut retry = db.begin().unwrap();
    first.set_name(1, Some("same".into())).unwrap();
    rejected.set_name(2, Some("same".into())).unwrap();
    busy(retry.set_name(2, Some("retry".into())));
    first.commit().unwrap();
    assert!(rejected
        .commit()
        .unwrap_err()
        .to_string()
        .contains("constraint"));
    assert!(rejected.set_name(2, None).is_err());
    retry.set_name(2, Some("retry".into())).unwrap();
    retry.commit().unwrap();
    let log = db.with_log(|log| Ok(log.clone())).unwrap();
    let recovered = Engine::<u32, _>::open(log, options()).unwrap();
    assert_eq!(
        recovered
            .snapshot()
            .unwrap()
            .get(2)
            .unwrap()
            .name
            .as_deref(),
        Some("retry")
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_pessimistic_histories_have_an_independent_serial_execution() {
    let orders = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for profile in 0..216 {
        let choices = [profile % 6, profile / 6 % 6, profile / 36];
        let reads = choices.map(|choice| choice / 2 + 1);
        let writes = choices.map(|choice| choice % 2 + 1);
        for order in orders {
            let db = seeded(EngineOptions {
                isolation: IsolationLevel::Serializable,
                serializable_validation: SerializableValidation::ReadSet,
                ..options()
            });
            let mut sessions = [
                db.begin().unwrap(),
                db.begin().unwrap(),
                db.begin().unwrap(),
            ];
            let mut admitted = [true; 3];
            for i in 0..3 {
                assert_eq!(sessions[i].get_name(reads[i]), None);
                assert_eq!(sessions[i].get_name(writes[i]), None);
                if let Err(error) = sessions[i].set_name(writes[i], Some(i.to_string())) {
                    assert!(error.to_string().contains("busy"));
                    sessions[i].rollback();
                    admitted[i] = false;
                }
            }
            let mut accepted = [false; 3];
            for i in order {
                if admitted[i] {
                    match sessions[i].commit() {
                        Ok(_) => accepted[i] = true,
                        Err(error) => assert!(error.to_string().contains("conflict")),
                    }
                }
            }
            let actual = db.snapshot().unwrap();
            assert!(
                orders.iter().any(|serial| {
                    let mut names: [Option<String>; 2] = [None, None];
                    for &i in serial {
                        if !accepted[i] {
                            continue;
                        }
                        if (reads[i] <= 2 && names[reads[i] as usize - 1].is_some())
                            || names[writes[i] as usize - 1].is_some()
                        {
                            return false;
                        }
                        names[writes[i] as usize - 1] = Some(i.to_string());
                    }
                    names[0] == actual.get(1).unwrap().name
                        && names[1] == actual.get(2).unwrap().name
                }),
                "nonserial profile={profile} order={order:?}"
            );
        }
    }
}

struct FaultLog {
    inner: MemoryLog,
    phase: u8,
}
impl link_cli::TransitionLogStore for FaultLog {
    fn read_log_entries(&mut self) -> Result<Vec<String>, link_cli::LinkError> {
        Ok(self.inner.entries.clone())
    }
    fn append_log_entry(&mut self, record: &str) -> Result<(), link_cli::LinkError> {
        if self.phase == 1 {
            return Err(link_cli::LinkError::Transaction("before append".into()));
        }
        self.inner.entries.push(record.into());
        if self.phase == 2 {
            return Err(link_cli::LinkError::Transaction("after append".into()));
        }
        Ok(())
    }
    fn flush_log(&mut self) -> Result<(), link_cli::LinkError> {
        if self.phase == 3 {
            Err(link_cli::LinkError::Transaction("flush".into()))
        } else {
            Ok(())
        }
    }
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn journal_failures_close_sessions_and_reopening_discovers_the_recorded_outcome() {
    for phase in 1..=3 {
        let db = Engine::<u32, _>::open(
            FaultLog {
                inner: MemoryLog::default(),
                phase: 0,
            },
            options(),
        )
        .unwrap();
        let mut seed = db.begin().unwrap();
        seed.ensure_link_created(1).unwrap();
        seed.ensure_link_created(2).unwrap();
        seed.commit().unwrap();
        drop(seed);
        let mut writer = db.begin().unwrap();
        let mut pending = db.begin().unwrap();
        writer.set_name(1, Some("uncertain".into())).unwrap();
        pending.set_name(2, Some("private".into())).unwrap();
        db.with_log(|log| {
            log.phase = phase;
            Ok(())
        })
        .unwrap();
        assert!(writer
            .commit()
            .unwrap_err()
            .to_string()
            .contains("indeterminate"));
        assert!(writer.commit().is_err());
        assert!(writer.set_name(1, None).is_err());
        assert!(pending.commit().is_err());
        drop(writer);
        drop(pending);
        let mut log = db.into_log().ok().unwrap();
        log.phase = 0;
        let recovered = Engine::<u32, _>::open(log, options()).unwrap();
        assert_eq!(
            recovered
                .snapshot()
                .unwrap()
                .get(1)
                .unwrap()
                .name
                .as_deref(),
            (phase != 1).then_some("uncertain")
        );
        assert_eq!(recovered.snapshot().unwrap().get(2).unwrap().name, None);
        let mut retry = recovered.begin().unwrap();
        retry.set_name(1, Some("retry".into())).unwrap();
        retry.commit().unwrap();
    }
}
