//! Sector-level power loss under the journal-file and LSM backends.
//!
//! `engine_simulation_tests.rs` runs the paged log over a simulated volatile
//! disk: every 512-byte sector independently keeps any prefix of the writes
//! issued since the last sync, sometimes half of the next one, and the file
//! length comes from any prefix of the pending operations. This file puts the
//! two other durable backends of the crate on the same disk model.
//!
//! **Journal file.** `FileTransitionLog` writes newline-framed entries to a real
//! file. `SimJournal` runs the real log on a temporary file and mirrors every
//! append and sync to a simulated file. A failed append leaves nothing, a
//! prefix or the whole line in the cache; a failed sync persists nothing.
//! At power loss the simulated crash image becomes the real file, and the
//! real `FileTransitionLog::open` (its torn-tail repair) and `Engine::open`
//! recover it. The repair truncation itself is a pending, unsynced length
//! change, so a later power loss can undo it. The shared crash workload then
//! requires the exact state of one revision between the last acknowledged and
//! the last attempted commit.
//!
//! **LSM.** `PhysicalLsmJournalStore` keeps a checkpointed paged write-ahead log
//! (a root and two banks) and one paged file per sorted table. Every one of
//! them is a `SimDevice`; creating and deleting a table is a durable directory
//! operation, as the namespace contract requires. Random groups of keys are
//! compare-exchanged and single keys compare-deleted, with stale guards for
//! conflicts, between flushes and compactions. After every power loss the reopened store must read exactly
//! the key state after one acknowledged or attempted group.
//!
//! SQLite is not simulated here: it runs in WAL mode with `synchronous=FULL`
//! (asserted by `engine_sqlite_tests.rs`), and its own crash tests already
//! replay power loss through a VFS that loses and tears unsynced sectors.
#[path = "engine_simulation/disk.rs"]
#[allow(dead_code)]
mod disk;
use disk::*;
use link_cli::LinkError;

// ---------------------------------------------------------------------------
// Journal file
// ---------------------------------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
mod journal_file {
    use super::*;
    use link_cli::engine::{
        ConcurrencyControl, EngineOptions, IsolationLevel, RevisionEncoding,
        SerializableValidation, StateStructure,
    };
    use link_cli::{FileTransitionLog, TransitionLogStore};
    use std::io::Write;
    use std::path::Path;

    /// The real file log, with every append and sync mirrored to `disk`.
    struct SimJournal {
        inner: FileTransitionLog,
        disk: SharedDisk,
    }
    impl TransitionLogStore for SimJournal {
        fn supports_durable_flush(&self) -> bool {
            true
        }
        fn append_log_entry(&mut self, entry: &str) -> Result<(), LinkError> {
            let line = format!("{entry}\n");
            let mut disk = self.disk.lock().unwrap();
            let offset = disk.files[0].view.len();
            if disk.fail() {
                // The failed call left nothing, a prefix or the whole line.
                let keep = match disk.random.below(3) {
                    0 => 0,
                    1 => disk.random.below(line.len() as u64) as usize,
                    _ => line.len(),
                };
                disk.files[0].write(offset, &line.as_bytes()[..keep]);
                std::fs::OpenOptions::new()
                    .append(true)
                    .open(self.inner.path())?
                    .write_all(&line.as_bytes()[..keep])?;
                return Err(LinkError::Transaction("injected append error".into()));
            }
            // `writeln!` on a file issues the entry and its newline separately.
            disk.files[0].write(offset, entry.as_bytes());
            disk.files[0].write(offset + entry.len(), b"\n");
            self.inner.append_log_entry(entry)?;
            if self.inner.sync_on_append() {
                disk.sync(0)?;
            }
            Ok(())
        }
        fn read_log_entries(&mut self) -> Result<Vec<String>, LinkError> {
            self.inner.read_log_entries()
        }
        fn flush_log(&mut self) -> Result<(), LinkError> {
            self.disk.lock().unwrap().sync(0)?;
            self.inner.flush_log()
        }
    }

