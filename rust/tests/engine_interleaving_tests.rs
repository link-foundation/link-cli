//! Every interleaving of two small transactions, checked for serializability.
//!
//! PostgreSQL's isolation tester runs "all possible interleavings of the steps
//! from each session" when a spec lists no permutation, and Jepsen's Elle
//! decides from client observations alone whether a history is serializable.
//! This test does both on a small scale: it runs each workload in every
//! interleaving, under every concurrency control and isolation level, and
//! checks the committed transactions against every serial order. Anomalies an
//! isolation level allows must show up in some interleaving, so the table
//! below is checked in both directions.
use link_cli::engine::{
    ConcurrencyControl, Engine, EngineOptions, EngineTransaction, IsolationLevel, MemoryLog,
    SerializableValidation,
};
use link_cli::LinksStorage;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug)]
enum Step {
    /// Read one link's target.
    Read(u32),
    /// Read and reserve it (`lock_for_update`).
    Lock(u32),
    /// Read the sum of every link's target.
    Scan,
    /// Write one plus the sum of everything this transaction read so far.
    Write(u32),
    Commit,
}
use Step::*;

const INITIAL: [(u32, u32); 2] = [(1, 3), (2, 4)];

struct Workload {
    name: &'static str,
    transactions: [&'static [Step]; 2],
}
const LOST_UPDATE: Workload = Workload {
    name: "lost update",
    transactions: [&[Read(1), Write(1), Commit], &[Read(1), Write(1), Commit]],
};
const READ_SKEW: Workload = Workload {
    name: "read skew",
    transactions: [&[Read(1), Read(2), Commit], &[Write(1), Write(2), Commit]],
};
const WRITE_SKEW: Workload = Workload {
    name: "write skew",
    transactions: [
        &[Read(1), Read(2), Write(1), Commit],
        &[Read(1), Read(2), Write(2), Commit],
    ],
};
const SCAN_SKEW: Workload = Workload {
    name: "write skew through a scan",
    transactions: [&[Scan, Write(1), Commit], &[Scan, Write(2), Commit]],
};
const LOCKED_WRITE_SKEW: Workload = Workload {
    name: "write skew with locked reads",
    transactions: [
        &[Lock(1), Lock(2), Write(1), Commit],
        &[Lock(1), Lock(2), Write(2), Commit],
    ],
};

const DISJOINT: Workload = Workload {
    name: "disjoint read and write",
    transactions: [&[Read(1), Write(1), Commit], &[Read(2), Write(2), Commit]],
};

type Session = EngineTransaction<u32, MemoryLog>;

/// What one transaction observed in one run.
#[derive(Default)]
struct Run {
    session: Option<Session>,
    reads: Vec<u32>,
    aborted: bool,
    committed: bool,
}

fn execute(
    db: &Engine<u32, MemoryLog>,
    run: &mut Run,
    step: Step,
) -> Result<(), link_cli::LinkError> {
    if run.session.is_none() {
        run.session = Some(db.begin()?);
    }
    let session = run.session.as_mut().unwrap();
    match step {
        Read(id) => run.reads.push(session.get_link(id).unwrap().target),
        Lock(id) => run.reads.push(session.lock_for_update(id)?.unwrap().target),
        Scan => run
            .reads
            .push(session.all_links().iter().map(|link| link.target).sum()),
        Write(id) => {
            let value = 1 + run.reads.iter().sum::<u32>();
            session.update_link(id, id, value)?;
        }
        Commit => {
            session.commit()?;
            run.committed = true;
        }
    }
    Ok(())
}

/// Replays transactions one after another from the initial state: the reads
/// each one would make and the final state.
fn serial(order: &[&[Step]]) -> (Vec<Vec<u32>>, BTreeMap<u32, u32>) {
    let mut state = BTreeMap::from(INITIAL);
    let reads = order
        .iter()
        .map(|steps| {
            let mut reads = Vec::new();
            for step in *steps {
                match *step {
                    Read(id) | Lock(id) => reads.push(state[&id]),
                    Scan => reads.push(state.values().sum()),
                    Write(id) => {
                        state.insert(id, 1 + reads.iter().sum::<u32>());
                    }
                    Commit => {}
                }
            }
            reads
        })
        .collect();
    (reads, state)
}

/// Every order of the steps of two transactions that keeps each one's own order.
fn interleavings(first: usize, second: usize) -> Vec<Vec<usize>> {
    if first == 0 || second == 0 {
        return vec![[0]
            .repeat(first)
            .into_iter()
            .chain([1].repeat(second))
            .collect()];
    }
    let mut all = Vec::new();
    for (who, rest) in [
        (0, interleavings(first - 1, second)),
        (1, interleavings(first, second - 1)),
    ] {
        for tail in rest {
            all.push(std::iter::once(who).chain(tail).collect());
        }
    }
    all
}

struct Outcome {
    serializable: bool,
    both_committed: bool,
}

fn run(options: &EngineOptions, workload: &Workload, schedule: &[usize]) -> Outcome {
    let db = Engine::open(MemoryLog::default(), options.clone()).unwrap();
    let mut seed = db.begin().unwrap();
    for (id, target) in INITIAL {
        seed.ensure_link_created(id).unwrap();
        seed.update_link(id, id, target).unwrap();
    }
    seed.commit().unwrap();
    let mut runs = [Run::default(), Run::default()];
    let mut next = [0, 0];
    for &who in schedule {
        let step = workload.transactions[who][next[who]];
        next[who] += 1;
        let run = &mut runs[who];
        if !run.aborted && execute(&db, run, step).is_err() {
            // A refused step ends the transaction, as a client would roll back.
            run.aborted = true;
            run.session.take();
        }
    }
    for run in &mut runs {
        run.session.take();
    }
    let committed: Vec<usize> = (0..2).filter(|who| runs[*who].committed).collect();
    let actual: BTreeMap<u32, u32> = db
        .snapshot()
        .unwrap()
        .links()
        .iter()
        .map(|link| (link.index, link.target))
        .collect();
    let orders: Vec<Vec<usize>> = match committed[..] {
        [a, b] => vec![vec![a, b], vec![b, a]],
        _ => vec![committed.clone()],
    };
    let serializable = orders.iter().any(|order| {
        let steps: Vec<&[Step]> = order
            .iter()
            .map(|who| workload.transactions[*who])
            .collect();
        let (reads, state) = serial(&steps);
        state == actual
            && order
                .iter()
                .zip(reads)
                .all(|(who, reads)| runs[*who].reads == reads)
    });
    Outcome {
        serializable,
        both_committed: committed.len() == 2,
    }
}

/// Whether the configuration must refuse every non-serializable outcome of the
/// workload. Where it need not, the anomaly must actually occur.
fn prevents(options: &EngineOptions, workload: &Workload) -> bool {
    if options.concurrency == ConcurrencyControl::Exclusive
        || options.isolation == IsolationLevel::Serializable
    {
        return true;
    }
    match workload.name {
        "lost update" | "write skew with locked reads" | "disjoint read and write" => true,
        "read skew" => options.isolation == IsolationLevel::Snapshot,
        _ => false,
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
fn every_interleaving_is_serializable_exactly_where_the_isolation_level_promises() {
    let mut checked = 0;
    for options in configurations() {
        for workload in [
            LOST_UPDATE,
            READ_SKEW,
            WRITE_SKEW,
            SCAN_SKEW,
            LOCKED_WRITE_SKEW,
            DISJOINT,
        ] {
            if matches!(workload.transactions[0][0], Lock(_))
                && options.concurrency != ConcurrencyControl::Pessimistic
            {
                continue;
            }
            let [first, second] = workload.transactions.map(<[Step]>::len);
            let outcomes: Vec<(Vec<usize>, Outcome)> = interleavings(first, second)
                .into_iter()
                .map(|schedule| {
                    let outcome = run(&options, &workload, &schedule);
                    (schedule, outcome)
                })
                .collect();
            let context = format!(
                "{} under {:?} {:?} {:?}",
                workload.name,
                options.concurrency,
                options.isolation,
                options.serializable_validation
            );
            assert!(
                outcomes.iter().any(|(_, outcome)| outcome.both_committed),
                "{context}: no interleaving committed both transactions"
            );
            let anomaly = outcomes
                .iter()
                .find(|(_, outcome)| !outcome.serializable)
                .map(|(schedule, _)| schedule);
            if prevents(&options, &workload) {
                assert_eq!(anomaly, None, "{context}: a non-serializable interleaving");
            } else {
                assert!(
                    anomaly.is_some(),
                    "{context}: the allowed anomaly never occurred"
                );
            }
            let anomalies = outcomes.iter().filter(|(_, o)| !o.serializable).count();
            let both = outcomes.iter().filter(|(_, o)| o.both_committed).count();
            println!(
                "{context}: {} interleavings, {both} committed both, {anomalies} not serializable",
                outcomes.len()
            );
            if workload.name == DISJOINT.name
                && options.concurrency != ConcurrencyControl::Exclusive
                && (options.isolation != IsolationLevel::Serializable
                    || options.serializable_validation == SerializableValidation::ReadSet)
            {
                assert_eq!(both, outcomes.len(), "{context}: refused independent work");
            }
            checked += outcomes.len();
        }
    }
    assert!(checked > 1000, "{checked} interleavings");
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
#[cfg_attr(not(target_arch = "wasm32"), test)]
fn the_checker_recognizes_serial_and_anomalous_histories() {
    assert_eq!(
        interleavings(2, 1),
        vec![vec![0, 0, 1], vec![0, 1, 0], vec![1, 0, 0]]
    );
    assert_eq!(interleavings(4, 4).len(), 70);
    let snapshot = EngineOptions {
        isolation: IsolationLevel::Snapshot,
        ..Default::default()
    };
    // One transaction after the other is always serializable.
    let one_after_the_other = run(&snapshot, &WRITE_SKEW, &[0, 0, 0, 0, 1, 1, 1, 1]);
    assert!(one_after_the_other.serializable && one_after_the_other.both_committed);
    // Both read before either writes: each takes one doctor off call.
    let overlapping = run(&snapshot, &WRITE_SKEW, &[0, 0, 1, 1, 0, 0, 1, 1]);
    assert!(!overlapping.serializable && overlapping.both_committed);
}
