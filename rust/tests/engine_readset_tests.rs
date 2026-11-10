use link_cli::engine::{
    ConcurrencyControl, Engine, EngineOptions, IsolationLevel, MemoryLog, SerializableValidation,
};
use link_cli::LinksStorage;

fn engine() -> Engine<u64, MemoryLog> {
    let db = Engine::open(
        MemoryLog::default(),
        EngineOptions {
            serializable_validation: SerializableValidation::ReadSet,
            ..Default::default()
        },
    )
    .unwrap();
    let mut seed = db.begin().unwrap();
    seed.ensure_link_created(1).unwrap();
    seed.ensure_link_created(2).unwrap();
    seed.commit().unwrap();
    db
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn read_set_configuration_requires_serializable_isolation() {
    for isolation in [IsolationLevel::Snapshot, IsolationLevel::ReadCommitted] {
        assert!(Engine::<u64, _>::open(
            MemoryLog::default(),
            EngineOptions {
                isolation,
                serializable_validation: SerializableValidation::ReadSet,
                ..Default::default()
            }
        )
        .is_err());
    }
    let exclusive = Engine::<u64, _>::open(
        MemoryLog::default(),
        EngineOptions {
            concurrency: ConcurrencyControl::Exclusive,
            serializable_validation: SerializableValidation::ReadSet,
            ..Default::default()
        },
    )
    .unwrap();
    let mut writer = exclusive.begin().unwrap();
    writer.create_link(0, 0).unwrap();
    assert!(exclusive.begin().is_err());
    writer.commit().unwrap();
    assert!(exclusive.begin().is_ok());
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn disjoint_points_commit_but_read_dependencies_prevent_write_skew() {
    let db = engine();
    let mut a = db.begin().unwrap();
    let mut b = db.begin().unwrap();
    assert_eq!(a.get_name(1), None);
    assert_eq!(b.get_name(2), None);
    a.set_name(1, Some("one".into())).unwrap();
    b.set_name(2, Some("two".into())).unwrap();
    assert_eq!(a.commit().unwrap(), 2);
    assert_eq!(b.commit().unwrap(), 3);
    let mut a = db.begin().unwrap();
    let mut b = db.begin().unwrap();
    assert_eq!(a.get_name(2).as_deref(), Some("two"));
    assert_eq!(b.get_name(1).as_deref(), Some("one"));
    a.set_name(1, None).unwrap();
    b.set_name(2, None).unwrap();
    a.commit().unwrap();
    assert!(b.commit().unwrap_err().to_string().contains("read"));
    assert_eq!(
        db.snapshot().unwrap().get(2).unwrap().name.as_deref(),
        Some("two")
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn missing_reads_aba_and_branch_publications_are_validated() {
    let db = engine();
    let mut missing = db.begin().unwrap();
    assert!(missing.get_link(99).is_none());
    missing.set_name(2, Some("must abort".into())).unwrap();
    let mut insert = db.begin().unwrap();
    insert.ensure_link_created(99).unwrap();
    insert.commit().unwrap();
    assert!(missing.commit().is_err());
    let mut aba = db.begin().unwrap();
    assert_eq!(aba.get_name(1), None);
    let mut delete = db.begin().unwrap();
    delete.delete_link(1).unwrap();
    delete.commit().unwrap();
    let mut recreate = db.begin().unwrap();
    recreate.ensure_link_created(1).unwrap();
    recreate.commit().unwrap();
    assert!(aba.commit().is_err());
    let mut branch = db
        .fork(MemoryLog::default(), EngineOptions::default())
        .unwrap();
    let mut stale = db.begin().unwrap();
    assert_eq!(stale.get_name(1), None);
    let mut incoming = branch.engine().begin().unwrap();
    incoming.set_name(1, Some("branch".into())).unwrap();
    incoming.commit().unwrap();
    branch.merge().unwrap();
    assert!(stale.commit().is_err());
    let log = db.with_log(|log| Ok(log.clone())).unwrap();
    let reopened = Engine::<u64, _>::open(
        log,
        EngineOptions {
            serializable_validation: SerializableValidation::ReadSet,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        reopened.snapshot().unwrap().get(1).unwrap().name.as_deref(),
        Some("branch")
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn scans_names_and_read_only_commits_validate_their_dependencies() {
    let db = engine();
    for scan in 0..3 {
        let mut reader = db.begin().unwrap();
        match scan {
            0 => {
                assert_eq!(reader.links_count(), 2);
            }
            1 => {
                assert!(reader.get_by_name("absent").is_none());
            }
            _ => {
                assert_eq!(reader.query_links(Some(1), None, None).len(), 1);
            }
        }
        let mut writer = db.begin().unwrap();
        writer.set_name(2, Some(format!("write-{scan}"))).unwrap();
        writer.commit().unwrap();
        assert!(reader.commit().is_err(), "scan {scan}");
    }
    let mut reader = db.begin().unwrap();
    assert_eq!(reader.get_name(1), None);
    let mut writer = db.begin().unwrap();
    writer.set_name(2, None).unwrap();
    writer.commit().unwrap();
    assert_eq!(reader.commit().unwrap(), db.snapshot().unwrap().revision);
    let mut empty = db.begin().unwrap();
    let mut writer = db.begin().unwrap();
    writer.set_name(1, Some("after".into())).unwrap();
    writer.commit().unwrap();
    assert!(empty.commit().is_ok());
    let mut reader = db.begin().unwrap();
    assert_eq!(reader.get_name(1).as_deref(), Some("after"));
    let mut writer = db.begin().unwrap();
    writer.set_name(1, None).unwrap();
    writer.commit().unwrap();
    assert!(reader.commit().is_err());
}

// Independent oracle: enumerate serial executions of acknowledged transactions,
// compare their observed point values and the final state. 216 profiles x 6
// commit orders, with at most three transactions and two stored rows per probe.
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn finite_concurrent_histories_have_a_serial_execution() {
    const ORDERS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    for profile in 0..216 {
        let choices = [profile % 6, profile / 6 % 6, profile / 36];
        let reads = choices.map(|c| c / 2 + 1);
        let writes = choices.map(|c| c % 2 + 1);
        for order in ORDERS {
            let db = engine();
            let mut sessions: Vec<_> = (0..3).map(|_| db.begin().unwrap()).collect();
            for i in 0..3 {
                assert_eq!(sessions[i].get_name(reads[i]), None);
                assert_eq!(sessions[i].get_name(writes[i]), None);
                sessions[i]
                    .set_name(writes[i], Some(i.to_string()))
                    .unwrap();
            }
            let mut accepted = [false; 3];
            for i in order {
                accepted[i] = sessions[i].commit().is_ok();
            }
            let actual = db.snapshot().unwrap();
            assert!(
                ORDERS.into_iter().any(|serial| {
                    let mut names: [Option<String>; 2] = [None, None];
                    for i in serial {
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
                "nonserial history profile={profile} order={order:?} accepted={accepted:?}"
            );
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn own_writes_and_blind_allocations_still_validate_written_addresses() {
    let db = engine();
    let mut own = db.begin().unwrap();
    let id = own.create_link(1, 2).unwrap();
    assert_eq!(own.get_link(id).unwrap().source, 1);
    own.set_name(id, Some("own".into())).unwrap();
    assert_eq!(own.get_name(id).as_deref(), Some("own"));
    assert_eq!(own.all_links().len(), 3);
    own.delete_link(id).unwrap();
    assert_eq!(own.all_links().len(), 2);
    own.commit().unwrap();
    let mut allocated = db.begin().unwrap();
    let reserved = allocated.create_link(0, 0).unwrap();
    // Explicit assignment can collide with another session's uncommitted ID.
    let mut assigned = db.begin().unwrap();
    assigned.ensure_link_created(reserved).unwrap();
    assigned.set_name(reserved, Some("winner".into())).unwrap();
    assigned.commit().unwrap();
    assert!(allocated
        .commit()
        .unwrap_err()
        .to_string()
        .contains("written"));
    assert_eq!(
        db.snapshot()
            .unwrap()
            .get(reserved)
            .unwrap()
            .name
            .as_deref(),
        Some("winner")
    );
    let mut retry = db.begin().unwrap();
    retry.set_name(reserved, Some("fresh".into())).unwrap();
    retry.commit().unwrap();
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
#[cfg(not(target_arch = "wasm32"))]
fn shared_rust_point_reads_preserve_send_sync_and_both_dependencies() {
    let db = engine();
    let mut reader = db.begin().unwrap();
    std::thread::scope(|scope| {
        let one = scope.spawn(|| reader.get_name(1));
        let two = scope.spawn(|| reader.get_name(2));
        assert_eq!(one.join().unwrap(), None);
        assert_eq!(two.join().unwrap(), None);
    });
    let mut writer = db.begin().unwrap();
    writer.set_name(2, Some("changed".into())).unwrap();
    writer.commit().unwrap();
    reader.set_name(1, Some("abort".into())).unwrap();
    assert!(reader.commit().unwrap_err().to_string().contains("read"));
}