    /// Makes the simulated image the real file and recovers it with the real
    /// torn-tail repair. The repair's truncation is pending until a sync.
    fn open(path: &Path, disk: &SharedDisk, sync_on_append: bool) -> Result<SimJournal, LinkError> {
        let image = disk.lock().unwrap().files[0].view.clone();
        std::fs::write(path, &image)?;
        let mut inner = FileTransitionLog::open(path)?;
        inner.set_sync_on_append(sync_on_append);
        let repaired = std::fs::metadata(path)?.len() as usize;
        assert!(repaired <= image.len());
        if repaired < image.len() {
            disk.lock().unwrap().files[0].set_len(repaired);
        }
        Ok(SimJournal {
            inner,
            disk: disk.clone(),
        })
    }

    fn configurations() -> Vec<(EngineOptions, bool)> {
        vec![
            (EngineOptions::default(), true),
            // Without per-append sync the commit's own flush is the barrier.
            (
                EngineOptions {
                    concurrency: ConcurrencyControl::Pessimistic,
                    isolation: IsolationLevel::Snapshot,
                    state_structure: StateStructure::Shared,
                    revision_encoding: RevisionEncoding::Changes,
                    ..Default::default()
                },
                false,
            ),
            (
                EngineOptions {
                    serializable_validation: SerializableValidation::ReadSet,
                    state_structure: StateStructure::SortedRuns,
                    revision_encoding: RevisionEncoding::Changes,
                    require_durable_flush: true,
                    ..Default::default()
                },
                false,
            ),
        ]
    }

    // An entry is a few hundred bytes and is synced at once, so a power cut
    // finds unsynced sectors less often than under the paged log.
    const SEEDS: u64 = 48;
    const CYCLES: usize = 8;

    fn single(
        seed: u64,
        options: &EngineOptions,
        sync_on_append: bool,
        lies: u64,
        errors: u64,
    ) -> Result<CrashStats, String> {
        let directory = tempfile::tempdir().map_err(|e| e.to_string())?;
        let path = directory.path().join("journal.log");
        let shared = disk(seed.wrapping_mul(41), lies, errors);
        // An entry is one write and one or two syncs, not a page set: lose
        // power within fewer mutations, so it lands between them.
        shared.lock().unwrap().reach = 8;
        crash_run(
            seed,
            options,
            CYCLES,
            || open(&path, &shared, sync_on_append),
            std::slice::from_ref(&shared),
        )
    }

    #[test]
    fn power_loss_under_the_journal_file_recovers_exactly_an_acknowledged_or_attempted_commit() {
        for (options, sync_on_append) in configurations() {
            let sum = total(
                (1..=SEEDS).map(|seed| single(seed, &options, sync_on_append, 0, 15).unwrap()),
            );
            println!("journal file, sync on append {sync_on_append}: {sum:?}");
            // An honest disk never loses an acknowledged commit or refuses to
            // open; the faults happened and failed commits resolved both ways.
            assert_eq!((sum.refused, sum.lost), (0, 0), "{options:?}: {sum:?}");
            assert!(sum.commits > 250, "{options:?}: {sum:?}");
            assert!(sum.dropped > 20 && sum.torn > 4, "{options:?}: {sum:?}");
            assert!(
                sum.unknown_committed > 0 && sum.unknown_lost > 0,
                "{options:?}: {sum:?}"
            );
        }
    }

    #[test]
    fn a_lying_disk_under_the_journal_file_rolls_back_to_an_exact_revision_or_is_refused() {
        let options = EngineOptions::default();
        let sum = total((1..=SEEDS).map(|seed| single(seed, &options, true, 500, 0).unwrap()));
        // `crash_run` checked that every opened state is the exact state of a
        // committed revision; acknowledged commits were lost to the lies.
        assert!(sum.lied > 100 && sum.lost > 0, "{sum:?}");
    }

    #[test]
    fn the_same_seed_replays_the_same_journal_file_crashes() {
        let options = EngineOptions::default();
        assert_eq!(
            single(7, &options, true, 300, 15),
            single(7, &options, true, 300, 15)
        );
    }
}

// ---------------------------------------------------------------------------
// LSM
// ---------------------------------------------------------------------------

