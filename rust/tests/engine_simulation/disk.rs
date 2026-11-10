//! A simulated volatile disk and the crash workload shared by the engine
//! simulations: every file keeps its durable bytes and the writes issued
//! since its last sync, and power loss keeps any per-sector prefix of them.
use link_cli::engine::{
    paged::{PageDevice, RepairPageDevice, PAGE_SIZE},
    Engine, EngineOptions, EngineTransaction,
};
use link_cli::{LinkError, LinksStorage, TransitionLogStore};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

/// splitmix64: the same sequence in Rust, C# and JS.
pub struct Random(pub u64);
impl Random {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn below(&mut self, bound: u64) -> u64 {
        self.next() % bound
    }
    pub fn per_mille(&mut self, chance: u64) -> bool {
        self.below(1000) < chance
    }
}

pub fn injected(what: &str) -> LinkError {
    LinkError::Transaction(format!("injected {what}"))
}

// ---------------------------------------------------------------------------
// Simulated disk
// ---------------------------------------------------------------------------

pub const SECTOR: usize = 512;

#[derive(Clone, Debug)]
pub enum Pending {
    Write(usize, Vec<u8>),
    Length(usize),
}

/// One file: what survives power loss, what the page cache shows, and the
/// operations between them.
#[derive(Clone, Default)]
pub struct SimFile {
    pub durable: Vec<u8>,
    pub view: Vec<u8>,
    pub pending: Vec<Pending>,
}
impl SimFile {
    pub fn write(&mut self, offset: usize, bytes: &[u8]) {
        if self.view.len() < offset + bytes.len() {
            self.view.resize(offset + bytes.len(), 0);
        }
        self.view[offset..offset + bytes.len()].copy_from_slice(bytes);
        self.pending.push(Pending::Write(offset, bytes.to_vec()));
    }
    pub fn set_len(&mut self, length: usize) {
        self.view.resize(length, 0);
        self.pending.push(Pending::Length(length));
    }
    pub fn sync(&mut self) {
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
    pub fn crash(&mut self, random: &mut Random) -> (u64, u64) {
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
pub struct Disk {
    pub files: [SimFile; 3],
    pub random: Random,
    /// Per mille of syncs acknowledged without persisting anything.
    pub lies: u64,
    /// Per mille of writes and syncs that fail.
    pub errors: u64,
    /// After one failure the device fails every mutation until power loss.
    pub failed: bool,
    /// Mutations left before the power fails in the middle of the work.
    pub countdown: Option<u64>,
    /// Bound of the mutation count after which `crash_run` cuts the power.
    pub reach: u64,
    pub lied: u64,
    pub dropped: u64,
    pub torn: u64,
}
pub type SharedDisk = Arc<Mutex<Disk>>;

pub fn disk(seed: u64, lies: u64, errors: u64) -> SharedDisk {
    Arc::new(Mutex::new(Disk {
        files: Default::default(),
        random: Random(seed),
        lies,
        errors,
        failed: false,
        countdown: None,
        reach: 60,
        lied: 0,
        dropped: 0,
        torn: 0,
    }))
}

impl Disk {
    /// Whether this mutation fails; a failed write may still reach the cache.
    pub fn fail(&mut self) -> bool {
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
    pub fn mutate(
        &mut self,
        file: usize,
        apply: impl FnOnce(&mut SimFile),
    ) -> Result<(), LinkError> {
        if self.fail() {
            if self.random.below(2) == 0 {
                apply(&mut self.files[file]);
            }
            return Err(injected("write error"));
        }
        apply(&mut self.files[file]);
        Ok(())
    }
    pub fn sync(&mut self, file: usize) -> Result<(), LinkError> {
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
    pub fn power_loss(&mut self) {
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

pub struct SimDevice(pub SharedDisk);
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

pub const LINKS: u32 = 4;
pub const DISK_CLIENTS: u64 = 3;
pub const FIRST_WRITE: u32 = 1000;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct CrashStats {
    pub commits: u64,
    pub crashes: u64,
    /// Opens that refused a damaged database.
    pub refused: u64,
    /// Recoveries to a revision before the last acknowledged one.
    pub lost: u64,
    /// Commits whose flush failed, then recovered as committed / not committed.
    pub unknown_committed: u64,
    pub unknown_lost: u64,
    /// Recoveries interrupted by an injected error, then retried.
    pub recovery_errors: u64,
    pub lied: u64,
    pub dropped: u64,
    pub torn: u64,
}

pub type State = BTreeMap<u32, u32>;

pub struct DiskClient<L: TransitionLogStore> {
    pub session: EngineTransaction<u32, L>,
    pub ops: Vec<Option<(u32, bool)>>,
    pub writes: State,
}

/// Runs `cycles` rounds of work, each ended by power loss, and checks every
/// recovery. Returns an error message for any recovery that is not the exact
/// state of one committed revision.
pub fn crash_run<L: TransitionLogStore>(
    seed: u64,
    options: &EngineOptions,
    cycles: usize,
    open: impl Fn() -> Result<L, LinkError>,
    disks: &[SharedDisk],
) -> Result<CrashStats, String> {
    let failed = || disks.iter().any(|disk| disk.lock().unwrap().failed);
    let mut random = Random(seed);
    let mut stats = CrashStats::default();
    let mut states: BTreeMap<u64, State> = BTreeMap::from([(0, State::new())]);
    let (mut acked, mut attempted) = (0_u64, 0_u64);
    let mut value = FIRST_WRITE;
    // Create the database without injected errors: power loss during the
    // very first metadata write leaves no decision at all, and an unreadable
    // log is refused rather than treated as new.
    let errors: Vec<u64> = disks
        .iter()
        .map(|disk| std::mem::take(&mut disk.lock().unwrap().errors))
        .collect();
    drop(open().map_err(|e| e.to_string())?);
    for (disk, errors) in disks.iter().zip(errors) {
        disk.lock().unwrap().errors = errors;
    }
    for _ in 0..cycles {
        let db = match open().and_then(|log| Engine::<u32, _>::open(log, options.clone())) {
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
            let reach = disks[0].lock().unwrap().reach;
            disks[0].lock().unwrap().countdown = Some(random.below(reach));
        }
        let mut clients: Vec<Option<DiskClient<L>>> = (0..DISK_CLIENTS).map(|_| None).collect();
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

pub fn total(runs: impl Iterator<Item = CrashStats>) -> CrashStats {
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
