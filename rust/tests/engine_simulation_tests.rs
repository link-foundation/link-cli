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
//! state of an earlier revision, never a mixture. A mirror of an honest and a
//! lying disk must recover every acknowledged commit.
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
    paged::{MirroredPageDevice, PageDevice, PagedLog, RepairPageDevice, PAGE_SIZE},
    ConcurrencyControl, Engine, EngineOptions, EngineTransaction, HistoryPolicy, IsolationLevel,
    RevisionEncoding, SerializableValidation, StateStructure,
};
use link_cli::{LinkError, LinksStorage};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::{Arc, Mutex},
};

#[path = "engine_history/check.rs"]
mod check;
use check::{check, History, Transaction};

/// splitmix64: the same sequence in Rust, C# and JS.
struct Random(u64);
impl Random {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
    fn per_mille(&mut self, chance: u64) -> bool {
        self.below(1000) < chance
    }
}

fn injected(what: &str) -> LinkError {
    LinkError::Transaction(format!("injected {what}"))
}

// ---------------------------------------------------------------------------
// Simulated disk
// ---------------------------------------------------------------------------

const SECTOR: usize = 512;

#[derive(Clone, Debug)]
enum Pending {
    Write(usize, Vec<u8>),
    Length(usize),
}

/// One file: what survives power loss, what the page cache shows, and the
/// operations between them.
#[derive(Clone, Default)]
struct SimFile {
    durable: Vec<u8>,
    view: Vec<u8>,
    pending: Vec<Pending>,
}
impl SimFile {
    fn write(&mut self, offset: usize, bytes: &[u8]) {
        if self.view.len() < offset + bytes.len() {
            self.view.resize(offset + bytes.len(), 0);
        }
        self.view[offset..offset + bytes.len()].copy_from_slice(bytes);
        self.pending.push(Pending::Write(offset, bytes.to_vec()));
    }
    fn set_len(&mut self, length: usize) {
        self.view.resize(length, 0);
        self.pending.push(Pending::Length(length));
    }
    fn sync(&mut self) {
        self.durable = self.view.clone();
        self.pending.clear();
    }
    /// Applies `ops` to the bytes of `range`, as they would land on the disk.
    fn replay(sector: &mut [u8], start: usize, ops: &[Pending]) {
        let end = start + sector.len();
        for op in ops {
            match op {
                Pending::Write(offset, bytes) => {
                    let from = (*offset).max(start);
                    let to = (offset + bytes.len()).min(end);
                    if from < to {
                        sector[from - start..to - start]
                            .copy_from_slice(&bytes[from - offset..to - offset]);
                    }
                }
                Pending::Length(length) if *length < end => {
                    sector[length.saturating_sub(start)..].fill(0);
                }
                Pending::Length(_) => {}
            }
        }
    }
    fn touches(op: &Pending, start: usize, end: usize) -> bool {
        match op {
            Pending::Write(offset, bytes) => *offset < end && offset + bytes.len() > start,
            Pending::Length(length) => *length < end,
        }
    }
    /// Power loss. Returns how many sectors lost or tore a pending write.
    fn crash(&mut self, random: &mut Random) -> (u64, u64) {
        let (mut dropped, mut torn) = (0, 0);
        if !self.pending.is_empty() {
            // The length after any prefix of the pending operations.
            let cut = random.below(self.pending.len() as u64 + 1) as usize;
            let mut length = self.durable.len();
            for op in &self.pending[..cut] {
                length = match op {
                    Pending::Write(offset, bytes) => length.max(offset + bytes.len()),
                    Pending::Length(new) => *new,
                };
            }
            let mut result = self.durable.clone();
            result.resize(length, 0);
            for start in (0..length).step_by(SECTOR) {
                let end = (start + SECTOR).min(length);
                let touching: Vec<usize> = (0..self.pending.len())
                    .filter(|at| Self::touches(&self.pending[*at], start, end))
                    .collect();
                if touching.is_empty() {
                    continue;
                }
                // Each sector independently keeps any prefix of its writes.
                let keep = random.below(touching.len() as u64 + 1) as usize;
                let mut sector = result[start..end].to_vec();
                if keep > 0 {
                    Self::replay(&mut sector, start, &self.pending[..=touching[keep - 1]]);
                }
                if keep < touching.len() {
                    dropped += 1;
                    if random.below(4) == 0 {
                        // The next write lands halfway.
                        let mut next = sector.clone();
                        Self::replay(
                            &mut next,
                            start,
                            &self.pending[touching[keep]..=touching[keep]],
                        );
                        let half = sector.len() / 2;
                        sector[..half].copy_from_slice(&next[..half]);
                        torn += 1;
                    }
                }
                result[start..end].copy_from_slice(&sector);
            }
            self.durable = result;
        }
        self.view = self.durable.clone();
        self.pending.clear();
        (dropped, torn)
    }
}

