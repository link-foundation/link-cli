//! Elle-style history checking, shared by `engine_history_tests.rs` (one
//! engine) and `engine_simulation_tests.rs` (cluster members over a faulty
//! simulated network). See `engine_history_tests.rs` for the edges and the
//! anomalies.
use std::collections::{BTreeMap, BTreeSet};

/// What a transaction did, as a client saw it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transaction {
    /// `(link, value)` in the order the client read them.
    pub reads: Vec<(u32, u32)>,
    /// `(link, value)` in the order the client wrote them.
    pub writes: Vec<(u32, u32)>,
    /// Reads that follow the transaction's own write to that link:
    /// `(link, value read, value last written)`.
    pub own_reads: Vec<(u32, u32, u32)>,
    /// The commit revision; `None` for aborted or rolled-back transactions.
    pub revision: Option<u64>,
    /// Scheduler steps: when the transaction began and when it ended.
    pub began: usize,
    pub ended: usize,
}

#[derive(Debug, Default)]
pub struct History {
    /// Transaction 0 installs the initial values.
    pub transactions: Vec<Transaction>,
    /// The final committed value of each link.
    pub final_state: BTreeMap<u32, u32>,
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
