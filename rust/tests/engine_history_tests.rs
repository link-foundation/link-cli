//! Long random histories, checked for isolation anomalies by dependency cycles.
//!
//! `engine_interleaving_tests.rs` runs every interleaving of two short
//! transactions. Jepsen's Elle instead checks long histories of many
//! transactions: it builds Adya's dependency graph from what clients observed
//! and searches it for cycles. This test does the same against the engine.
//!
//! A seeded scheduler runs several clients against one engine, one step at a
//! time, so every history is deterministic and replays from its seed. Every
//! write stores a value no other write uses, so each read names the
//! transaction it observed. Commit revisions give each link's version order.
//! From them the checker draws Adya's edges between committed transactions:
//!
//! - `ww`: one transaction installed the version of a link after another's;
//! - `wr`: a transaction read a version another installed;
//! - `rw`: a transaction read a version another then replaced;
//! - `rt`: a transaction committed before another began (real time).
//!
//! It reports:
//!
//! - **G1a** (aborted read), **G1b** (intermediate read), a read of a value
//!   nobody wrote, a read that misses the transaction's own write, a lost write
//!   (the final state is not the last committed version);
//! - **G0 / G1c**: a cycle without `rw` edges. No level allows one;
//! - **G-single**: a cycle with exactly one `rw` edge (read skew, lost update,
//!   a stale read). Snapshot isolation forbids it;
//! - **G2**: any other cycle (write skew). Only serializability forbids it.
//!
//! Where a level allows an anomaly, some history must show it, so the test
//! cannot pass by refusing every transaction or by a checker that sees nothing.
//! The JS/WASM conformance (`js/client/test/conformance/history.mjs`) and the C#
//! `EngineHistoryTests` use the same generator, seeds and checker.
use link_cli::engine::{
    ConcurrencyControl, Engine, EngineOptions, EngineTransaction, IsolationLevel, MemoryLog,
    SerializableValidation,
};
use link_cli::LinksStorage;
use std::collections::{BTreeMap, BTreeSet};

const CLIENTS: u64 = 4;
const KEYS: u64 = 3;
const TRANSACTIONS: usize = 60;
const SEEDS: u64 = 20;
/// The first value a write stores; the initial value of link `k` is `k`.
const FIRST_WRITE: u32 = 1000;

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
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Op {
    Read(u32),
    /// `lock_for_update`: a read that reserves the link.
    Lock(u32),
    /// Every link at once.
    Scan,
    Write(u32),
    Commit,
    Rollback,
}

/// One transaction's plan: up to four operations, then commit or (rarely)
/// rollback.
fn plan(random: &mut Random, pessimistic: bool) -> Vec<Op> {
    let length = 1 + random.below(4);
    let mut ops = Vec::new();
    for _ in 0..length {
        let kind = random.below(10);
        let key = 1 + random.below(KEYS) as u32;
        ops.push(match kind {
            0..=3 => Op::Read(key),
            4..=6 | 9 => Op::Write(key),
            7 => Op::Scan,
            _ if pessimistic => Op::Lock(key),
            _ => Op::Read(key),
        });
    }
    ops.push(if random.below(20) == 0 {
        Op::Rollback
    } else {
        Op::Commit
    });
    ops
}

/// What a transaction did, as a client saw it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transaction {
    /// `(link, value)` in the order the client read them.
    reads: Vec<(u32, u32)>,
    /// `(link, value)` in the order the client wrote them.
    writes: Vec<(u32, u32)>,
    /// Reads that follow the transaction's own write to that link:
    /// `(link, value read, value last written)`.
    own_reads: Vec<(u32, u32, u32)>,
    /// The commit revision; `None` for aborted or rolled-back transactions.
    revision: Option<u64>,
    /// Scheduler steps: when the transaction began and when it ended.
    began: usize,
    ended: usize,
}

#[derive(Debug, Default)]
pub struct History {
    /// Transaction 0 installs the initial values.
    transactions: Vec<Transaction>,
    /// The final committed value of each link.
    final_state: BTreeMap<u32, u32>,
}

type Session = EngineTransaction<u32, MemoryLog>;

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

struct Client {
    session: Option<Session>,
    ops: Vec<Op>,
    next: usize,
    record: Transaction,
}

