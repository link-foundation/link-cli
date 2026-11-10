import assert from 'node:assert/strict';
import { WasmEngine } from '../../src/index.js';

/**
 * Long random histories on the compiled engine, checked for isolation
 * anomalies by dependency cycles, as Jepsen's Elle does with Adya's model. A
 * seeded scheduler runs four clients against one engine, one step at a time;
 * every write stores a unique value and commit revisions give each link's
 * version order. Mirrors rust/tests/engine_history_tests.rs and the C#
 * EngineHistoryTests: the same generator, seeds, checker and pinned counts.
 */
const CLIENTS = 4n;
const KEYS = 3n;
const TRANSACTIONS = 60;
const SEEDS = 20n;
const FIRST_WRITE = 1000n;
const MASK = (1n << 64n) - 1n;

/** splitmix64: the same sequence in Rust, C# and JS. */
export function random(seed) {
  let state = seed;
  const next = () => {
    state = (state + 0x9e3779b97f4a7c15n) & MASK;
    let z = state;
    z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & MASK;
    z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & MASK;
    return z ^ (z >> 31n);
  };
  return { next, below: (bound) => next() % bound };
}

/** Up to four operations, then commit or (rarely) rollback. */
function plan(rng, pessimistic) {
  const length = 1n + rng.below(4n);
  const ops = [];
  for (let i = 0n; i < length; i++) {
    const kind = rng.below(10n);
    const link = 1n + rng.below(KEYS);
    if (kind <= 3n) ops.push({ read: link });
    else if (kind <= 6n || kind === 9n) ops.push({ write: link });
    else if (kind === 7n) ops.push({ scan: true });
    else ops.push(pessimistic ? { lock: link } : { read: link });
  }
  ops.push(rng.below(20n) === 0n ? { rollback: true } : { commit: true });
  return ops;
}

function observe(record, link, target) {
  const own = record.writes.findLast(([written]) => written === link);
  if (own) record.ownReads.push([link, target, own[1]]);
  else record.reads.push([link, target]);
}

const transaction = (began = 0) => ({
  reads: [],
  writes: [],
  ownReads: [],
  revision: undefined,
  began,
  ended: 0,
});

async function step(engine, client, op, counter) {
  client.session ??= engine.begin();
  const { session, record } = client;
  if (op.read) observe(record, op.read, (await session.each([op.read]))[0].target);
  else if (op.lock) observe(record, op.lock, (await session.lockForUpdate(op.lock)).target);
  else if (op.scan)
    for (const link of await session.each()) observe(record, link.index, link.target);
  else if (op.write) {
    const value = ++counter.value;
    await session.update(op.write, op.write, value);
    record.writes.push([op.write, value]);
  } else if (op.commit) record.revision = await session.commit();
  else {
    await session.close();
    client.session = undefined;
  }
}

export async function generate(binding, options, seed) {
  const engine = new WasmEngine(binding, options);
  try {
    const initial = transaction();
    const setup = engine.begin();
    for (let link = 1n; link <= KEYS; link++) {
      assert.equal((await setup.create(0n, 0n)).index, link);
      await setup.update(link, link, link);
      initial.writes.push([link, link]);
    }
    initial.revision = await setup.commit();
    await setup.close();
    const transactions = [initial];
    const rng = random(seed);
    const pessimistic = options.concurrency === 'pessimistic';
    const clients = Array(Number(CLIENTS)).fill(undefined);
    let started = 0;
    let finished = 0;
    let now = 0;
    const counter = { value: FIRST_WRITE - 1n };
    while (finished < TRANSACTIONS) {
      now++;
      const slot = Number(rng.below(CLIENTS));
      if (clients[slot] === undefined) {
        if (started === TRANSACTIONS) continue;
        started++;
        clients[slot] = {
          ops: plan(rng, pessimistic),
          next: 0,
          record: transaction(now),
        };
      }
      const client = clients[slot];
      const op = client.ops[client.next++];
      let failed = false;
      try {
        await step(engine, client, op, counter);
      } catch (error) {
        if (error instanceof TypeError || error instanceof assert.AssertionError) throw error;
        // A refused step ends the transaction, as a client would roll back.
        failed = true;
      }
      if (failed || client.next === client.ops.length) {
        await client.session?.close();
        if (failed) client.record.revision = undefined;
        client.record.ended = now;
        transactions.push(client.record);
        clients[slot] = undefined;
        finished++;
      }
    }
    const finalState = new Map(engine.snapshot().rows.map((row) => [row.index, row.target]));
    return { transactions, finalState };
  } finally {
    engine.close();
  }
}

function reachableFrom(graph, start) {
  const seen = Array(graph.length).fill(false);
  const stack = [start];
  while (stack.length > 0)
    for (const next of graph[stack.pop()])
      if (!seen[next]) {
        seen[next] = true;
        stack.push(next);
      }
  return seen;
}