/// Pages, metadata slot 0 and metadata slot 1, as `FilePageDevice` keeps them.
struct Disk {
    files: [SimFile; 3],
    random: Random,
    /// Per mille of syncs acknowledged without persisting anything.
    lies: u64,
    /// Per mille of writes and syncs that fail.
    errors: u64,
    /// After one failure the device fails every mutation until power loss.
    failed: bool,
    /// Mutations left before the power fails in the middle of the work.
    countdown: Option<u64>,
    lied: u64,
    dropped: u64,
    torn: u64,
}
type SharedDisk = Arc<Mutex<Disk>>;

fn disk(seed: u64, lies: u64, errors: u64) -> SharedDisk {
    Arc::new(Mutex::new(Disk {
        files: Default::default(),
        random: Random(seed),
        lies,
        errors,
        failed: false,
        countdown: None,
        lied: 0,
        dropped: 0,
        torn: 0,
    }))
}

impl Disk {
    /// Whether this mutation fails; a failed write may still reach the cache.
    fn fail(&mut self) -> bool {
        match self.countdown {
            Some(0) => {
                self.countdown = None;
                self.failed = true;
            }
            Some(left) => self.countdown = Some(left - 1),
            None => {}
        }
        if !self.failed && self.errors > 0 && self.random.per_mille(self.errors) {
            self.failed = true;
            return true;
        }
        self.failed
    }
    fn mutate(&mut self, file: usize, apply: impl FnOnce(&mut SimFile)) -> Result<(), LinkError> {
        if self.fail() {
            if self.random.below(2) == 0 {
                apply(&mut self.files[file]);
            }
            return Err(injected("write error"));
        }
        apply(&mut self.files[file]);
        Ok(())
    }
    fn sync(&mut self, file: usize) -> Result<(), LinkError> {
        if self.fail() {
            return Err(injected("sync error"));
        }
        if self.lies > 0 && self.random.per_mille(self.lies) {
            self.lied += 1;
            return Ok(());
        }
        self.files[file].sync();
        Ok(())
    }
    fn power_loss(&mut self) {
        let Disk {
            files,
            random,
            dropped,
            torn,
            ..
        } = self;
        for file in files.iter_mut() {
            let (lost, half) = file.crash(random);
            *dropped += lost;
            *torn += half;
        }
        self.failed = false;
        self.countdown = None;
    }
}