fn step(
    db: &Engine<u32, MemoryLog>,
    client: &mut Client,
    op: Op,
    value: &mut u32,
) -> Result<(), link_cli::LinkError> {
    if client.session.is_none() {
        client.session = Some(db.begin()?);
    }
    let session = client.session.as_mut().unwrap();
    let record = &mut client.record;
    match op {
        Op::Read(link) => {
            let target = session
                .get_link(link)
                .expect("links are never deleted")
                .target;
            observe(record, link, target);
        }
        Op::Lock(link) => {
            let target = session.lock_for_update(link)?.expect("present").target;
            observe(record, link, target);
        }
        Op::Scan => {
            for found in session.all_links() {
                observe(record, found.index, found.target);
            }
        }
        Op::Write(link) => {
            *value += 1;
            session.update_link(link, link, *value)?;
            record.writes.push((link, *value));
        }
        Op::Commit => record.revision = Some(session.commit()?),
        Op::Rollback => {
            client.session.take();
        }
    }
    Ok(())
}

/// Runs one seeded history: `CLIENTS` clients, `TRANSACTIONS` transactions.
pub fn generate(options: &EngineOptions, seed: u64) -> History {
    let db = Engine::open(MemoryLog::default(), options.clone()).unwrap();
    let mut initial = Transaction::default();
    let mut setup = db.begin().unwrap();
    for link in 1..=KEYS as u32 {
        setup.ensure_link_created(link).unwrap();
        setup.update_link(link, link, link).unwrap();
        initial.writes.push((link, link));
    }
    initial.revision = Some(setup.commit().unwrap());
    let mut history = History {
        transactions: vec![initial],
        ..Default::default()
    };
    let mut random = Random(seed);
    let pessimistic = options.concurrency == ConcurrencyControl::Pessimistic;
    let mut clients: Vec<Option<Client>> = (0..CLIENTS).map(|_| None).collect();
    let (mut started, mut finished, mut now) = (0, 0, 0);
    let mut value = FIRST_WRITE - 1;
    while finished < TRANSACTIONS {
        now += 1;
        let slot = &mut clients[random.below(CLIENTS) as usize];
        if slot.is_none() {
            if started == TRANSACTIONS {
                continue;
            }
            started += 1;
            *slot = Some(Client {
                session: None,
                ops: plan(&mut random, pessimistic),
                next: 0,
                record: Transaction {
                    began: now,
                    ..Default::default()
                },
            });
        }
        let client = slot.as_mut().unwrap();
        let op = client.ops[client.next];
        client.next += 1;
        // A refused step ends the transaction, as a client would roll back.
        let failed = step(&db, client, op, &mut value).is_err();
        if failed || client.next == client.ops.len() {
            let mut client = slot.take().unwrap();
            client.session.take();
            if failed {
                client.record.revision = None;
            }
            client.record.ended = now;
            history.transactions.push(client.record);
            finished += 1;
        }
    }
    history.final_state = db
        .snapshot()
        .unwrap()
        .links()
        .iter()
        .map(|link| (link.index, link.target))
        .collect();
    history
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Anomaly {
    /// A read of a value no transaction wrote.
    Garbage,
    /// A read of a write by a transaction that did not commit.
    G1a,
    /// A read of a value its writer later overwrote in the same transaction.
    G1b,
    /// A read that does not return the transaction's own latest write.
    Internal,
    /// The final state is not the last committed write of each link.
    LostWrite,
    /// A cycle of `ww`, `wr` and real-time edges.
    G1c,
    /// A cycle with exactly one anti-dependency.
    GSingle,
    /// A cycle with more than one anti-dependency.
    G2,
}

/// Every anomaly in the history, each with one example: the transactions
/// involved.
pub fn check(history: &History) -> BTreeMap<Anomaly, Vec<usize>> {
    let transactions = &history.transactions;
    let mut found: BTreeMap<Anomaly, Vec<usize>> = BTreeMap::new();
    let report = |found: &mut BTreeMap<Anomaly, Vec<usize>>, anomaly, example: Vec<usize>| {
        found.entry(anomaly).or_insert(example);
    };
    // Who wrote each value, and whether it was that writer's last write.
    let mut writer: BTreeMap<u32, (usize, bool)> = BTreeMap::new();
    for (id, transaction) in transactions.iter().enumerate() {
        for (at, (link, value)) in transaction.writes.iter().enumerate() {
            let last = !transaction.writes[at + 1..]
                .iter()
                .any(|(later, _)| later == link);
            writer.insert(*value, (id, last));
        }
    }
    let committed: Vec<usize> = (0..transactions.len())
        .filter(|id| transactions[*id].revision.is_some())
        .collect();
    let last_write = |id: usize, link: u32| {
        transactions[id]
            .writes
            .iter()
            .rev()
            .find(|(written, _)| *written == link)
            .map(|(_, value)| *value)
    };
    // Each link's versions in commit order: the committed transactions that
    // wrote it.
    let mut versions: BTreeMap<u32, Vec<usize>> = BTreeMap::new();
    let mut by_revision = committed.clone();
    by_revision.sort_by_key(|id| transactions[*id].revision);
    for id in by_revision {
        let links: BTreeSet<u32> = transactions[id].writes.iter().map(|(l, _)| *l).collect();
        for link in links {
            versions.entry(link).or_default().push(id);
        }
    }
    for (link, writers) in &versions {
        let expected = last_write(writers[writers.len() - 1], *link);
        if history.final_state.get(link).copied() != expected {
            report(&mut found, Anomaly::LostWrite, writers.clone());
        }
    }
    // Edges between committed transactions, by kind.
    let n = transactions.len();
    let mut dependencies: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); n];
    let mut anti: Vec<(usize, usize)> = Vec::new();
    for writers in versions.values() {
        for pair in writers.windows(2) {
            dependencies[pair[0]].insert(pair[1]);
        }
    }
    for &id in &committed {
        let transaction = &transactions[id];
        if transaction
            .own_reads
            .iter()
            .any(|(_, read, written)| read != written)
        {
            report(&mut found, Anomaly::Internal, vec![id]);
        }
        for (link, value) in &transaction.reads {
            let Some(&(source, last)) = writer.get(value) else {
                report(&mut found, Anomaly::Garbage, vec![id]);
                continue;
            };
            if transactions[source].revision.is_none() {
                report(&mut found, Anomaly::G1a, vec![source, id]);
                continue;
            }
            if !last {
                report(&mut found, Anomaly::G1b, vec![source, id]);
                continue;
            }
            if source == id {
                // A transaction cannot read its own write as an external read.
                report(&mut found, Anomaly::Internal, vec![id]);
                continue;
            }
            dependencies[source].insert(id);
            let order = &versions[link];
            let position = order.iter().position(|w| *w == source).unwrap();
            if let Some(&next) = order.get(position + 1) {
                if next != id {
                    anti.push((id, next));
                }
            }
        }
    }
    // Real time: a transaction that committed before another began precedes it.
    for &earlier in &committed {
        for &later in &committed {
            if transactions[earlier].ended < transactions[later].began {
                dependencies[earlier].insert(later);
            }
        }
    }
    // A cycle without anti-dependencies.
    if let Some(cycle) = cycle_in(&dependencies) {
        report(&mut found, Anomaly::G1c, cycle);
    }
    // One anti-dependency closed by a path of the other edges.
    let mut reach: BTreeMap<usize, Vec<bool>> = BTreeMap::new();
    for &(from, to) in &anti {
        let reachable = reach
            .entry(to)
            .or_insert_with(|| reachable_from(&dependencies, to));
        if reachable[from] {
            report(&mut found, Anomaly::GSingle, vec![from, to]);
        }
    }
    // Any other cycle has two or more anti-dependencies.
    if !found.contains_key(&Anomaly::GSingle) && !found.contains_key(&Anomaly::G1c) {
        let mut all = dependencies.clone();
        for &(from, to) in &anti {
            all[from].insert(to);
        }
        if let Some(cycle) = cycle_in(&all) {
            report(&mut found, Anomaly::G2, cycle);
        }
    }
    found
}

