import assert from 'node:assert/strict';
import { WasmEngine } from '../../src/index.js';

/**
 * Every interleaving of two small transactions on the compiled engine, checked
 * for serializability, as in PostgreSQL's isolation tester (all interleavings
 * of the sessions' steps) and Jepsen's Elle (judging a history from client
 * observations). Mirrors rust/tests/engine_interleaving_tests.rs.
 */
const read = (id) => ({ read: id });
const lock = (id) => ({ lock: id });
const scan = { scan: true };
const write = (id) => ({ write: id });
const commit = { commit: true };
const INITIAL = [
  [1n, 3n],
  [2n, 4n],
];

const WORKLOADS = [
  {
    name: 'lost update',
    transactions: [
      [read(1n), write(1n), commit],
      [read(1n), write(1n), commit],
    ],
  },
  {
    name: 'read skew',
    transactions: [
      [read(1n), read(2n), commit],
      [write(1n), write(2n), commit],
    ],
  },
  {
    name: 'write skew',
    transactions: [
      [read(1n), read(2n), write(1n), commit],
      [read(1n), read(2n), write(2n), commit],
    ],
  },
  {
    name: 'write skew through a scan',
    transactions: [
      [scan, write(1n), commit],
      [scan, write(2n), commit],
    ],
  },
  {
    name: 'write skew with locked reads',
    transactions: [
      [lock(1n), lock(2n), write(1n), commit],
      [lock(1n), lock(2n), write(2n), commit],
    ],
  },
  {
    name: 'disjoint read and write',
    transactions: [
      [read(1n), write(1n), commit],
      [read(2n), write(2n), commit],
    ],
  },
];
const [, , WRITE_SKEW, , LOCKED_WRITE_SKEW, DISJOINT] = WORKLOADS;

const sum = (values) => values.reduce((total, value) => total + value, 0n);

/** Every order of two transactions' steps that keeps each one's own order. */
export function interleavings(first, second) {
  if (first === 0 || second === 0) return [[...Array(first).fill(0), ...Array(second).fill(1)]];
  return [
    ...interleavings(first - 1, second).map((tail) => [0, ...tail]),
    ...interleavings(first, second - 1).map((tail) => [1, ...tail]),
  ];
}

/** The reads each transaction makes, and the final state, run one after another. */
function serial(order) {
  const state = new Map(INITIAL);
  const reads = order.map((steps) => {
    const seen = [];
    for (const step of steps) {
      if (step.read ?? step.lock) seen.push(state.get(step.read ?? step.lock));
      else if (step.scan) seen.push(sum([...state.values()]));
      else if (step.write) state.set(step.write, 1n + sum(seen));
    }
    return seen;
  });
  return { reads, state };
}

async function execute(engine, run, step) {
  run.session ??= engine.begin();
  const session = run.session;
  if (step.read) run.reads.push((await session.each([step.read]))[0].target);
  else if (step.lock) run.reads.push((await session.lockForUpdate(step.lock)).target);
  else if (step.scan) run.reads.push(sum((await session.each()).map((link) => link.target)));
  else if (step.write) await session.update(step.write, step.write, 1n + sum(run.reads));
  else {
    await session.commit();
    run.committed = true;
  }
}

export async function runSchedule(binding, options, workload, schedule) {
  const engine = new WasmEngine(binding, options);
  const runs = [
    { reads: [], aborted: false, committed: false },
    { reads: [], aborted: false, committed: false },
  ];
  try {
    const seed = engine.begin();
    for (const [index, target] of INITIAL) {
      assert.equal((await seed.create(0n, 0n)).index, index);
      await seed.update(index, index, target);
    }
    await seed.commit();
    await seed.close();
    const next = [0, 0];
    for (const who of schedule) {
      const step = workload.transactions[who][next[who]++];
      const run = runs[who];
      if (run.aborted) continue;
      try {
        await execute(engine, run, step);
      } catch (error) {
        if (error instanceof TypeError || error instanceof assert.AssertionError) throw error;
        // A refused step ends the transaction, as a client would roll back.
        run.aborted = true;
        await run.session?.close();
        run.session = undefined;
      }
    }
    for (const run of runs) await run.session?.close();
    const committed = [0, 1].filter((who) => runs[who].committed);
    const actual = new Map(engine.snapshot().rows.map((row) => [row.index, row.target]));
    const orders = committed.length === 2 ? [committed, [...committed].reverse()] : [committed];
    const serializable = orders.some((order) => {
      const { reads, state } = serial(order.map((who) => workload.transactions[who]));
      return (
        state.size === actual.size &&
        [...state].every(([index, target]) => actual.get(index) === target) &&
        order.every(
          (who, position) =>
            runs[who].reads.length === reads[position].length &&
            runs[who].reads.every((value, at) => value === reads[position][at]),
        )
      );
    });
    return { serializable, bothCommitted: committed.length === 2 };
  } finally {
    engine.close();
  }
}

function prevents(options, workload) {
  if (options.concurrency === 'exclusive' || options.isolation === 'serializable') return true;
  if (['lost update', 'write skew with locked reads', 'disjoint read and write'].includes(workload.name))
    return true;
  return workload.name === 'read skew' && options.isolation === 'snapshot';
}

export async function interleavingConformance(binding) {
  assert.deepEqual(interleavings(2, 1), [
    [0, 0, 1],
    [0, 1, 0],
    [1, 0, 0],
  ]);
  const snapshot = { isolation: 'snapshot' };
  assert.deepEqual(await runSchedule(binding, snapshot, WRITE_SKEW, [0, 0, 0, 0, 1, 1, 1, 1]), {
    serializable: true,
    bothCommitted: true,
  });
  // Both read before either writes: each takes one doctor off call.
  assert.deepEqual(await runSchedule(binding, snapshot, WRITE_SKEW, [0, 0, 1, 1, 0, 0, 1, 1]), {
    serializable: false,
    bothCommitted: true,
  });
  let checked = 0;
  for (const concurrency of ['optimistic', 'pessimistic', 'exclusive'])
    for (const [isolation, serializableValidation] of [
      ['read-committed', 'revision'],
      ['snapshot', 'revision'],
      ['serializable', 'revision'],
      ['serializable', 'read-set'],
    ]) {
      const options = { concurrency, isolation, serializableValidation };
      for (const workload of WORKLOADS) {
        if (workload === LOCKED_WRITE_SKEW && concurrency !== 'pessimistic') continue;
        const [first, second] = workload.transactions.map((steps) => steps.length);
        const outcomes = [];
        for (const schedule of interleavings(first, second))
          outcomes.push({ schedule, ...(await runSchedule(binding, options, workload, schedule)) });
        const context = `${workload.name} under ${concurrency} ${isolation} ${serializableValidation}`;
        assert.ok(
          outcomes.some((outcome) => outcome.bothCommitted),
          `${context}: no interleaving committed both`,
        );
        const anomaly = outcomes.find((outcome) => !outcome.serializable)?.schedule;
        if (prevents(options, workload))
          assert.equal(anomaly, undefined, `${context}: a non-serializable interleaving`);
        else assert.ok(anomaly, `${context}: the allowed anomaly never occurred`);
        if (
          workload === DISJOINT &&
          concurrency !== 'exclusive' &&
          (isolation !== 'serializable' || serializableValidation === 'read-set')
        )
          assert.ok(
            outcomes.every((outcome) => outcome.bothCommitted),
            `${context}: refused independent work`,
          );
        checked += outcomes.length;
      }
    }
  assert.ok(checked > 1000, `${checked} interleavings`);
}