#[cfg(feature = "physical-lsm")]
mod lsm {
    use super::*;
    use link_cli::engine::{
        cluster::{AtomicJournalStore, JournalVersion},
        lsm::{LsmOptions, LsmStoreOptions, LsmTableNamespace, PhysicalLsmJournalStore},
        maintenance::JournalMaintenanceStore,
        paged::CheckpointPagedLog,
    };
    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    /// Table files. Creating and deleting one is durable at once.
    #[derive(Clone)]
    struct SimTables(Arc<Mutex<Tables>>);
    struct Tables {
        files: BTreeMap<u64, SharedDisk>,
        seed: u64,
        errors: u64,
        created: u64,
    }
    impl SimTables {
        fn disks(&self) -> Vec<SharedDisk> {
            self.0.lock().unwrap().files.values().cloned().collect()
        }
    }
    impl LsmTableNamespace for SimTables {
        type Device = SimDevice;
        fn supports_durable_flush(&self) -> bool {
            true
        }
        fn list(&mut self, max_entries: usize) -> Result<Vec<u64>, LinkError> {
            let tables = self.0.lock().unwrap();
            if tables.files.len() > max_entries {
                return Err(LinkError::StorageError("too many tables".into()));
            }
            Ok(tables.files.keys().copied().collect())
        }
        fn create(&mut self, id: u64) -> Result<SimDevice, LinkError> {
            let mut tables = self.0.lock().unwrap();
            assert!(!tables.files.contains_key(&id), "table {id} reused");
            tables.created += 1;
            let seed = tables.seed.wrapping_mul(1009).wrapping_add(id);
            let shared = disk(seed, 0, tables.errors);
            tables.files.insert(id, shared.clone());
            Ok(SimDevice(shared))
        }
        fn open(&mut self, id: u64) -> Result<SimDevice, LinkError> {
            let tables = self.0.lock().unwrap();
            let shared = tables
                .files
                .get(&id)
                .ok_or_else(|| LinkError::StorageError(format!("table {id} is gone")))?;
            Ok(SimDevice(shared.clone()))
        }
        fn close(&mut self, _: &mut SimDevice) -> Result<(), LinkError> {
            Ok(())
        }
        fn delete(&mut self, id: u64) -> Result<(), LinkError> {
            self.0.lock().unwrap().files.remove(&id);
            Ok(())
        }
    }

    /// Small limits, so buffers flush and runs compact within a few groups. A
    /// group holds at most `buffer_keys` keys.
    fn options() -> LsmStoreOptions {
        LsmStoreOptions {
            limits: LsmOptions {
                max_keys: 32,
                max_bytes: 4096,
                buffer_keys: 2,
                buffer_bytes: 128,
                max_runs: 2,
                max_wal_records: 4,
                max_wal_bytes: 8192,
            },
            max_namespace_entries: 16,
        }
    }

    type Store = PhysicalLsmJournalStore<CheckpointPagedLog<SimDevice, SimDevice>, SimTables>;
    /// Present keys and their entries.
    type Keys = BTreeMap<String, Vec<String>>;

    const KEYS: u64 = 6;
    const SEEDS: u64 = 24;
    const CYCLES: usize = 8;

    #[derive(Debug, Default, Clone, PartialEq)]
    struct LsmStats {
        commits: u64,
        conflicts: u64,
        flushes: u64,
        compactions: u64,
        tables: u64,
        crashes: u64,
        refused: u64,
        lost: u64,
        unknown_committed: u64,
        unknown_lost: u64,
        recovery_errors: u64,
        lied: u64,
        dropped: u64,
        torn: u64,
    }

    /// `SIM_TRACE=1` prints every step and recovery.
    fn trace() -> bool {
        std::env::var_os("SIM_TRACE").is_some()
    }

    fn key(index: u64) -> String {
        format!("k{index}")
    }

    fn read_all(store: &mut Store) -> Result<Keys, LinkError> {
        let keys: Vec<String> = (0..KEYS).map(key).collect();
        let versions = store.read(&keys)?;
        Ok(keys
            .into_iter()
            .zip(versions)
            .filter(|(_, version)| !version.entries.is_empty())
            .map(|(key, version)| (key, version.entries))
            .collect())
    }