fn reachable_from(graph: &[BTreeSet<usize>], start: usize) -> Vec<bool> {
    let mut seen = vec![false; graph.len()];
    let mut stack = vec![start];
    while let Some(node) = stack.pop() {
        for &next in &graph[node] {
            if !seen[next] {
                seen[next] = true;
                stack.push(next);
            }
        }
    }
    seen
}

/// One cycle, as the transactions on it, or `None` when the graph is acyclic.
fn cycle_in(graph: &[BTreeSet<usize>]) -> Option<Vec<usize>> {
    // 0 unvisited, 1 on the current path, 2 done.
    let mut state = vec![0u8; graph.len()];
    for root in 0..graph.len() {
        if state[root] != 0 {
            continue;
        }
        let mut path = vec![root];
        let mut iterators = vec![graph[root].iter()];
        state[root] = 1;
        while let Some(iterator) = iterators.last_mut() {
            match iterator.next() {
                Some(&next) if state[next] == 1 => {
                    let start = path.iter().position(|node| *node == next).unwrap();
                    return Some(path[start..].to_vec());
                }
                Some(&next) if state[next] == 0 => {
                    state[next] = 1;
                    path.push(next);
                    iterators.push(graph[next].iter());
                }
                Some(_) => {}
                None => {
                    state[path.pop().unwrap()] = 2;
                    iterators.pop();
                }
            }
        }
    }
    None
}