struct SimDevice(SharedDisk);
impl SimDevice {
    fn disk(&self) -> std::sync::MutexGuard<'_, Disk> {
        self.0.lock().unwrap()
    }
}
impl PageDevice for SimDevice {
    fn supports_durable_flush(&self) -> bool {
        true
    }
    fn new_database_id(&mut self) -> Result<[u8; 16], LinkError> {
        let mut disk = self.disk();
        let mut id = [0; 16];
        id[..8].copy_from_slice(&disk.random.next().to_le_bytes());
        id[8..].copy_from_slice(&disk.random.next().to_le_bytes());
        Ok(id)
    }
    fn data_length(&mut self) -> Result<u64, LinkError> {
        Ok(self.disk().files[0].view.len() as u64)
    }
    fn read_metadata(&mut self, slot: usize) -> Result<Option<Vec<u8>>, LinkError> {
        let view = &self.disk().files[1 + slot].view;
        Ok(match view.len() {
            0 => None,
            128 => Some(view.clone()),
            _ => Some(vec![0]),
        })
    }
    fn write_metadata(&mut self, slot: usize, bytes: &[u8; 128]) -> Result<(), LinkError> {
        self.disk().mutate(1 + slot, |file| file.write(0, bytes))
    }
    fn sync_metadata(&mut self, slot: usize) -> Result<(), LinkError> {
        self.disk().sync(1 + slot)
    }
    fn read_page(&mut self, index: u64) -> Result<[u8; PAGE_SIZE], LinkError> {
        let disk = self.disk();
        let start = index as usize * PAGE_SIZE;
        let page = disk.files[0]
            .view
            .get(start..start + PAGE_SIZE)
            .ok_or_else(|| injected("short read"))?;
        Ok(page.try_into().unwrap())
    }
    fn write_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        self.disk()
            .mutate(0, |file| file.write(index as usize * PAGE_SIZE, bytes))
    }
    fn sync_data(&mut self) -> Result<(), LinkError> {
        self.disk().sync(0)
    }
    fn truncate_data(&mut self, length: u64) -> Result<(), LinkError> {
        self.disk().mutate(0, |file| file.set_len(length as usize))
    }
}
impl RepairPageDevice for SimDevice {
    fn repair_page(&mut self, index: u64, bytes: &[u8; PAGE_SIZE]) -> Result<(), LinkError> {
        if self.disk().files[0].view.len() < index as usize * PAGE_SIZE {
            return Err(injected("repair hole"));
        }
        self.write_page(index, bytes)
    }
}

// ---------------------------------------------------------------------------
// Crash workload
// ---------------------------------------------------------------------------

const LINKS: u32 = 4;
const DISK_CLIENTS: u64 = 3;
const FIRST_WRITE: u32 = 1000;

#[derive(Debug, Default, Clone, PartialEq)]
struct CrashStats {
    commits: u64,
    crashes: u64,
    /// Opens that refused a damaged database.
    refused: u64,
    /// Recoveries to a revision before the last acknowledged one.
    lost: u64,
    /// Commits whose flush failed, then recovered as committed / not committed.
    unknown_committed: u64,
    unknown_lost: u64,
    /// Recoveries interrupted by an injected error, then retried.
    recovery_errors: u64,
    lied: u64,
    dropped: u64,
    torn: u64,
}

type State = BTreeMap<u32, u32>;

struct DiskClient<D: PageDevice> {
    session: EngineTransaction<u32, PagedLog<D>>,
    ops: Vec<Option<(u32, bool)>>,
    writes: State,
}