    fn lsm_run(seed: u64, lies: u64, errors: u64) -> Result<LsmStats, String> {
        let wal: Vec<SharedDisk> = (0..3)
            .map(|i| disk(seed.wrapping_mul(43).wrapping_add(i), lies, errors))
            .collect();
        let tables = SimTables(Arc::new(Mutex::new(Tables {
            files: BTreeMap::new(),
            seed,
            errors,
            created: 0,
        })));
        let all = || -> Vec<SharedDisk> { wal.iter().cloned().chain(tables.disks()).collect() };
        let failed = || all().iter().any(|disk| disk.lock().unwrap().failed);
        let power_loss = || {
            for disk in all() {
                disk.lock().unwrap().power_loss();
            }
        };
        let open = || -> Result<Store, LinkError> {
            let log = CheckpointPagedLog::with_max_pages(
                SimDevice(wal[0].clone()),
                [SimDevice(wal[1].clone()), SimDevice(wal[2].clone())],
                128,
            )?;
            PhysicalLsmJournalStore::open(log, tables.clone(), options())
        };
        let mut random = Random(seed);
        let mut stats = LsmStats::default();
        // States after each acknowledged or attempted group, oldest first.
        let mut states: Vec<Keys> = vec![Keys::new()];
        let (mut acked, mut attempted) = (0_usize, 0_usize);
        let mut value = 0_u64;
        // Create the store without injected errors, as `crash_run` does.
        for disk in &wal {
            disk.lock().unwrap().errors = 0;
        }
        drop(open().map_err(|e| e.to_string())?);
        for disk in &wal {
            disk.lock().unwrap().errors = errors;
        }
        for _ in 0..CYCLES {
            let mut store = match open() {
                Ok(store) => store,
                Err(_) if failed() => {
                    power_loss();
                    stats.crashes += 1;
                    stats.recovery_errors += 1;
                    continue;
                }
                Err(error) => {
                    if trace() {
                        eprintln!("seed {seed}: open refused: {error}");
                    }
                    stats.refused += 1;
                    break;
                }
            };
            let state = read_all(&mut store).map_err(|e| e.to_string())?;
            if trace() {
                eprintln!("seed {seed}: recovered {state:?}");
            }
            let Some(at) = (acked..=attempted).rev().find(|at| states[*at] == state) else {
                return Err(format!(
                    "seed {seed}: recovered {state:?}, expected one of {:?}",
                    &states[acked..=attempted]
                ));
            };
            if attempted > acked {
                if at == attempted {
                    stats.unknown_committed += 1;
                } else {
                    stats.unknown_lost += 1;
                }
            }
            states.truncate(at + 1);
            acked = at;
            attempted = at;
            if random.below(2) == 0 {
                wal[random.below(3) as usize].lock().unwrap().countdown = Some(random.below(80));
            }
            // A guard read earlier, to make stale compare-exchanges.
            let mut stale: Option<(Vec<String>, Vec<JournalVersion>)> = None;
            let steps = 5 + random.below(30);
            for _ in 0..steps {
                if failed() {
                    break;
                }
                let roll = random.below(100);
                // The key state if the group commits; none for maintenance.
                let mut next: Option<Keys> = None;
                let outcome = if roll < 8 {
                    stats.flushes += 1;
                    store.flush().map(|()| None)
                } else if roll < 12 {
                    stats.compactions += 1;
                    store.compact().map(|()| None)
                } else if roll < 20 {
                    // Delete one key at the generation read now or earlier.
                    let name = key(random.below(KEYS));
                    let generation = match stale.take().filter(|_| random.below(4) == 0) {
                        Some((keys, versions)) => keys
                            .iter()
                            .position(|k| *k == name)
                            .map_or(1, |at| versions[at].generation),
                        None => match store.read(std::slice::from_ref(&name)) {
                            Ok(versions) => versions[0].generation,
                            Err(_) if failed() => break,
                            Err(error) => return Err(format!("seed {seed}: {error}")),
                        },
                    };
                    if generation == 0 {
                        continue;
                    }
                    let mut after = states[attempted].clone();
                    after.remove(&name);
                    next = Some(after);
                    store.compare_delete(&name, generation).map(Some)
                } else {
                    let mut keys: Vec<String> = (0..1 + random.below(2))
                        .map(|_| key(random.below(KEYS)))
                        .collect();
                    keys.sort();
                    keys.dedup();
                    let guard = match stale.take().filter(|_| random.below(4) == 0) {
                        Some(old) => old,
                        None => {
                            let versions = match store.read(&keys) {
                                Ok(versions) => versions,
                                Err(_) if failed() => break,
                                Err(error) => return Err(format!("seed {seed}: {error}")),
                            };
                            if random.below(3) == 0 {
                                stale = Some((keys.clone(), versions.clone()));
                            }
                            (keys, versions)
                        }
                    };
                    let updates: Vec<Option<Vec<String>>> = guard
                        .0
                        .iter()
                        .map(|_| {
                            value += 1;
                            // `None` only guards the key's generation.
                            (random.below(6) != 0).then(|| vec![format!("v{value}")])
                        })
                        .collect();
                    let mut after = states[attempted].clone();
                    for (key, update) in guard.0.iter().zip(&updates) {
                        if let Some(entries) = update {
                            after.insert(key.clone(), entries.clone());
                        }
                    }
                    next = Some(after);
                    store
                        .compare_exchange(&guard.0, &guard.1, &updates)
                        .map(Some)
                }
                .map_err(|error| (failed(), error));
                if trace() {
                    eprintln!("seed {seed}: roll {roll} next {next:?} -> {outcome:?}");
                }
                match outcome {
                    Ok(None) => {}
                    Ok(Some(true)) => {
                        states.push(next.expect("a group"));
                        acked = states.len() - 1;
                        attempted = acked;
                        stats.commits += 1;
                    }
                    Ok(Some(false)) => stats.conflicts += 1,
                    Err((true, _)) => {
                        // A failed group may still have reached the WAL; a
                        // failed flush or compaction keeps the key state.
                        if let Some(next) = next {
                            states.push(next);
                            attempted = states.len() - 1;
                        }
                        break;
                    }
                    Err((false, error)) => return Err(format!("seed {seed}: {error}")),
                }
            }
            drop(store);
            power_loss();
            stats.crashes += 1;
        }
        stats.tables = tables.0.lock().unwrap().created;
        for disk in all() {
            let disk = disk.lock().unwrap();
            stats.lied += disk.lied;
            stats.dropped += disk.dropped;
            stats.torn += disk.torn;
        }
        Ok(stats)
    }

