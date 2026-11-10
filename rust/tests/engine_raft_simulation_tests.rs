//! Deterministic simulation of an etcd-like Raft group with its own timers,
//! skewed and jumping clocks, and an unreliable operating system and network.
//!
//! `engine_simulation_tests.rs` checks `ClusterEngine` members against one
//! idealised atomic store. Here the store itself is three replicas of the
//! draft `RaftNode` (`src/engine/raft.rs`), the algorithm etcd runs, and the
//! simulation owns everything etcd would get from the operating system:
//!
//! * **Clocks and timers.** Each replica reads its own clock, which runs at
//!   its own rate (0.5x to 2x real time), jumps forward and backward, and
//!   changes rate during the run. Election timeouts, heartbeats, the
//!   CheckQuorum interval and the client deadlines are all measured on these
//!   skewed clocks.
//! * **Processes.** Replicas crash and restart, keeping only their hard state
//!   and log, as a process that fsyncs them would.
//! * **Network.** Messages are dropped, delayed and reordered. A replica can
//!   be isolated, *deaf* (it sends but does not receive) or *mute* (it
//!   receives but does not send).
//!
//! Clients on every member run random transactions through `ClusterEngine`,
//! whose store calls propose a command to their own replica and wait, on that
//! replica's skewed clock, for the replica to apply it. The Elle-style checker
//! shared with `engine_history_tests.rs` searches every history for anomalies,
//! real-time edges included, and every replica must end with the same
//! committed log and state.
//!
//! **Liveness while faults last.** With at most one replica faulty at a time
//! — the failure a three-member group is built to survive — faults never
//! stop, yet the gap between successive acknowledged commits must stay below
//! a bound. A negative control turns PreVote and CheckQuorum off and shows
//! that the same fault schedules then stall the group far longer: a deaf or
//! mute replica keeps disrupting it, which is why etcd has both options.
//!
//! **Safety under any faults.** With every replica allowed to fail at once,
//! progress may stop, but the history must stay strictly serializable.
use link_cli::engine::{
    cluster::{AtomicJournalStore, ClusterEngine, JournalVersion},
    raft::{RaftCommand, RaftConfig, RaftEnvelope, RaftNode, RaftOutcome, RaftRole},
    EngineOptions,
};
use link_cli::{LinkError, LinksStorage};
use std::{
    collections::BTreeMap,
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

fn failure(what: &str) -> LinkError {
    LinkError::Transaction(format!("injected {what}"))
}

// ---------------------------------------------------------------------------
// Simulated clocks, processes and network
// ---------------------------------------------------------------------------

const REPLICAS: usize = 3;
/// Client deadline, in the local clock units of the client's replica.
const DEADLINE: u64 = 400;
/// The most global ticks one call may take, whatever the clocks do.
const CALL_LIMIT: u64 = 20_000;
/// Real-time ticks before a probe read is abandoned for a fresh one.
const PROBE_TIMEOUT: u64 = 300;

/// A clock running at `rate` per mille of real (global) time since `since`.
#[derive(Clone, Copy, Debug)]
struct Clock {
    rate: u64,
    base: u64,
    since: u64,
}
impl Clock {
    fn local(&self, now: u64) -> u64 {
        self.base + (now - self.since) * self.rate / 1000
    }
    fn reset(&mut self, now: u64, local: u64, rate: u64) {
        *self = Clock {
            rate,
            base: local,
            since: now,
        };
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Fault {
    None,
    /// Nothing in or out.
    Isolated,
    /// Sends, but receives nothing.
    Deaf,
    /// Receives, but sends nothing.
    Mute,
    /// Crashed; restarts when the fault heals.
    Down,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Mode {
    /// At most one replica is faulty at any time.
    Minority,
    /// Any number of replicas fail at once.
    Any,
}

#[derive(Debug, Default, PartialEq, Clone)]
struct SimStats {
    delivered: u64,
    dropped: u64,
    faults: u64,
    restarts: u64,
    clock_jumps: u64,
    leaders: u64,
    max_term: u64,
    /// Probe reads answered, and the longest real time between two answers.
    probes: u64,
    longest_gap: u64,
}

struct Sim {
    now: u64,
    /// Drives the network only.
    random: Random,
    /// Drives faults and clocks only, so a fault schedule does not depend on
    /// the traffic and is the same with and without PreVote and CheckQuorum.
    schedule: Random,
    nodes: Vec<RaftNode>,
    clocks: Vec<Clock>,
    faults: Vec<Fault>,
    heals: Vec<u64>,
    mode: Mode,
    chaos: bool,
    /// Messages in flight, by delivery time and send order.
    queue: BTreeMap<(u64, u64), RaftEnvelope>,
    sent: u64,
    next_id: u64,
    leader_term: u64,
    stats: SimStats,
    /// The transaction a member is committing, and every exchange's caller.
    caller: usize,
    callers: BTreeMap<u64, usize>,
    /// The probe read in flight: replica, command id and when it started.
    probe: Option<(usize, u64, u64)>,
    last_answer: u64,
}

impl Sim {
    fn new(seed: u64, mode: Mode, pre_vote_and_check_quorum: bool) -> Self {
        let mut schedule = Random(seed.wrapping_mul(6151));
        let mut clocks = Vec::new();
        let mut nodes = Vec::new();
        for id in 0..REPLICAS {
            let rate = 500 + schedule.below(1501);
            let base = schedule.below(10_000);
            clocks.push(Clock {
                rate,
                base,
                since: 0,
            });
            let mut config = RaftConfig::new(id, REPLICAS);
            config.seed = seed.wrapping_mul(31).wrapping_add(id as u64);
            config.pre_vote = pre_vote_and_check_quorum;
            config.check_quorum = pre_vote_and_check_quorum;
            nodes.push(RaftNode::new(config, base));
        }
        Sim {
            now: 0,
            random: Random(seed.wrapping_mul(7919)),
            schedule,
            nodes,
            clocks,
            faults: vec![Fault::None; REPLICAS],
            heals: vec![0; REPLICAS],
            mode,
            chaos: false,
            queue: BTreeMap::new(),
            sent: 0,
            next_id: 0,
            leader_term: 0,
            stats: SimStats::default(),
            caller: 0,
            callers: BTreeMap::new(),
            probe: None,
            last_answer: 0,
        }
    }

    fn local(&self, node: usize) -> u64 {
        self.clocks[node].local(self.now)
    }

    fn can_send(&self, node: usize) -> bool {
        matches!(self.faults[node], Fault::None | Fault::Deaf)
    }
    fn can_receive(&self, node: usize) -> bool {
        matches!(self.faults[node], Fault::None | Fault::Mute)
    }

    fn flush(&mut self) {
        for node in 0..REPLICAS {
            for envelope in self.nodes[node].take_messages() {
                if !self.can_send(node) || (self.chaos && self.random.per_mille(30)) {
                    self.stats.dropped += 1;
                    continue;
                }
                let mut delay = 1 + self.random.below(10);
                if self.chaos && self.random.per_mille(50) {
                    delay += self.random.below(150);
                }
                self.sent += 1;
                self.queue.insert((self.now + delay, self.sent), envelope);
            }
        }
    }

    fn heal(&mut self, node: usize) {
        if self.faults[node] == Fault::Down {
            let local = self.local(node);
            self.nodes[node].restart(local);
            self.stats.restarts += 1;
        }
        self.faults[node] = Fault::None;
    }

    fn inject(&mut self) {
        for node in 0..REPLICAS {
            if self.faults[node] != Fault::None && self.now >= self.heals[node] {
                self.heal(node);
            }
        }
        if !self.schedule.per_mille(3) {
            return;
        }
        let node = self.schedule.below(REPLICAS as u64) as usize;
        let kind = self.schedule.below(7);
        match kind {
            0..=3 => {
                if self.mode == Mode::Minority {
                    for other in 0..REPLICAS {
                        if other != node && self.faults[other] != Fault::None {
                            self.heal(other);
                        }
                    }
                }
                if self.faults[node] != Fault::None {
                    self.heal(node);
                }
                self.faults[node] =
                    [Fault::Isolated, Fault::Deaf, Fault::Mute, Fault::Down][kind as usize];
                self.heals[node] = self.now + 200 + self.schedule.below(1800);
                self.stats.faults += 1;
            }
            4 | 5 => {
                // NTP steps the clock forward or backward.
                let local = self.local(node);
                let step = self.schedule.below(400);
                let jumped = if kind == 4 {
                    local + step
                } else {
                    local.saturating_sub(step)
                };
                let rate = self.clocks[node].rate;
                self.clocks[node].reset(self.now, jumped, rate);
                self.stats.clock_jumps += 1;
            }
            _ => {
                // The clock drifts at a new rate.
                let local = self.local(node);
                let rate = 500 + self.schedule.below(1501);
                self.clocks[node].reset(self.now, local, rate);
            }
        }
    }

    fn step(&mut self) {
        self.now += 1;
        if self.chaos {
            self.inject();
        }
        while let Some((&(at, sent), _)) = self.queue.first_key_value() {
            if at > self.now {
                break;
            }
            let envelope = self.queue.remove(&(at, sent)).unwrap();
            if !self.can_receive(envelope.to) {
                self.stats.dropped += 1;
                continue;
            }
            self.stats.delivered += 1;
            let to = envelope.to;
            let local = self.local(to);
            self.nodes[to].step(local, envelope);
        }
        for node in 0..REPLICAS {
            if self.faults[node] != Fault::Down {
                let local = self.local(node);
                self.nodes[node].tick(local);
            }
        }
        for node in 0..REPLICAS {
            self.stats.max_term = self.stats.max_term.max(self.nodes[node].term());
            if self.nodes[node].role() == RaftRole::Leader {
                let term = self.nodes[node].term();
                if term > self.leader_term {
                    self.leader_term = term;
                    self.stats.leaders += 1;
                }
            }
        }
        if self.chaos {
            self.probe();
        }
        self.flush();
    }

    /// Measures availability apart from the clients, whose calls run one at a
    /// time: a probe keeps a linearizable read in flight on a healthy
    /// replica, and the longest real time between two answers is the longest
    /// time the group could not serve a healthy replica.
    fn probe(&mut self) {
        if let Some((node, id, since)) = self.probe {
            if self.nodes[node].take_outcome(id).is_some() {
                self.stats.probes += 1;
                self.stats.longest_gap = self.stats.longest_gap.max(self.now - self.last_answer);
                self.last_answer = self.now;
                self.probe = None;
            } else if self.now - since > PROBE_TIMEOUT || self.faults[node] != Fault::None {
                self.nodes[node].forget(id);
                self.probe = None;
            }
        }
        if self.probe.is_some() {
            return;
        }
        for offset in 0..REPLICAS {
            let node = (self.now as usize + offset) % REPLICAS;
            if self.faults[node] == Fault::None {
                self.next_id += 1;
                let id = self.next_id;
                let local = self.local(node);
                if self.nodes[node].propose(
                    local,
                    RaftCommand::Read {
                        id,
                        keys: Vec::new(),
                    },
                ) {
                    self.probe = Some((node, id, self.now));
                }
                return;
            }
        }
    }

    /// Proposes on `member`'s replica and waits, on that replica's clock, for
    /// it to apply the command.
    fn call(
        &mut self,
        member: usize,
        command: impl Fn(u64) -> RaftCommand,
    ) -> Result<RaftOutcome, LinkError> {
        if self.faults[member] == Fault::Down {
            return Err(failure("refused connection"));
        }
        self.next_id += 1;
        let id = self.next_id;
        let command = command(id);
        if matches!(command, RaftCommand::CompareExchange { .. }) {
            self.callers.insert(id, self.caller);
        }
        let (start, started) = (self.local(member), self.now);
        let mut proposed = false;
        loop {
            if self.faults[member] == Fault::Down {
                return Err(failure("connection reset"));
            }
            if !proposed {
                // Without a known leader nothing was sent; proposing again
                // cannot duplicate the command.
                let local = self.local(member);
                proposed = self.nodes[member].propose(local, command.clone());
                self.flush();
            }
            if let Some(outcome) = self.nodes[member].take_outcome(id) {
                return Ok(outcome);
            }
            if self.local(member) >= start + DEADLINE {
                self.nodes[member].forget(id);
                return Err(failure("deadline exceeded"));
            }
            assert!(self.now - started < CALL_LIMIT, "a call never ended");
            self.step();
        }
    }
}

struct Member {
    sim: Arc<Mutex<Sim>>,
    id: usize,
}
impl AtomicJournalStore for Member {
    fn read(&mut self, keys: &[String]) -> Result<Vec<JournalVersion>, LinkError> {
        let keys = keys.to_vec();
        match self
            .sim
            .lock()
            .unwrap()
            .call(self.id, |id| RaftCommand::Read {
                id,
                keys: keys.clone(),
            })? {
            RaftOutcome::Read(values) => Ok(values),
            other => panic!("{other:?}"),
        }
    }
    fn compare_exchange(
        &mut self,
        keys: &[String],
        expected: &[JournalVersion],
        updates: &[Option<Vec<String>>],
    ) -> Result<bool, LinkError> {
        match self
            .sim
            .lock()
            .unwrap()
            .call(self.id, |id| RaftCommand::CompareExchange {
                id,
                keys: keys.to_vec(),
                expected: expected.to_vec(),
                updates: updates.to_vec(),
            })? {
            RaftOutcome::Exchanged(applied) => Ok(applied),
            other => panic!("{other:?}"),
        }
    }
}

// ---------------------------------------------------------------------------
// Client histories
// ---------------------------------------------------------------------------

const SHARDS: [&str; 2] = ["a", "b"];
const SHARD_LINKS: u32 = 2;
const CLIENTS: u64 = 5;
const TRANSACTIONS: usize = 80;
const FIRST_WRITE: u32 = 1000;

fn key(shard: usize, link: u32) -> u32 {
    10 * (shard as u32 + 1) + link
}

#[derive(Clone, Copy, Debug)]
enum Op {
    Read(usize, u32),
    Write(usize, u32),
    Scan(usize),
    Commit,
    Rollback,
}

struct Client {
    member: usize,
    id: usize,
    shards: Vec<usize>,
    session: Option<link_cli::engine::cluster::ClusterTransaction<u32, Member>>,
    ops: Vec<Op>,
}

#[derive(Debug, Default, PartialEq, Clone)]
struct Stats {
    committed: u64,
    conflicts: u64,
    unknown_committed: u64,
    unknown_aborted: u64,
    unavailable: u64,
    sim: SimStats,
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

fn history(seed: u64, mode: Mode, pre_vote_and_check_quorum: bool) -> (History, Stats) {
    let sim = Arc::new(Mutex::new(Sim::new(seed, mode, pre_vote_and_check_quorum)));
    let members: Vec<ClusterEngine<u32, Member>> = (0..REPLICAS)
        .map(|id| {
            let member = ClusterEngine::new(
                Member {
                    sim: sim.clone(),
                    id,
                },
                EngineOptions::default(),
            )
            .unwrap();
            if id == 1 {
                member.with_journal_checkpoint_interval(3).unwrap()
            } else {
                member
            }
        })
        .collect();
    // A fault-free start: elect a leader, then install the initial values.
    let now = || sim.lock().unwrap().now as usize;
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
    sim.lock().unwrap().caller = 0;
    setup.commit().unwrap();
    let mut records: Vec<Transaction> = vec![initial];
    // 0 acknowledged, 1 refused, 2 unknown.
    let mut outcomes: Vec<u8> = vec![0];
    {
        let mut sim = sim.lock().unwrap();
        sim.chaos = true;
        sim.last_answer = sim.now;
    }
    let mut random = Random(seed);
    let mut stats = Stats::default();
    let mut clients: Vec<Option<Client>> = (0..CLIENTS).map(|_| None).collect();
    let (mut started, mut finished) = (0, 0);
    let mut value = FIRST_WRITE;
    while finished < TRANSACTIONS {
        {
            let mut sim = sim.lock().unwrap();
            for _ in 0..1 + random.below(3) {
                sim.step();
            }
        }
        let index = random.below(CLIENTS) as usize;
        let slot = &mut clients[index];
        if slot.is_none() {
            if started == TRANSACTIONS {
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
                    0..=3 => Op::Read(shard, link),
                    4..=7 => Op::Write(shard, link),
                    _ => Op::Scan(shard),
                });
            }
            ops.push(if random.below(20) == 0 {
                Op::Rollback
            } else {
                Op::Commit
            });
            records.push(Transaction {
                began: now(),
                ..Default::default()
            });
            outcomes.push(1);
            *slot = Some(Client {
                member: index % REPLICAS,
                id: records.len() - 1,
                shards,
                session: None,
                ops,
            });
        }
        let client = slot.as_mut().unwrap();
        if client.session.is_none() {
            let names: Vec<&str> = client.shards.iter().map(|s| SHARDS[*s]).collect();
            match members[client.member].begin(&names) {
                Ok(session) => client.session = Some(session),
                Err(_) => {
                    stats.unavailable += 1;
                    records[client.id].ended = now();
                    slot.take();
                    finished += 1;
                    continue;
                }
            }
        }
        let record = &mut records[client.id];
        let session = client.session.as_mut().unwrap();
        let op = client.ops.remove(0);
        let mut done = matches!(op, Op::Commit | Op::Rollback);
        match op {
            Op::Read(shard, link) => {
                let target = session
                    .shard(SHARDS[shard])
                    .unwrap()
                    .get_link(link)
                    .unwrap()
                    .target;
                observe(record, key(shard, link), target);
            }
            Op::Scan(shard) => {
                for found in session.shard(SHARDS[shard]).unwrap().all_links() {
                    observe(record, key(shard, found.index), found.target);
                }
            }
            Op::Write(shard, link) => {
                value += 1;
                session
                    .shard(SHARDS[shard])
                    .unwrap()
                    .update_link(link, link, value)
                    .unwrap();
                record.writes.push((key(shard, link), value));
            }
            Op::Commit => {
                sim.lock().unwrap().caller = client.id;
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
            Op::Rollback => session.rollback(),
        }
        if client.ops.is_empty() {
            done = true;
        }
        if done {
            record.ended = now();
            slot.take();
            finished += 1;
        }
    }
    // Heal everything, let the replicas converge, and read the outcome of
    // every exchange from the replicated log.
    let mut guard = sim.lock().unwrap();
    guard.chaos = false;
    for node in 0..REPLICAS {
        guard.heal(node);
    }
    let healed = guard.now;
    loop {
        guard.step();
        let applied: Vec<u64> = guard.nodes.iter().map(|n| n.applied_index()).collect();
        let last = guard.nodes.iter().map(|n| n.log().len() as u64 - 1).max();
        if guard.nodes.iter().any(|n| n.role() == RaftRole::Leader)
            && applied.iter().all(|a| Some(*a) == last)
        {
            break;
        }
        assert!(
            guard.now - healed < 5_000,
            "seed {seed}: no convergence after heal"
        );
    }
    for node in 1..REPLICAS {
        assert_eq!(
            guard.nodes[node].log(),
            guard.nodes[0].log(),
            "seed {seed}: replicas disagree on the committed log"
        );
        assert_eq!(guard.nodes[node].state(), guard.nodes[0].state());
    }
    let exchanges = guard.nodes[0].state().exchanges.clone();
    for (revision, command) in exchanges {
        let id = guard.callers[&command];
        let record = &mut records[id];
        assert!(
            record.revision.is_none(),
            "seed {seed}: transaction {id} applied twice"
        );
        record.revision = Some(revision);
        match outcomes[id] {
            0 => stats.committed += 1,
            2 => {
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
    stats.sim = guard.stats.clone();
    drop(guard);
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

const SEEDS: u64 = 20;
/// Real-time ticks: the longest a healthy replica may wait for an answer while
/// one replica at a time keeps failing. Election timeouts are 100 to 200
/// local units and clocks run at 0.5x to 2x, so one lost leader costs up to
/// about 600 ticks; the bound leaves room for a second election when a vote
/// is lost or a clock steps backward.
const STALL_BOUND: u64 = 1_500;

fn run(mode: Mode, pre_vote_and_check_quorum: bool) -> (Stats, Vec<u64>) {
    let mut sum = Stats::default();
    let mut stalls = Vec::new();
    for seed in 1..=SEEDS {
        let (history, stats) = history(seed, mode, pre_vote_and_check_quorum);
        let anomalies = check(&history);
        assert!(anomalies.is_empty(), "seed {seed}: {anomalies:?}");
        sum.committed += stats.committed;
        sum.conflicts += stats.conflicts;
        sum.unknown_committed += stats.unknown_committed;
        sum.unknown_aborted += stats.unknown_aborted;
        sum.unavailable += stats.unavailable;
        sum.sim.delivered += stats.sim.delivered;
        sum.sim.dropped += stats.sim.dropped;
        sum.sim.faults += stats.sim.faults;
        sum.sim.restarts += stats.sim.restarts;
        sum.sim.clock_jumps += stats.sim.clock_jumps;
        sum.sim.leaders += stats.sim.leaders;
        sum.sim.max_term = sum.sim.max_term.max(stats.sim.max_term);
        sum.sim.probes += stats.sim.probes;
        sum.sim.longest_gap = sum.sim.longest_gap.max(stats.sim.longest_gap);
        stalls.push(stats.sim.longest_gap);
    }
    println!("{mode:?} pre-vote/check-quorum={pre_vote_and_check_quorum}: {sum:?}");
    println!("longest stall per seed: {stalls:?}");
    (sum, stalls)
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn raft_group_stays_strictly_serializable_and_live_while_one_replica_keeps_failing() {
    let (sum, _) = run(Mode::Minority, true);
    assert!(
        sum.sim.longest_gap <= STALL_BOUND,
        "the group stalled for {} ticks: {sum:?}",
        sum.sim.longest_gap
    );
    assert!(
        sum.committed > 400 && sum.conflicts > 50 && sum.sim.probes > 4000,
        "{sum:?}"
    );
    assert!(sum.sim.restarts > 0 && sum.sim.clock_jumps > 0 && sum.sim.leaders > SEEDS);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn without_pre_vote_and_check_quorum_one_faulty_replica_stalls_the_group() {
    // The same schedules, without the two options: still safe, but a deaf or
    // mute replica now stops progress for much longer.
    let (with, _) = run(Mode::Minority, true);
    let (without, _) = run(Mode::Minority, false);
    assert!(
        without.sim.longest_gap > STALL_BOUND && without.sim.max_term > 4 * with.sim.max_term,
        "with: {with:?}\nwithout: {without:?}"
    );
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn raft_group_stays_strictly_serializable_when_every_replica_fails() {
    let (sum, _) = run(Mode::Any, true);
    assert!(sum.committed > 300, "{sum:?}");
    assert!(
        sum.unknown_committed > 0 && sum.unknown_aborted > 0,
        "{sum:?}"
    );
    assert!(sum.unavailable > 0, "{sum:?}");
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn the_same_seed_replays_the_same_raft_history() {
    let (first, first_stats) = history(3, Mode::Any, true);
    let (again, again_stats) = history(3, Mode::Any, true);
    assert_eq!(first.transactions, again.transactions);
    assert_eq!(first_stats, again_stats);
}