/// Runs `cycles` rounds of work, each ended by power loss, and checks every
/// recovery. Returns an error message for any recovery that is not the exact
/// state of one committed revision.
fn crash_run<D: PageDevice>(
    seed: u64,
    options: &EngineOptions,
    cycles: usize,
    device: impl Fn() -> D,
    disks: &[SharedDisk],
) -> Result<CrashStats, String> {
    let failed = || disks.iter().any(|disk| disk.lock().unwrap().failed);
    let mut random = Random(seed);
    let mut stats = CrashStats::default();
    let mut states: BTreeMap<u64, State> = BTreeMap::from([(0, State::new())]);
    let (mut acked, mut attempted) = (0_u64, 0_u64);
    let mut value = FIRST_WRITE;
    for _ in 0..cycles {
        let db = match PagedLog::open(device())
            .and_then(|log| Engine::<u32, _>::open(log, options.clone()))
        {
            Ok(db) => db,
            // Recovery itself truncates and repairs, so it can hit an injected
            // error too; the machine loses power again and retries.
            Err(_) if failed() => {
                for disk in disks {
                    disk.lock().unwrap().power_loss();
                }
                stats.crashes += 1;
                stats.recovery_errors += 1;
                continue;
            }
            Err(error) => {
                if std::env::var_os("SIM_TRACE").is_some() {
                    eprintln!("seed {seed}: open refused: {error}");
                }
                stats.refused += 1;
                break;
            }
        };
        let snapshot = db.snapshot().map_err(|e| e.to_string())?;
        let revision = snapshot.revision;
        let state: State = snapshot
            .links()
            .iter()
            .map(|link| (link.index, link.target))
            .collect();
        if revision > attempted || states.get(&revision) != Some(&state) {
            return Err(format!(
                "seed {seed}: recovered revision {revision} {state:?}, expected one of {:?} \
                 between acknowledged {acked} and attempted {attempted}",
                states
            ));
        }
        if revision < acked {
            stats.lost += 1;
        }
        if attempted > acked {
            if revision == attempted {
                stats.unknown_committed += 1;
            } else {
                stats.unknown_lost += 1;
            }
        }
        states.retain(|at, _| *at <= revision);
        acked = revision;
        attempted = revision;
        if revision == 0 {
            // Set up every link in one commit, without injected errors.
            let errors: Vec<u64> = disks
                .iter()
                .map(|disk| std::mem::take(&mut disk.lock().unwrap().errors))
                .collect();
            let mut setup = db.begin().map_err(|e| e.to_string())?;
            for link in 1..=LINKS {
                setup.ensure_link_created(link).unwrap();
                setup.update_link(link, link, link).unwrap();
            }
            let at = setup.commit().map_err(|e| e.to_string())?;
            states.insert(at, (1..=LINKS).map(|link| (link, link)).collect());
            acked = at;
            attempted = at;
            for (disk, errors) in disks.iter().zip(errors) {
                disk.lock().unwrap().errors = errors;
            }
        }
        // Half of the rounds lose power in the middle of a write or sync,
        // leaving whatever the cache had not yet persisted.
        if random.below(2) == 0 {
            disks[0].lock().unwrap().countdown = Some(random.below(60));
        }
        let mut clients: Vec<Option<DiskClient<D>>> = (0..DISK_CLIENTS).map(|_| None).collect();
        let steps = 5 + random.below(40);
        for _ in 0..steps {
            if failed() {
                break;
            }
            let slot = &mut clients[random.below(DISK_CLIENTS) as usize];
            if slot.is_none() {
                let Ok(session) = db.begin() else { continue };
                let length = 1 + random.below(4);
                let mut ops: Vec<Option<(u32, bool)>> = (0..length)
                    .map(|_| Some((1 + random.below(LINKS as u64) as u32, random.below(2) == 0)))
                    .collect();
                ops.push(None);
                *slot = Some(DiskClient {
                    session,
                    ops,
                    writes: State::new(),
                });
            }
            let client = slot.as_mut().unwrap();
            let op = client.ops.remove(0);
            let finished = match op {
                Some((link, false)) => {
                    client
                        .session
                        .get_link(link)
                        .expect("links are never deleted");
                    false
                }
                Some((link, true)) => {
                    value += 1;
                    let refused = client.session.update_link(link, link, value).is_err();
                    client.writes.insert(link, value);
                    refused
                }
                None => {
                    let latest = *states.keys().next_back().unwrap();
                    let mut next = states[&latest].clone();
                    next.extend(client.writes.iter().map(|(k, v)| (*k, *v)));
                    match client.session.commit() {
                        // A read-only commit reports the revision it read.
                        Ok(at) if client.writes.is_empty() && at == latest => {}
                        Ok(at) => {
                            if at != latest + 1 {
                                return Err(format!("seed {seed}: revision {at} after {latest}"));
                            }
                            states.insert(at, next);
                            acked = at;
                            attempted = at;
                            stats.commits += 1;
                        }
                        Err(_) if failed() => {
                            states.insert(latest + 1, next);
                            attempted = latest + 1;
                        }
                        Err(_) => {}
                    }
                    true
                }
            };
            if finished {
                slot.take();
            }
        }
        drop(clients);
        drop(db);
        for disk in disks {
            disk.lock().unwrap().power_loss();
        }
        stats.crashes += 1;
    }
    for disk in disks {
        let disk = disk.lock().unwrap();
        stats.lied += disk.lied;
        stats.dropped += disk.dropped;
        stats.torn += disk.torn;
    }
    Ok(stats)
}

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

