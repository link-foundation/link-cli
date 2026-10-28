//! Tests precede the optional mirrored device; runs unchanged in actual WASM.
#[path = "support/mirrored_pages.rs"]
mod support;
use link_cli::engine::paged::{PageDevice, PagedLog};
use link_cli::engine::replication::{ReplicatedLog, Synchronization};
use link_cli::engine::{
    Constraint, ConstraintValidation, DeltaLog, Engine, EngineOptions, GroupCommitOutcome,
    HistoryPolicy, IsolationLevel, RevisionEncoding, StateStructure,
};
use link_cli::LinksStorage;
use link_cli::{LinkError, TransitionLogStore};
use support::*;
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_vectors_and_every_single_copy_damage_repair() -> Result<(), LinkError> {
    let generated = Pair::new(Default::default());
    let mut log = generated.open()?;
    for device in &generated.devices {
        assert_eq!(device.durable(), fixture("empty").0);
    }
    for record in fixture("oneGroup").1 {
        log.append_log_entry(&record)?;
    }
    log.flush_log()?;
    for device in &generated.devices {
        assert_eq!(device.durable(), fixture("oneGroup").0);
    }
    log.append_log_entry("")?;
    log.flush_log()?;
    for device in &generated.devices {
        assert_eq!(device.durable(), fixture("twoGroups").0);
    }
    for name in ["empty", "oneGroup", "twoGroups"] {
        let (image, expected) = fixture(name);
        let pair = Pair::new([image.clone(), image.clone()]);
        assert_eq!(pair.open()?.read_log_entries()?, expected);
        assert!(pair.trace.borrow().calls.is_empty());
        assert!(PagedLog::open(pair.devices[0].clone()).is_err());
        following(&pair, &expected)?;
    }
    let (healthy, expected) = fixture("twoGroups");
    for fault in wire()["singleCopyFaults"].as_array().unwrap() {
        let mut images = [healthy.clone(), healthy.clone()];
        let side = fault["side"].as_u64().unwrap() as usize;
        let image = &mut images[side];
        let n = |name: &str| fault[name].as_u64().unwrap() as usize;
        match fault["kind"].as_str().unwrap() {
            "page" => image.pages[n("page")][n("offset")] ^= 1,
            "metadata" => image.metadata[n("slot")]
                .as_mut()
                .unwrap()
                .truncate(n("length")),
            "truncated" => image.pages.truncate(n("pages")),
            "stale" => {
                let page = &mut image.pages[n("page")];
                page[64] ^= 1;
                sign_page(page);
            }
            _ => panic!("unknown instruction"),
        }
        let pair = Pair::new(images);
        assert_eq!(pair.open()?.read_log_entries()?, expected, "{fault}");
        for d in &pair.devices {
            assert_eq!(d.durable(), healthy);
        }
        pair.clear();
        pair.open()?;
        assert!(pair.trace.borrow().calls.is_empty());
        following(&pair, &expected)?;
    }
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_flush_position_before_after_is_old_or_complete_then_following() -> Result<(), LinkError> {
    let (image, prior) = fixture("oneGroup");
    let probe = Pair::new([image.clone(), image.clone()]);
    let mut log = probe.open()?;
    probe.clear();
    let next = "雪".repeat(700);
    log.append_log_entry(&next)?;
    log.flush_log()?;
    let operations = probe.trace.borrow().calls.len();
    for at in 1..=operations {
        for after in [false, true] {
            let pair = Pair::new([image.clone(), image.clone()]);
            let mut log = pair.open()?;
            pair.clear();
            pair.trace.borrow_mut().fault = Some((at, after));
            log.append_log_entry(&next)?;
            assert!(log.flush_log().is_err());
            assert!(log.append_log_entry("retry").is_err());
            assert!(log.read_log_entries().is_err());
            assert!(log.flush_log().is_err());
            let recovered = pair.crash();
            let actual = recovered.open()?.read_log_entries()?;
            let full = [prior.clone(), vec![next.clone()]].concat();
            assert!(
                actual == prior || actual == full,
                "position {at}, after {after}"
            );
            following(&recovered, &actual)?;
        }
    }
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_repair_position_before_after_keeps_the_surviving_source() -> Result<(), LinkError> {
    let (healthy, expected) = fixture("twoGroups");
    for side in 0..2 {
        let mut damaged = healthy.clone();
        damaged.pages.clear();
        damaged.metadata[0] = Some(vec![0; 17]);
        let images = if side == 0 {
            [damaged, healthy.clone()]
        } else {
            [healthy.clone(), damaged]
        };
        let probe = Pair::new(images.clone());
        probe.open()?;
        let operations = probe.trace.borrow().calls.len();
        for at in 1..=operations {
            for after in [false, true] {
                let pair = Pair::new(images.clone());
                pair.trace.borrow_mut().fault = Some((at, after));
                assert!(pair.open().is_err());
                assert_eq!(pair.devices[1 - side].durable(), healthy);
                let recovered = pair.crash();
                assert_eq!(recovered.open()?.read_log_entries()?, expected);
                following(&recovered, &expected)?;
            }
        }
    }
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn corruption_rejects_before_any_repair_or_tail_truncation() -> Result<(), LinkError> {
    let (healthy, _) = fixture("twoGroups");
    let mut damaged = healthy.clone();
    damaged.pages[0][56] ^= 1;
    let pair = Pair::new([damaged.clone(), damaged]);
    assert!(pair.open().is_err());
    assert!(pair.trace.borrow().calls.is_empty());
    let mut invalid = fixture("oneGroup").0;
    invalid.pages[0][80] = 0xff;
    let h = sign_page(&mut invalid.pages[0]);
    let meta = invalid.metadata[1].as_mut().unwrap();
    meta[72..80].copy_from_slice(&h.to_le_bytes());
    sign_meta(meta);
    let mut bad = invalid.clone();
    bad.pages[0][56] ^= 1;
    bad.pages.push([0; 4096]);
    let pair = Pair::new([invalid, bad]);
    assert!(pair.open().is_err());
    assert!(pair.trace.borrow().calls.is_empty());
    for identity in [false, true] {
        let mut bad = healthy.clone();
        let meta = bad.metadata[0].as_mut().unwrap();
        meta[if identity { 16 } else { 72 }] ^= 1;
        sign_meta(meta);
        let pair = Pair::new([healthy.clone(), bad]);
        assert!(pair.open().is_err());
        assert!(pair.trace.borrow().calls.is_empty());
    }
    Ok(())
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn selected_flush_costs_stay_constant_and_defaults_are_unchanged() -> Result<(), LinkError> {
    for history in [0, 1, 32] {
        let pair = Pair::new(Default::default());
        let mut log = pair.open()?;
        for _ in 0..history {
            log.append_log_entry("prior")?;
            log.flush_log()?;
        }
        let before = pair.devices[0].image().pages;
        pair.clear();
        log.append_log_entry("next")?;
        log.flush_log()?;
        assert!(pair.trace.borrow().reads.is_empty());
        let calls = pair.trace.borrow().calls.clone();
        for (name, count) in [
            ("writePage", 2),
            ("syncData", 2),
            ("writeMetadata", 2),
            ("syncMetadata", 2),
        ] {
            assert_eq!(calls.iter().filter(|c| c.contains(name)).count(), count);
        }
        assert_eq!(&pair.devices[0].image().pages[..history], before);
        assert!(calls[..4]
            .iter()
            .all(|c| c.contains("writePage") || c.contains("syncData")));
        let mut device = log.into_device();
        assert!(device.read_page(0).is_err());
        assert_eq!(device.metadata_flags(), 1);
        assert!(device.supports_durable_flush());
        pair.devices[0].faults.borrow_mut().volatile = true;
        assert!(!device.supports_durable_flush());
        let _ = device.into_devices();
    }
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn unreadable_copies_stale_decisions_and_changed_sources_are_checked() -> Result<(), LinkError> {
    let (healthy, records) = fixture("twoGroups");
    for side in 0..2 {
        let pair = Pair::new([healthy.clone(), healthy.clone()]);
        pair.devices[side].faults.borrow_mut().page = true;
        assert_eq!(pair.open()?.read_log_entries()?, records);
        pair.devices[side].faults.borrow_mut().page = false;
        following(&pair, &records)?;
        let pair = Pair::new([healthy.clone(), healthy.clone()]);
        pair.devices[side].faults.borrow_mut().metadata = true;
        assert_eq!(pair.open()?.read_log_entries()?, records);
        pair.devices[side].faults.borrow_mut().metadata = false;
        following(&pair, &records)?;
    }
    let pair = Pair::new([healthy.clone(), healthy.clone()]);
    for device in &pair.devices {
        device.faults.borrow_mut().page = true;
    }
    assert!(pair.open().is_err());
    assert!(pair.trace.borrow().calls.is_empty());
    let pair = Pair::new([healthy.clone(), healthy.clone()]);
    pair.devices[0].faults.borrow_mut().length = Some(u64::MAX);
    assert!(pair.open().is_err());
    assert!(pair.trace.borrow().calls.is_empty());
    let mut older = healthy.clone();
    older.metadata[0] = fixture("empty").0.metadata[0].clone();
    let pair = Pair::new([older, healthy.clone()]);
    assert_eq!(pair.open()?.read_log_entries()?, records);
    assert_eq!(pair.devices[0].durable(), healthy);
    let mut unsupported = healthy.clone();
    unsupported.metadata[0].as_mut().unwrap()[12] = 2;
    sign_meta(unsupported.metadata[0].as_mut().unwrap());
    let pair = Pair::new([healthy.clone(), unsupported]);
    assert!(pair.open().is_err());
    assert!(pair.trace.borrow().calls.is_empty());
    let mut bad = healthy.clone();
    bad.pages.clear();
    let pair = Pair::new([healthy, bad]);
    pair.devices[0].faults.borrow_mut().change_source = true;
    assert!(pair.open().is_err());
    assert!(pair.trace.borrow().calls.is_empty());
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn every_tail_cleanup_failure_fences_until_both_devices_reopen() -> Result<(), LinkError> {
    let (healthy, records) = fixture("twoGroups");
    let mut extra = healthy.clone();
    extra.pages.push([0; 4096]);
    for at in 1..=4 {
        for after in [false, true] {
            let pair = Pair::new([extra.clone(), extra.clone()]);
            pair.trace.borrow_mut().fault = Some((at, after));
            let mut device = link_cli::engine::paged::MirroredPageDevice::new(
                pair.devices[0].clone(),
                pair.devices[1].clone(),
            );
            // Exercise the opener's truncate/sync sequence while retaining the
            // failed owner so its fence can be inspected directly.
            assert!(device
                .truncate_data(healthy.pages.len() as u64 * 4096)
                .and_then(|()| device.sync_data())
                .is_err());
            assert!(device.sync_data().is_err());
            assert!(device.read_metadata(0).is_err());
            assert!(device.read_checked_page(0, 0).is_err());
            assert!(device.complete_recovery().is_err());
            assert!(device.new_database_id().is_err());
            assert!(device.data_length().is_err());
            following(&pair.crash(), &records)?;
        }
    }
    let pair = Pair::new([healthy.clone(), healthy]);
    let mut device = link_cli::engine::paged::MirroredPageDevice::new(
        pair.devices[0].clone(),
        pair.devices[1].clone(),
    );
    assert!(device.read_metadata(2).is_err());
    Ok(())
}

fn composed_round<L: TransitionLogStore>(
    log: L,
    options: EngineOptions,
    primary: &Pair,
    replica: &Pair,
    round: u64,
) -> Result<(), LinkError> {
    let db = Engine::<u64, _>::open(log, options)?;
    let before = db.snapshot()?;
    assert_eq!(before.revision, round * 3);
    assert_eq!(before.rows().len() as u64, round * 3);
    for row in before.rows().values() {
        assert!(matches!(
            row.name.as_deref(),
            Some("\u{feff}雪\0😀" | "second" | "following")
        ));
    }
    primary.clear();
    replica.clear();
    let mut a = db.begin()?;
    let first = a.create_link(0, 0)?;
    a.set_name(first, Some("\u{feff}雪\0😀".into()))?;
    let mut b = db.begin()?;
    let second = b.create_link(first, 0)?;
    b.set_name(second, Some("second".into()))?;
    assert_eq!(
        db.commit_group(vec![a, b])?,
        [
            GroupCommitOutcome::Committed(before.revision + 1),
            GroupCommitOutcome::Committed(before.revision + 2),
        ]
    );
    let syncs = |device: &Pair| {
        device
            .trace
            .borrow()
            .calls
            .iter()
            .filter(|call| call.ends_with(":syncData"))
            .count()
    };
    assert_eq!(syncs(primary), 2);
    assert_eq!(syncs(replica), 2);
    assert_eq!(db.snapshot_at(before.revision)?.rows(), before.rows());
    let mut invalid = db.begin()?;
    invalid.create_link(999999, 0)?;
    assert!(invalid.commit().is_err());
    assert_eq!(syncs(primary), 2);
    let mut discarded = db.begin()?;
    discarded.create_link(0, 0)?;
    discarded.rollback();
    let mut following = db.begin()?;
    let third = following.create_link(second, 0)?;
    following.set_name(third, Some("following".into()))?;
    assert_eq!(following.commit()?, before.revision + 3);
    assert_eq!(syncs(primary), 4);
    assert_eq!(syncs(replica), 4);
    assert_eq!(db.snapshot()?.rows().len(), before.rows().len() + 3);
    assert_eq!(
        primary.crash().open()?.read_log_entries()?,
        replica.crash().open()?.read_log_entries()?
    );
    Ok(())
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn independent_engine_policies_compose_with_replicas_groups_and_reopen() -> Result<(), LinkError> {
    for state_structure in [StateStructure::Flat, StateStructure::Shared] {
        for revision_encoding in [RevisionEncoding::Snapshot, RevisionEncoding::Changes] {
            for constraint_validation in [ConstraintValidation::Scan, ConstraintValidation::Indexed]
            {
                for delta in [false, true] {
                    let primary = Pair::new(Default::default());
                    let replica = Pair::new(Default::default());
                    for round in 0..2 {
                        let log = ReplicatedLog::new(
                            primary.open()?,
                            vec![replica.open()?],
                            Synchronization::Synchronous {
                                acknowledgements: 2,
                            },
                        )?;
                        let options = EngineOptions {
                            state_structure,
                            revision_encoding,
                            constraint_validation,
                            isolation: IsolationLevel::Snapshot,
                            history: HistoryPolicy::All,
                            constraints: vec![Constraint::References],
                            require_durable_flush: true,
                            ..Default::default()
                        };
                        if delta {
                            composed_round(
                                DeltaLog::new(log, 128)?,
                                options,
                                &primary,
                                &replica,
                                round,
                            )?;
                        } else {
                            composed_round(log, options, &primary, &replica, round)?;
                        }
                    }
                }
            }
        }
    }
    Ok(())
}