    fn sum(runs: impl Iterator<Item = LsmStats>) -> LsmStats {
        runs.fold(LsmStats::default(), |mut s, r| {
            s.commits += r.commits;
            s.conflicts += r.conflicts;
            s.flushes += r.flushes;
            s.compactions += r.compactions;
            s.tables += r.tables;
            s.crashes += r.crashes;
            s.refused += r.refused;
            s.lost += r.lost;
            s.unknown_committed += r.unknown_committed;
            s.unknown_lost += r.unknown_lost;
            s.recovery_errors += r.recovery_errors;
            s.lied += r.lied;
            s.dropped += r.dropped;
            s.torn += r.torn;
            s
        })
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn power_loss_under_the_lsm_store_recovers_exactly_an_acknowledged_or_attempted_group() {
        let total = sum((1..=SEEDS).map(|seed| lsm_run(seed, 0, 15).unwrap()));
        // Deletes are counted with the other acknowledged groups.
        println!("lsm: {total:?}");
        assert_eq!((total.refused, total.lost), (0, 0), "{total:?}");
        assert!(total.commits > 200 && total.conflicts > 10, "{total:?}");
        assert!(total.tables > 20 && total.compactions > 10, "{total:?}");
        assert!(total.dropped > 20 && total.torn > 4, "{total:?}");
        assert!(
            total.unknown_committed > 0 && total.unknown_lost > 0,
            "{total:?}"
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn the_same_seed_replays_the_same_lsm_crashes() {
        assert_eq!(lsm_run(7, 0, 15), lsm_run(7, 0, 15));
        assert_ne!(lsm_run(7, 0, 15), lsm_run(8, 0, 15));
    }
}