fn total(runs: impl Iterator<Item = CrashStats>) -> CrashStats {
    runs.fold(CrashStats::default(), |mut sum, run| {
        sum.commits += run.commits;
        sum.crashes += run.crashes;
        sum.refused += run.refused;
        sum.lost += run.lost;
        sum.unknown_committed += run.unknown_committed;
        sum.unknown_lost += run.unknown_lost;
        sum.recovery_errors += run.recovery_errors;
        sum.lied += run.lied;
        sum.dropped += run.dropped;
        sum.torn += run.torn;
        sum
    })
}

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
        || SimDevice(shared.clone()),
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
    for lying_first in [true, false] {
        let runs: Vec<CrashStats> = (1..=DISK_SEEDS)
            .map(|seed| {
                let honest = disk(seed.wrapping_mul(31), 0, 10);
                let lying = disk(seed.wrapping_mul(37), 500, 10);
                let (first, second) = if lying_first {
                    (lying.clone(), honest.clone())
                } else {
                    (honest.clone(), lying.clone())
                };
                crash_run(
                    seed,
                    &EngineOptions::default(),
                    CYCLES,
                    || MirroredPageDevice::new(SimDevice(first.clone()), SimDevice(second.clone())),
                    &[first.clone(), second.clone()],
                )
                .unwrap()
            })
            .collect();
        let sum = total(runs.into_iter());
        assert_eq!((sum.refused, sum.lost), (0, 0), "{sum:?}");
        assert!(sum.lied > 100 && sum.commits > 300, "{sum:?}");
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn the_same_seed_replays_the_same_crashes() {
    let options = EngineOptions::default();
    assert_eq!(single(7, &options, 300, 15), single(7, &options, 300, 15));
    assert_ne!(single(7, &options, 300, 15), single(8, &options, 300, 15));
}

// ---------------------------------------------------------------------------
// Simulated network
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq)]
enum Link {
    Up,
    /// Nothing gets through.
    Cut,
    /// Requests arrive; replies are lost.
    OneWay,
}

struct Request {
    keys: Vec<String>,
    expected: Vec<JournalVersion>,
    updates: Vec<Option<Vec<String>>>,
    caller: usize,
}

/// The atomic store and every member's link to it.
struct Network {
    random: Random,
    faults: bool,
    links: Vec<Link>,
    revision: u64,
    journals: BTreeMap<String, JournalVersion>,
    /// The transaction id of every applied compare-exchange, by revision.
    applied: BTreeMap<u64, usize>,
    /// Requests the network holds back and delivers later.
    delayed: VecDeque<Request>,
    /// The transaction a member is committing; the store logs it.
    caller: usize,
    delivered_late: u64,
}
impl Network {
    fn apply(&mut self, request: &Request) -> bool {
        if request
            .keys
            .iter()
            .zip(&request.expected)
            .any(|(key, value)| {
                self.journals.get(key).map_or(0, |v| v.generation) != value.generation
            })
        {
            return false;
        }
        self.revision += 1;
        let generation = self.revision;
        for (key, update) in request.keys.iter().zip(&request.updates) {
            if let Some(entries) = update {
                self.journals.insert(
                    key.clone(),
                    JournalVersion {
                        generation,
                        entries: entries.clone(),
                    },
                );
            }
        }
        self.applied.insert(generation, request.caller);
        true
    }
    fn deliver_one_late(&mut self) {
        if let Some(request) = self.delayed.pop_front() {
            if self.apply(&request) {
                self.delivered_late += 1;
            }
        }
    }
    /// Whether the request reaches the store.
    fn send(&mut self, member: usize) -> bool {
        !self.faults || (self.links[member] != Link::Cut && !self.random.per_mille(40))
    }
    /// Whether the reply reaches the member.
    fn reply(&mut self, member: usize) -> bool {
        !self.faults || (self.links[member] == Link::Up && !self.random.per_mille(40))
    }
}