/** One cycle, as the transactions on it, or undefined when acyclic. */
function cycleIn(graph) {
  const state = Array(graph.length).fill(0); // 0 unvisited, 1 on the path, 2 done
  for (let root = 0; root < graph.length; root++) {
    if (state[root] !== 0) continue;
    const path = [root];
    const iterators = [[...graph[root]].sort((a, b) => a - b).values()];
    state[root] = 1;
    while (iterators.length > 0) {
      const { value: next, done } = iterators.at(-1).next();
      if (done) {
        state[path.pop()] = 2;
        iterators.pop();
      } else if (state[next] === 1) return path.slice(path.indexOf(next));
      else if (state[next] === 0) {
        state[next] = 1;
        path.push(next);
        iterators.push([...graph[next]].sort((a, b) => a - b).values());
      }
    }
  }
  return undefined;
}

export const ANOMALIES = ['Garbage', 'G1a', 'G1b', 'Internal', 'LostWrite', 'G1c', 'GSingle', 'G2'];

/** Every anomaly in the history, each with one example: the transactions involved. */
export function check({ transactions, finalState }) {
  const found = new Map();
  const report = (anomaly, example) => {
    if (!found.has(anomaly)) found.set(anomaly, example);
  };
  const writer = new Map();
  transactions.forEach(({ writes }, id) =>
    writes.forEach(([link, value], at) =>
      writer.set(value, {
        id,
        last: !writes.slice(at + 1).some(([later]) => later === link),
      }),
    ),
  );
  const committed = transactions.flatMap((t, id) => (t.revision === undefined ? [] : [id]));
  const versions = new Map();
  for (const id of [...committed].sort((a, b) =>
    transactions[a].revision < transactions[b].revision ? -1 : 1,
  ))
    for (const link of new Set(transactions[id].writes.map(([written]) => written))) {
      if (!versions.has(link)) versions.set(link, []);
      versions.get(link).push(id);
    }
  for (const [link, writers] of versions) {
    const expected = transactions[writers.at(-1)].writes.findLast(
      ([written]) => written === link,
    )[1];
    if (finalState.get(link) !== expected) report('LostWrite', writers);
  }
  const dependencies = transactions.map(() => new Set());
  const anti = [];
  for (const writers of versions.values())
    for (let i = 0; i + 1 < writers.length; i++) dependencies[writers[i]].add(writers[i + 1]);
  for (const id of committed) {
    const { reads, ownReads } = transactions[id];
    if (ownReads.some(([, read, written]) => read !== written)) report('Internal', [id]);
    for (const [link, value] of reads) {
      const source = writer.get(value);
      if (source === undefined) report('Garbage', [id]);
      else if (transactions[source.id].revision === undefined) report('G1a', [source.id, id]);
      else if (!source.last) report('G1b', [source.id, id]);
      else if (source.id === id) report('Internal', [id]);
      else {
        dependencies[source.id].add(id);
        const order = versions.get(link);
        const following = order[order.indexOf(source.id) + 1];
        if (following !== undefined && following !== id) anti.push([id, following]);
      }
    }
  }
  // Real time: a transaction that committed before another began precedes it.
  for (const earlier of committed)
    for (const later of committed)
      if (transactions[earlier].ended < transactions[later].began) dependencies[earlier].add(later);
  const cycle = cycleIn(dependencies);
  if (cycle) report('G1c', cycle);
  const reach = new Map();
  for (const [from, to] of anti) {
    if (!reach.has(to)) reach.set(to, reachableFrom(dependencies, to));
    if (reach.get(to)[from]) report('GSingle', [from, to]);
  }
  if (!found.has('GSingle') && !found.has('G1c')) {
    const all = dependencies.map((edges) => new Set(edges));
    for (const [from, to] of anti) all[from].add(to);
    const longer = cycleIn(all);
    if (longer) report('G2', longer);
  }
  return new Map(ANOMALIES.filter((anomaly) => found.has(anomaly)).map((a) => [a, found.get(a)]));
}

function allowed({ concurrency, isolation }) {
  if (concurrency === 'exclusive' || isolation === 'serializable') return [];
  return isolation === 'snapshot' ? ['G2'] : ['GSingle', 'G2'];
}

/** Committed transactions and histories with G-single and with G2, pinned to
 * the numbers the Rust and C# ports report for the same seeds. */
const EXPECTED = [
  [847, 19, 0],
  [847, 0, 20],
  [573, 0, 0],
  [662, 0, 0],
  [635, 17, 2],
  [635, 0, 19],
  [453, 0, 0],
  [515, 0, 0],
  [148, 0, 0],
  [148, 0, 0],
  [148, 0, 0],
  [148, 0, 0],
];

function committedAt(revision, reads, writes, overlapping = false) {
  return {
    reads,
    writes,
    ownReads: [],
    revision: BigInt(revision),
    began: overlapping ? 0 : revision * 10,
    ended: overlapping ? 1000 : revision * 10 + 1,
  };
}