/// The anomalies a configuration may show.
fn allowed(options: &EngineOptions) -> &'static [Anomaly] {
    if options.concurrency == ConcurrencyControl::Exclusive
        || options.isolation == IsolationLevel::Serializable
    {
        return &[];
    }
    match options.isolation {
        IsolationLevel::Snapshot => &[Anomaly::G2],
        _ => &[Anomaly::GSingle, Anomaly::G2],
    }
}

fn configurations() -> Vec<EngineOptions> {
    let mut all = Vec::new();
    for concurrency in [
        ConcurrencyControl::Optimistic,
        ConcurrencyControl::Pessimistic,
        ConcurrencyControl::Exclusive,
    ] {
        for (isolation, serializable_validation) in [
            (
                IsolationLevel::ReadCommitted,
                SerializableValidation::Revision,
            ),
            (IsolationLevel::Snapshot, SerializableValidation::Revision),
            (
                IsolationLevel::Serializable,
                SerializableValidation::Revision,
            ),
            (
                IsolationLevel::Serializable,
                SerializableValidation::ReadSet,
            ),
        ] {
            all.push(EngineOptions {
                concurrency,
                isolation,
                serializable_validation,
                ..Default::default()
            });
        }
    }
    all
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn long_random_histories_show_only_the_anomalies_each_isolation_level_allows() {
    let mut total = 0;
    for options in configurations() {
        let context = format!(
            "{:?} {:?} {:?}",
            options.concurrency, options.isolation, options.serializable_validation
        );
        let mut seen: BTreeMap<Anomaly, usize> = BTreeMap::new();
        let mut committed = 0;
        for seed in 1..=SEEDS {
            let history = generate(&options, seed);
            committed += history
                .transactions
                .iter()
                .skip(1)
                .filter(|t| t.revision.is_some())
                .count();
            for (anomaly, example) in check(&history) {
                assert!(
                    allowed(&options).contains(&anomaly),
                    "{context}, seed {seed}: {anomaly:?} through transactions {example:?}: {:#?}",
                    example
                        .iter()
                        .map(|id| &history.transactions[*id])
                        .collect::<Vec<_>>()
                );
                *seen.entry(anomaly).or_default() += 1;
            }
        }
        let attempted = SEEDS as usize * TRANSACTIONS;
        println!("{context}: {committed} of {attempted} transactions committed, histories with anomalies {seen:?}");
        // Refusing everything would pass the check above, so a share of the
        // transactions must commit: a quarter, or a tenth where exclusive
        // admission refuses to begin while another of the four clients is open.
        let share = if options.concurrency == ConcurrencyControl::Exclusive {
            10
        } else {
            4
        };
        assert!(
            committed * share >= attempted,
            "{context}: {committed} committed"
        );
        // The weakest anomaly a level allows must show up. A history with a
        // G-single is not checked for G2 as well, so read committed is held to
        // G-single only.
        if let Some(anomaly) = allowed(&options).first() {
            assert!(
                seen.contains_key(anomaly),
                "{context}: the allowed {anomaly:?} never occurred"
            );
        }
        total += attempted;
    }
    assert_eq!(total, 12 * SEEDS as usize * TRANSACTIONS);
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn the_same_seed_replays_the_same_history() {
    let options = EngineOptions::default();
    let first = generate(&options, 7);
    let second = generate(&options, 7);
    assert_eq!(first.transactions, second.transactions);
    assert_eq!(first.final_state, second.final_state);
    assert_ne!(first.transactions, generate(&options, 8).transactions);
    let mut random = Random(1);
    // splitmix64's published first output for seed 1, shared by every port.
    assert_eq!(random.next(), 0x910A_2DEC_8902_5CC1);
}

fn committed(revision: u64, reads: &[(u32, u32)], writes: &[(u32, u32)]) -> Transaction {
    Transaction {
        reads: reads.to_vec(),
        writes: writes.to_vec(),
        revision: Some(revision),
        began: revision as usize * 10,
        ended: revision as usize * 10 + 1,
        ..Default::default()
    }
}

fn overlapping(mut transaction: Transaction) -> Transaction {
    transaction.began = 0;
    transaction.ended = 1000;
    transaction
}

fn history(transactions: Vec<Transaction>, final_state: &[(u32, u32)]) -> History {
    let mut all = vec![committed(0, &[], &[(1, 1), (2, 2)])];
    all.extend(transactions);
    History {
        transactions: all,
        final_state: final_state.iter().copied().collect(),
    }
}

fn kinds(history: &History) -> Vec<Anomaly> {
    check(history).into_keys().collect()
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn the_checker_names_each_anomaly() {
    // Serial: T1 writes 1, T2 reads it and writes 2.
    let serial = history(
        vec![
            committed(1, &[(1, 1)], &[(1, 1000)]),
            committed(2, &[(1, 1000)], &[(2, 1001)]),
        ],
        &[(1, 1000), (2, 1001)],
    );
    assert_eq!(kinds(&serial), vec![]);
    // Write skew: both read both links, each writes one.
    let write_skew = history(
        vec![
            overlapping(committed(1, &[(1, 1), (2, 2)], &[(1, 1000)])),
            overlapping(committed(2, &[(1, 1), (2, 2)], &[(2, 1001)])),
        ],
        &[(1, 1000), (2, 1001)],
    );
    assert_eq!(kinds(&write_skew), vec![Anomaly::G2]);
    // Lost update: both read link 1, both write it.
    let lost_update = history(
        vec![
            overlapping(committed(1, &[(1, 1)], &[(1, 1000)])),
            overlapping(committed(2, &[(1, 1)], &[(1, 1001)])),
        ],
        &[(1, 1001), (2, 2)],
    );
    assert_eq!(kinds(&lost_update), vec![Anomaly::GSingle]);
    // Read skew: T2 sees link 1 before T1 and link 2 after it.
    let read_skew = history(
        vec![
            overlapping(committed(1, &[], &[(1, 1000), (2, 1001)])),
            overlapping(committed(2, &[(1, 1), (2, 1001)], &[])),
        ],
        &[(1, 1000), (2, 1001)],
    );
    assert_eq!(kinds(&read_skew), vec![Anomaly::GSingle]);
    // A stale read: T2 began after T1 committed but read the older value.
    let stale = history(
        vec![
            committed(1, &[], &[(1, 1000)]),
            committed(2, &[(1, 1)], &[]),
        ],
        &[(1, 1000), (2, 2)],
    );
    assert_eq!(kinds(&stale), vec![Anomaly::GSingle]);
    // Circular information flow: each reads the other's write.
    let circular = history(
        vec![
            overlapping(committed(1, &[(2, 1001)], &[(1, 1000)])),
            overlapping(committed(2, &[(1, 1000)], &[(2, 1001)])),
        ],
        &[(1, 1000), (2, 1001)],
    );
    assert_eq!(kinds(&circular), vec![Anomaly::G1c]);
    // An aborted write that was read, an intermediate write that was read,
    // and a value nobody wrote.
    let mut aborted = committed(1, &[], &[(1, 1000)]);
    aborted.revision = None;
    let dirty = history(
        vec![
            aborted,
            committed(2, &[], &[(2, 1001), (2, 1002)]),
            committed(3, &[(1, 1000), (2, 1001), (1, 5)], &[]),
        ],
        &[(1, 1), (2, 1002)],
    );
    assert_eq!(
        kinds(&dirty),
        vec![Anomaly::Garbage, Anomaly::G1a, Anomaly::G1b]
    );
    // The final state lost T1's committed write; T1 read its own write wrong.
    let mut own = committed(1, &[], &[(1, 1000), (1, 1001)]);
    own.own_reads = vec![(1, 1001, 1001), (1, 1000, 1001)];
    let lost = history(vec![own], &[(1, 1), (2, 2)]);
    assert_eq!(kinds(&lost), vec![Anomaly::Internal, Anomaly::LostWrite]);
}