struct Member {
    network: Arc<Mutex<Network>>,
    id: usize,
}
impl AtomicJournalStore for Member {
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        let mut network = self.network.lock().unwrap();
        if !network.send(self.id) {
            return Err(injected("dropped request"));
        }
        let values = keys
            .iter()
            .map(|key| network.journals.get(key).cloned().unwrap_or_default())
            .collect();
        if !network.reply(self.id) {
            return Err(injected("lost reply"));
        }
        Ok(values)
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        let mut network = self.network.lock().unwrap();
        let request = Request {
            keys: keys.to_vec(),
            expected: expected.to_vec(),
            updates: updates.to_vec(),
            caller: network.caller,
        };
        if !network.send(self.id) {
            return Err(injected("dropped request"));
        }
        if network.faults && network.random.per_mille(60) {
            // A slow link: the client times out; the request arrives later.
            network.delayed.push_back(request);
            return Err(injected("timeout"));
        }
        let applied = network.apply(&request);
        if !network.reply(self.id) {
            return Err(injected("lost reply"));
        }
        Ok(applied)
    }
}

const SHARDS: [&str; 2] = ["a", "b"];
const SHARD_LINKS: u32 = 2;
const MEMBERS: usize = 3;
const NET_CLIENTS: u64 = 5;
const NET_TRANSACTIONS: usize = 80;
const NET_SEEDS: u64 = 30;

/// One key per link of every shard, as the checker sees it.
fn key(shard: usize, link: u32) -> u32 {
    10 * (shard as u32 + 1) + link
}

#[derive(Clone, Copy, Debug)]
enum NetOp {
    Read(usize, u32),
    Write(usize, u32),
    Scan(usize),
    Commit,
    Rollback,
}

struct NetClient {
    member: usize,
    id: usize,
    shards: Vec<usize>,
    session: Option<link_cli::engine::cluster::ClusterTransaction<u32, Member>>,
    ops: Vec<NetOp>,
}

#[derive(Debug, Default, PartialEq)]
struct NetStats {
    committed: u64,
    conflicts: u64,
    unknown_committed: u64,
    unknown_aborted: u64,
    delivered_late: u64,
    unavailable: u64,
}

