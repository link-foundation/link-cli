//! Deterministic simulation of an unreliable disk and an unreliable network.
//!
//! FoundationDB and TigerBeetle test their storage engines by running the real
//! code against simulated disks and networks, driven by one seeded random
//! source, so that every fault schedule replays exactly. This test does the
//! same for the two places where this crate depends on hardware it does not
//! control.
//!
//! **Disk.** `SimDevice` implements `PageDevice` over three simulated files
//! (pages and two metadata slots). Each file keeps its durable bytes and the
//! writes issued since its last sync. A simulated power loss keeps, for every
//! 512-byte sector independently, the durable bytes or any prefix of the
//! pending writes to that sector, sometimes half of the next one (a torn
//! sector), and a file length from any prefix of the pending operations. That
//! is every reordering and loss a volatile disk cache may perform within a
//! flush interval. Writes and syncs also fail at random (the failed call may or
//! may not have reached the cache). The real `PagedLog` and `Engine` run several
//! clients on top; after every crash the test reopens the database and requires
//! the exact committed state of one revision between the last acknowledged and
//! the last attempted commit.
//!
//! A *lying* disk acknowledges syncs without persisting anything. Over such a
//! disk no engine can keep its promise, so the test requires the next best
//! thing: recovery either refuses the damaged database or returns the exact
//! state of an earlier revision, never a mixture. A mirror that includes a
//! lying disk must recover every acknowledged commit; with an honest primary
//! it must also always open, because power loss alone never tears both
//! copies of a metadata decision.
//!
//! **Network.** Several `ClusterEngine` members reach one atomic journal store
//! through simulated links that drop requests, lose replies, deliver requests
//! late (after other transactions ran), and are cut or made one-way by
//! partitions. Clients on every member run random transactions over two shards.
//! Every client observation goes into one history, which the Elle-style
//! checker shared with `engine_history_tests.rs` searches for anomalies,
//! including real-time edges between members: the cluster must be strictly
//! serializable. Commits with unknown outcomes are resolved from the store's
//! own log, as a client would read fresh journals after reconnecting.
use link_cli::engine::{
    cluster::{AtomicJournalStore, ClusterEngine, JournalVersion},
    paged::{MirroredPageDevice, PagedLog},
    ConcurrencyControl, EngineOptions, HistoryPolicy, IsolationLevel, RevisionEncoding,
    SerializableValidation, StateStructure,
};
use link_cli::{LinkError, LinksStorage};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex},
};

#[path = "engine_history/check.rs"]
mod check;
#[path = "engine_simulation/disk.rs"]
mod disk;
#[path = "engine_simulation/network.rs"]
mod network;
use check::{check, History, Transaction};
use disk::*;

/// Different journal encodings, state structures and concurrency controls
/// write different pages; each must survive the same faults.
fn disk_configurations() -> Vec<EngineOptions> {
    vec![
        EngineOptions::default(),
        EngineOptions {
            concurrency: ConcurrencyControl::Pessimistic,
            isolation: IsolationLevel::Snapshot,
            state_structure: StateStructure::Shared,
            revision_encoding: RevisionEncoding::Changes,
            ..Default::default()
        },
        EngineOptions {
            serializable_validation: SerializableValidation::ReadSet,
            state_structure: StateStructure::SortedRuns,
            revision_encoding: RevisionEncoding::Changes,
            history: HistoryPolicy::Last(4),
            ..Default::default()
        },
        EngineOptions {
            concurrency: ConcurrencyControl::Exclusive,
            require_durable_flush: true,
            ..Default::default()
        },
    ]
}

const DISK_SEEDS: u64 = 24;
const CYCLES: usize = 8;

fn single(
    seed: u64,
    options: &EngineOptions,
    lies: u64,
    errors: u64,
) -> Result<CrashStats, String> {
    let shared = disk(seed.wrapping_mul(31), lies, errors);
    crash_run(
        seed,
        options,
        CYCLES,
        || PagedLog::open(SimDevice(shared.clone())),
        std::slice::from_ref(&shared),
    )
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn power_loss_over_reordering_disk_recovers_exactly_an_acknowledged_or_attempted_commit() {
    for options in disk_configurations() {
        let runs: Vec<CrashStats> = (1..=DISK_SEEDS)
            .map(|seed| single(seed, &options, 0, 15).unwrap())
            .collect();
        let sum = total(runs.into_iter());
        // An honest disk never loses an acknowledged commit or refuses to open.
        assert_eq!((sum.refused, sum.lost), (0, 0), "{options:?}: {sum:?}");
        // The faults happened: unsynced sectors were lost and torn, and failed
        // flushes recovered both ways.
        assert!(sum.commits > 250, "{options:?}: {sum:?}");
        assert!(sum.dropped > 20 && sum.torn > 4, "{options:?}: {sum:?}");
        assert!(
            sum.unknown_committed > 0 && sum.unknown_lost > 0,
            "{options:?}: {sum:?}"
        );
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn lying_disk_is_detected_or_rolls_back_to_an_exact_earlier_revision_never_a_mixture() {
    let options = EngineOptions::default();
    let runs: Vec<CrashStats> = (1..=DISK_SEEDS)
        .map(|seed| single(seed, &options, 500, 0).unwrap())
        .collect();
    let sum = total(runs.into_iter());
    // Acknowledged commits are lost, and some recoveries refuse the damage;
    // `crash_run` already checked that every opened state is exact.
    assert!(sum.lied > 100, "{sum:?}");
    assert!(sum.lost > 0 && sum.refused > 0, "{sum:?}");
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn mirror_of_honest_and_lying_disks_keeps_every_acknowledged_commit() {
    // (primary lies, secondary lies)
    for lies in [(false, false), (false, true), (true, false)] {
        let runs: Vec<CrashStats> = (1..=DISK_SEEDS)
            .map(|seed| {
                let first = disk(seed.wrapping_mul(31), if lies.0 { 500 } else { 0 }, 10);
                let second = disk(seed.wrapping_mul(37), if lies.1 { 500 } else { 0 }, 10);
                crash_run(
                    seed,
                    &EngineOptions::default(),
                    CYCLES,
                    || {
                        PagedLog::open(MirroredPageDevice::new(
                            SimDevice(first.clone()),
                            SimDevice(second.clone()),
                        ))
                    },
                    &[first.clone(), second.clone()],
                )
                .unwrap()
            })
            .collect();
        let sum = total(runs.into_iter());
        assert_eq!(sum.lost, 0, "{lies:?} {sum:?}");
        assert!(sum.commits > 300, "{lies:?} {sum:?}");
        assert_eq!(sum.lied > 100, lies != (false, false), "{lies:?} {sum:?}");
        // The secondary copy of a decision is written only after the primary
        // copy is durable, so power loss alone never tears both copies. Only
        // a primary that lies about its sync can leave both torn; that
        // second fault is refused, never a silent rollback.
        if !lies.0 {
            assert_eq!(sum.refused, 0, "{lies:?} {sum:?}");
        }
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn the_same_seed_replays_the_same_crashes() {
    let options = EngineOptions::default();
    assert_eq!(single(7, &options, 300, 15), single(7, &options, 300, 15));
    assert_ne!(single(7, &options, 300, 15), single(8, &options, 300, 15));
}