function kinds(transactions, final) {
  return [
    ...check({
      transactions: [
        committedAt(
          0,
          [],
          [
            [1n, 1n],
            [2n, 2n],
          ],
        ),
        ...transactions,
      ],
      finalState: new Map(final),
    }).keys(),
  ];
}

function checkerSelfTest() {
  const r = (link, value) => [BigInt(link), BigInt(value)];
  const both = [r(1, 1000), r(2, 1001)];
  assert.deepEqual(
    kinds(
      [committedAt(1, [r(1, 1)], [r(1, 1000)]), committedAt(2, [r(1, 1000)], [r(2, 1001)])],
      both,
    ),
    [],
  );
  const initial = [r(1, 1), r(2, 2)];
  // Write skew.
  assert.deepEqual(
    kinds(
      [committedAt(1, initial, [r(1, 1000)], true), committedAt(2, initial, [r(2, 1001)], true)],
      both,
    ),
    ['G2'],
  );
  // Lost update.
  assert.deepEqual(
    kinds(
      [
        committedAt(1, [r(1, 1)], [r(1, 1000)], true),
        committedAt(2, [r(1, 1)], [r(1, 1001)], true),
      ],
      [r(1, 1001), r(2, 2)],
    ),
    ['GSingle'],
  );
  // Read skew.
  assert.deepEqual(
    kinds([committedAt(1, [], both, true), committedAt(2, [r(1, 1), r(2, 1001)], [], true)], both),
    ['GSingle'],
  );
  // A stale read after the writer committed.
  assert.deepEqual(
    kinds([committedAt(1, [], [r(1, 1000)]), committedAt(2, [r(1, 1)], [])], [r(1, 1000), r(2, 2)]),
    ['GSingle'],
  );
  // Circular information flow.
  assert.deepEqual(
    kinds(
      [
        committedAt(1, [r(2, 1001)], [r(1, 1000)], true),
        committedAt(2, [r(1, 1000)], [r(2, 1001)], true),
      ],
      both,
    ),
    ['G1c'],
  );
  // Aborted, intermediate and unknown values read.
  const aborted = committedAt(1, [], [r(1, 1000)]);
  aborted.revision = undefined;
  assert.deepEqual(
    kinds(
      [
        aborted,
        committedAt(2, [], [r(2, 1001), r(2, 1002)]),
        committedAt(3, [r(1, 1000), r(2, 1001), r(1, 5)], []),
      ],
      [r(1, 1), r(2, 1002)],
    ),
    ['Garbage', 'G1a', 'G1b'],
  );
  // A lost committed write and a read that missed the transaction's own write.
  const own = committedAt(1, [], [r(1, 1000), r(1, 1001)]);
  own.ownReads = [
    [1n, 1001n, 1001n],
    [1n, 1000n, 1001n],
  ];
  assert.deepEqual(kinds([own], initial), ['Internal', 'LostWrite']);
}

export async function historyConformance(binding) {
  checkerSelfTest();
  // splitmix64's first output for seed 1, shared by every port.
  assert.equal(random(1n).next(), 0x910a2dec89025cc1n);
  const replay = async (seed) =>
    JSON.stringify(await generate(binding, {}, seed), (_, value) =>
      typeof value === 'bigint' ? `${value}` : value instanceof Map ? [...value] : value,
    );
  assert.equal(await replay(7n), await replay(7n));
  assert.notEqual(await replay(7n), await replay(8n));
  let index = 0;
  for (const concurrency of ['optimistic', 'pessimistic', 'exclusive'])
    for (const [isolation, serializableValidation] of [
      ['read-committed', 'revision'],
      ['snapshot', 'revision'],
      ['serializable', 'revision'],
      ['serializable', 'read-set'],
    ]) {
      const options = { concurrency, isolation, serializableValidation };
      const context = `${concurrency} ${isolation} ${serializableValidation}`;
      const seen = new Map();
      let committed = 0;
      for (let seed = 1n; seed <= SEEDS; seed++) {
        const history = await generate(binding, options, seed);
        committed += history.transactions.slice(1).filter((t) => t.revision !== undefined).length;
        for (const [anomaly, example] of check(history)) {
          assert.ok(
            allowed(options).includes(anomaly),
            `${context}, seed ${seed}: ${anomaly} through transactions ${example}`,
          );
          seen.set(anomaly, (seen.get(anomaly) ?? 0) + 1);
        }
      }
      const attempted = Number(SEEDS) * TRANSACTIONS;
      // Refusing everything would pass the check above, so a share must
      // commit: a tenth under exclusive admission, a quarter otherwise.
      const share = concurrency === 'exclusive' ? 10 : 4;
      assert.ok(committed * share >= attempted, `${context}: ${committed} committed`);
      const [weakest] = allowed(options);
      if (weakest)
        assert.ok(seen.has(weakest), `${context}: the allowed ${weakest} never occurred`);
      assert.deepEqual(
        [committed, seen.get('GSingle') ?? 0, seen.get('G2') ?? 0],
        EXPECTED[index++],
        `${context}: differs from the Rust and C# ports`,
      );
    }
}