/// A history of `NET_TRANSACTIONS` transactions by clients spread over
/// `MEMBERS` cluster members.
fn network_history(seed: u64) -> (History, NetStats) {
    let network = Arc::new(Mutex::new(Network {
        random: Random(seed.wrapping_mul(7919)),
        faults: false,
        links: vec![Link::Up; MEMBERS],
        revision: 0,
        journals: BTreeMap::new(),
        applied: BTreeMap::new(),
        delayed: VecDeque::new(),
        caller: 0,
        delivered_late: 0,
    }));
    let members: Vec<ClusterEngine<u32, Member>> = (0..MEMBERS)
        .map(|id| {
            let member = ClusterEngine::new(
                Member {
                    network: network.clone(),
                    id,
                },
                EngineOptions::default(),
            )
            .unwrap();
            // One member rewrites journals as checkpoints; the others read them.
            if id == 1 {
                member.with_journal_checkpoint_interval(3).unwrap()
            } else {
                member
            }
        })
        .collect();
    // Transaction 0 installs the initial values.
    let mut initial = Transaction::default();
    let mut setup = members[0].begin(&SHARDS).unwrap();
    for (shard, name) in SHARDS.iter().enumerate() {
        for link in 1..=SHARD_LINKS {
            let session = setup.shard(name).unwrap();
            session.ensure_link_created(link).unwrap();
            session.update_link(link, link, key(shard, link)).unwrap();
            initial.writes.push((key(shard, link), key(shard, link)));
        }
    }
    setup.commit().unwrap();
    let mut records: Vec<Transaction> = vec![initial];
    // 0 acknowledged, 1 refused, 2 unknown.
    let mut outcomes: Vec<u8> = vec![0];
    network.lock().unwrap().faults = true;
    let mut random = Random(seed);
    let mut stats = NetStats::default();
    let mut clients: Vec<Option<NetClient>> = (0..NET_CLIENTS).map(|_| None).collect();
    let (mut started, mut finished, mut now) = (0, 0, 0);
    let mut value = FIRST_WRITE;
    while finished < NET_TRANSACTIONS {
        now += 1;
        {
            let mut network = network.lock().unwrap();
            if random.below(25) == 0 {
                let member = random.below(MEMBERS as u64) as usize;
                network.links[member] = match random.below(4) {
                    0 => Link::Cut,
                    1 => Link::OneWay,
                    _ => Link::Up,
                };
            }
            if random.below(6) == 0 {
                network.deliver_one_late();
            }
        }
        let index = random.below(NET_CLIENTS) as usize;
        let slot = &mut clients[index];
        if slot.is_none() {
            if started == NET_TRANSACTIONS {
                continue;
            }
            started += 1;
            let shards = match random.below(3) {
                0 => vec![0],
                1 => vec![1],
                _ => vec![0, 1],
            };
            let mut ops = Vec::new();
            for _ in 0..1 + random.below(4) {
                let shard = shards[random.below(shards.len() as u64) as usize];
                let link = 1 + random.below(SHARD_LINKS as u64) as u32;
                ops.push(match random.below(10) {
                    0..=3 => NetOp::Read(shard, link),
                    4..=7 => NetOp::Write(shard, link),
                    _ => NetOp::Scan(shard),
                });
            }
            ops.push(if random.below(20) == 0 {
                NetOp::Rollback
            } else {
                NetOp::Commit
            });
            records.push(Transaction {
                began: now,
                ..Default::default()
            });
            outcomes.push(1);
            *slot = Some(NetClient {
                member: index % MEMBERS,
                id: records.len() - 1,
                shards,
                session: None,
                ops,
            });
        }
        let client = slot.as_mut().unwrap();
        let record = &mut records[client.id];
        if client.session.is_none() {
            let names: Vec<&str> = client.shards.iter().map(|s| SHARDS[*s]).collect();
            match members[client.member].begin(&names) {
                Ok(session) => client.session = Some(session),
                Err(_) => {
                    // The member could not reach the store: nothing happened.
                    stats.unavailable += 1;
                    record.ended = now;
                    slot.take();
                    finished += 1;
                    continue;
                }
            }
        }
        let session = client.session.as_mut().unwrap();
        let op = client.ops.remove(0);
        let mut done = matches!(op, NetOp::Commit | NetOp::Rollback);
        match op {
            NetOp::Read(shard, link) => {
                let target = session
                    .shard(SHARDS[shard])
                    .unwrap()
                    .get_link(link)
                    .unwrap()
                    .target;
                observe(record, key(shard, link), target);
            }
            NetOp::Scan(shard) => {
                for found in session.shard(SHARDS[shard]).unwrap().all_links() {
                    observe(record, key(shard, found.index), found.target);
                }
            }
            NetOp::Write(shard, link) => {
                value += 1;
                session
                    .shard(SHARDS[shard])
                    .unwrap()
                    .update_link(link, link, value)
                    .unwrap();
                record.writes.push((key(shard, link), value));
            }
            NetOp::Commit => {
                network.lock().unwrap().caller = client.id;
                match session.commit() {
                    Ok(()) => outcomes[client.id] = 0,
                    Err(error) if error.to_string().contains("conflict") => {
                        stats.conflicts += 1;
                    }
                    Err(error) => {
                        assert!(error.to_string().contains("indeterminate"), "{error}");
                        outcomes[client.id] = 2;
                    }
                }
            }
            NetOp::Rollback => session.rollback(),
        }
        if client.ops.is_empty() {
            done = true;
        }
        if done {
            record.ended = now;
            slot.take();
            finished += 1;
        }
    }
    // Heal the network; requests still in flight arrive.
    let mut network = network.lock().unwrap();
    while !network.delayed.is_empty() {
        network.deliver_one_late();
    }
    network.faults = false;
    stats.delivered_late = network.delivered_late;
    for (revision, id) in &network.applied {
        let record = &mut records[*id];
        record.revision = Some(*revision);
        match outcomes[*id] {
            0 => stats.committed += 1,
            2 => {
                // Unknown to the client: it could have ended any time later.
                record.ended = usize::MAX;
                stats.unknown_committed += 1;
            }
            _ => panic!("seed {seed}: refused transaction {id} was applied"),
        }
    }
    for (id, outcome) in outcomes.iter().enumerate().skip(1) {
        match (*outcome, records[id].revision) {
            (0, None) => panic!("seed {seed}: acknowledged transaction {id} is not applied"),
            (2, None) => stats.unknown_aborted += 1,
            _ => {}
        }
    }
    drop(network);
    // Read the final state through a healed member.
    let mut last = members[2].begin(&SHARDS).unwrap();
    let mut final_state = BTreeMap::new();
    for (shard, name) in SHARDS.iter().enumerate() {
        for link in last.shard(name).unwrap().all_links() {
            final_state.insert(key(shard, link.index), link.target);
        }
    }
    (
        History {
            transactions: records,
            final_state,
        },
        stats,
    )
}

