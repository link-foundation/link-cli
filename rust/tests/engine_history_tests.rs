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
use std::collections::BTreeMap;

#[path = "engine_history/check.rs"]
mod check;
use check::{check, Anomaly, History, Transaction};

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

/// Per configuration, in `configurations()` order: transactions committed and
/// histories with G-single and with G2. The C# and JS/WASM ports pin the same
/// numbers, so each port commits and refuses exactly the same transactions.
const EXPECTED: [(usize, usize, usize); 12] = [
    (847, 19, 0),
    (847, 0, 20),
    (573, 0, 0),
    (662, 0, 0),
    (635, 17, 2),
    (635, 0, 19),
    (453, 0, 0),
    (515, 0, 0),
    (148, 0, 0),
    (148, 0, 0),
    (148, 0, 0),
    (148, 0, 0),
];

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn long_random_histories_show_only_the_anomalies_each_isolation_level_allows() {
    let mut total = 0;
    for (options, expected) in configurations().into_iter().zip(EXPECTED) {
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
        let count = |anomaly| seen.get(&anomaly).copied().unwrap_or(0);
        assert_eq!(
            (committed, count(Anomaly::GSingle), count(Anomaly::G2)),
            expected,
            "{context}: differs from the other ports"
        );
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
