//! The network half of `engine_simulation_tests.rs`: `ClusterEngine` members
//! over simulated links to one atomic journal store, checked for strict
//! serializability.
use super::*;

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