fn observe(record: &mut Transaction, link: u32, target: u32) {
    match record
        .writes
        .iter()
        .rev()
        .find(|(written, _)| *written == link)
    {
        Some(&(_, own)) => record.own_reads.push((link, target, own)),
        None => record.reads.push((link, target)),
    }
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn cluster_members_over_a_faulty_network_stay_strictly_serializable() {
    let mut sum = NetStats::default();
    let mut shards_per_commit = BTreeSet::new();
    for seed in 1..=NET_SEEDS {
        let (history, stats) = network_history(seed);
        let anomalies = check(&history);
        assert!(anomalies.is_empty(), "seed {seed}: {anomalies:?}");
        for transaction in history.transactions.iter().filter(|t| t.revision.is_some()) {
            let shards: BTreeSet<u32> = transaction.writes.iter().map(|(k, _)| k / 10).collect();
            shards_per_commit.insert(shards.len());
        }
        sum.committed += stats.committed;
        sum.conflicts += stats.conflicts;
        sum.unknown_committed += stats.unknown_committed;
        sum.unknown_aborted += stats.unknown_aborted;
        sum.delivered_late += stats.delivered_late;
        sum.unavailable += stats.unavailable;
    }
    // Every kind of outcome happened, including cross-shard commits, late
    // deliveries that still committed, and unknown outcomes both ways.
    assert!(shards_per_commit.contains(&2), "{shards_per_commit:?}");
    assert!(sum.committed > 500 && sum.conflicts > 50, "{sum:?}");
    assert!(
        sum.unknown_committed > 0 && sum.unknown_aborted > 0,
        "{sum:?}"
    );
    assert!(sum.delivered_late > 0 && sum.unavailable > 0, "{sum:?}");
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn the_same_seed_replays_the_same_network_history() {
    let (first, first_stats) = network_history(3);
    let (again, again_stats) = network_history(3);
    assert_eq!(first.transactions, again.transactions);
    assert_eq!(first_stats, again_stats);
}
