// Deterministic simulation of an etcd-like Raft group with its own timers,
// skewed and jumping clocks, and an unreliable operating system and network.
//
// A port of rust/tests/engine_raft_simulation_tests.rs. engine-simulation
// checks `ClusterEngine` members against one idealised atomic store. Here the
// store itself is three replicas of the draft `RaftNode` (src/raft.js), the
// algorithm etcd runs, and the simulation owns everything etcd would get from
// the operating system:
//
// * Clocks and timers. Each replica reads its own clock, which runs at its own
//   rate (0.5x to 2x real time), jumps forward and backward, and changes rate
//   during the run. Election timeouts, heartbeats, the CheckQuorum interval
//   and the client deadlines are all measured on these skewed clocks.
// * Processes. Replicas crash and restart, keeping only their hard state and
//   log, as a process that fsyncs them would.
// * Network. Messages are dropped, delayed and reordered. A replica can be
//   isolated, deaf (it sends but does not receive) or mute (it receives but
//   does not send).
//
// Clients on every member run random transactions through `ClusterEngine`,
// whose store calls propose a command to their own replica and wait, on that
// replica's skewed clock, for the replica to apply it. The Elle-style checker
// searches every history for anomalies, real-time edges included, and every
// replica must end with the same committed log and state.
//
// Liveness while faults last: with at most one replica faulty at a time,
// faults never stop, yet the longest gap between answers to a probe read
// stays below a bound. Without PreVote and CheckQuorum the same fault
// schedules stall the group far longer. Safety under any faults: with every
// replica allowed to fail at once, the history stays strictly serializable.
//
// Every random draw happens in the order of the Rust test, so the counts are
// pinned to the Rust reference (experiments/issue-138/raft-simulation-expected.txt).
//
// The engine is the Rust core compiled to WebAssembly, so this needs a
// `wasm-pack build --target web` package of rust/wasm: CLINK_WASM, or js/pkg.
// Without one the tests are skipped.
import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import process from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { ClusterEngine } from '../src/cluster.js';
import { RaftConfig, RaftNode, RaftRole } from '../src/raft.js';
import { check, random } from './conformance/history.mjs';

const MASK = (1n << 64n) - 1n;

async function loadBinding() {
  const explicit = process.env.CLINK_WASM;
  const dir = explicit ?? fileURLToPath(new URL('../../pkg', import.meta.url));
  if (!explicit && !existsSync(join(dir, 'clink_wasm_bg.wasm'))) return undefined;
  const binding = await import(pathToFileURL(join(dir, 'clink_wasm.js')).href);
  binding.initSync({ module: readFileSync(join(dir, 'clink_wasm_bg.wasm')) });
  return binding;
}
const binding = await loadBinding();
const skip = binding ? false : 'set CLINK_WASM to a wasm-pack --target web build of rust/wasm';

/** splitmix64, shared with every port, plus a per-mille coin. */
function rng(seed) {
  const source = random(seed & MASK);
  return { ...source, perMille: (chance) => source.below(1000n) < BigInt(chance) };
}
const below = (source, bound) => Number(source.below(BigInt(bound)));
const injected = (what) => new Error(`injected ${what}`);

// ---------------------------------------------------------------------------
// Simulated clocks, processes and network
// ---------------------------------------------------------------------------

const REPLICAS = 3;
/** Client deadline, in the local clock units of the client's replica. */
const DEADLINE = 400;
/** The most global ticks one call may take, whatever the clocks do. */
const CALL_LIMIT = 20_000;
/** Real-time ticks before a probe read is abandoned for a fresh one. */
const PROBE_TIMEOUT = 300;

/** A clock running at `rate` per mille of real (global) time since `since`. */
const local = ({ rate, base, since }, now) => base + Math.floor(((now - since) * rate) / 1000);

const NONE = 'none';
/** Nothing in or out. */
const ISOLATED = 'isolated';
/** Sends, but receives nothing. */
const DEAF = 'deaf';
/** Receives, but sends nothing. */
const MUTE = 'mute';
/** Crashed; restarts when the fault heals. */
const DOWN = 'down';

/** At most one replica is faulty at any time. */
const MINORITY = 'Minority';
/** Any number of replicas fail at once. */
const ANY = 'Any';

const simStats = () => ({
  delivered: 0,
  dropped: 0,
  faults: 0,
  restarts: 0,
  clockJumps: 0,
  leaders: 0,
  maxTerm: 0,
  // Probe reads answered, and the longest real time between two answers.
  probes: 0,
  longestGap: 0,
});

class Sim {
  now = 0;
  faults = Array(REPLICAS).fill(NONE);
  heals = Array(REPLICAS).fill(0);
  chaos = false;
  /** Messages in flight: delivery time -> envelopes in send order. Every
   * message is due after `now`, and each step delivers the messages due now,
   * so the buckets are delivered in (delivery time, send order). */
  queue = new Map();
  nextId = 0;
  leaderTerm = 0;
  stats = simStats();
  /** The transaction a member is committing, and every exchange's caller. */
  caller = 0;
  callers = new Map();
  /** The probe read in flight: `{ node, id, since }`. */
  probe = undefined;
  lastAnswer = 0;
  constructor(seed, mode, preVoteAndCheckQuorum) {
    // Drives faults and clocks only, so a fault schedule does not depend on
    // the traffic and is the same with and without PreVote and CheckQuorum.
    this.schedule = rng(seed * 6151n);
    this.clocks = [];
    this.nodes = [];
    for (let id = 0; id < REPLICAS; id++) {
      const rate = 500 + below(this.schedule, 1501);
      const base = below(this.schedule, 10_000);
      this.clocks.push({ rate, base, since: 0 });
      const config = new RaftConfig(id, REPLICAS);
      config.seed = seed * 31n + BigInt(id);
      config.preVote = preVoteAndCheckQuorum;
      config.checkQuorum = preVoteAndCheckQuorum;
      this.nodes.push(new RaftNode(config, base));
    }
    // Drives the network only.
    this.random = rng(seed * 7919n);
    this.mode = mode;
  }

  local(node) {
    return local(this.clocks[node], this.now);
  }
  canSend(node) {
    return this.faults[node] === NONE || this.faults[node] === DEAF;
  }
  canReceive(node) {
    return this.faults[node] === NONE || this.faults[node] === MUTE;
  }

  flush() {
    for (let node = 0; node < REPLICAS; node++)
      for (const envelope of this.nodes[node].takeMessages()) {
        if (!this.canSend(node) || (this.chaos && this.random.perMille(30))) {
          this.stats.dropped++;
          continue;
        }
        let delay = 1 + below(this.random, 10);
        if (this.chaos && this.random.perMille(50)) delay += below(this.random, 150);
        const at = this.now + delay;
        if (!this.queue.has(at)) this.queue.set(at, []);
        this.queue.get(at).push(envelope);
      }
  }

  heal(node) {
    if (this.faults[node] === DOWN) {
      this.nodes[node].restart(this.local(node));
      this.stats.restarts++;
    }
    this.faults[node] = NONE;
  }

  inject() {
    for (let node = 0; node < REPLICAS; node++)
      if (this.faults[node] !== NONE && this.now >= this.heals[node]) this.heal(node);
    if (!this.schedule.perMille(3)) return;
    const node = below(this.schedule, REPLICAS);
    const kind = below(this.schedule, 7);
    if (kind <= 3) {
      if (this.mode === MINORITY)
        for (let other = 0; other < REPLICAS; other++)
          if (other !== node && this.faults[other] !== NONE) this.heal(other);
      if (this.faults[node] !== NONE) this.heal(node);
      this.faults[node] = [ISOLATED, DEAF, MUTE, DOWN][kind];
      this.heals[node] = this.now + 200 + below(this.schedule, 1800);
      this.stats.faults++;
    } else if (kind <= 5) {
      // NTP steps the clock forward or backward.
      const time = this.local(node);
      const step = below(this.schedule, 400);
      const jumped = kind === 4 ? time + step : Math.max(time - step, 0);
      this.clocks[node] = { rate: this.clocks[node].rate, base: jumped, since: this.now };
      this.stats.clockJumps++;
    } else {
      // The clock drifts at a new rate.
      const time = this.local(node);
      const rate = 500 + below(this.schedule, 1501);
      this.clocks[node] = { rate, base: time, since: this.now };
    }
  }

  step() {
    this.now++;
    if (this.chaos) this.inject();
    const due = this.queue.get(this.now) ?? [];
    this.queue.delete(this.now);
    for (const envelope of due) {
      if (!this.canReceive(envelope.to)) {
        this.stats.dropped++;
        continue;
      }
      this.stats.delivered++;
      this.nodes[envelope.to].step(this.local(envelope.to), envelope);
    }
    for (let node = 0; node < REPLICAS; node++)
      if (this.faults[node] !== DOWN) this.nodes[node].tick(this.local(node));
    for (const node of this.nodes) {
      this.stats.maxTerm = Math.max(this.stats.maxTerm, node.term);
      if (node.role === RaftRole.Leader && node.term > this.leaderTerm) {
        this.leaderTerm = node.term;
        this.stats.leaders++;
      }
    }
    if (this.chaos) this.probeRead();
    this.flush();
  }

  /** Measures availability apart from the clients, whose calls run one at a
   * time: a probe keeps a linearizable read in flight on a healthy replica,
   * and the longest real time between two answers is the longest time the
   * group could not serve a healthy replica. */
  probeRead() {
    if (this.probe !== undefined) {
      const { node, id, since } = this.probe;
      if (this.nodes[node].takeOutcome(id) !== undefined) {
        this.stats.probes++;
        this.stats.longestGap = Math.max(this.stats.longestGap, this.now - this.lastAnswer);
        this.lastAnswer = this.now;
        this.probe = undefined;
      } else if (this.now - since > PROBE_TIMEOUT || this.faults[node] !== NONE) {
        this.nodes[node].forget(id);
        this.probe = undefined;
      }
    }
    if (this.probe !== undefined) return;
    for (let offset = 0; offset < REPLICAS; offset++) {
      const node = (this.now + offset) % REPLICAS;
      if (this.faults[node] === NONE) {
        const id = ++this.nextId;
        if (this.nodes[node].propose(this.local(node), { type: 'read', id, keys: [] }))
          this.probe = { node, id, since: this.now };
        return;
      }
    }
  }

  /** Proposes on `member`'s replica and waits, on that replica's clock, for it
   * to apply the command. */
  call(member, make) {
    if (this.faults[member] === DOWN) throw injected('refused connection');
    const id = ++this.nextId;
    const command = make(id);
    if (command.type === 'compareExchange') this.callers.set(id, this.caller);
    const start = this.local(member);
    const started = this.now;
    let proposed = false;
    for (;;) {
      if (this.faults[member] === DOWN) throw injected('connection reset');
      if (!proposed) {
        // Without a known leader nothing was sent; proposing again cannot
        // duplicate the command.
        proposed = this.nodes[member].propose(this.local(member), command);
        this.flush();
      }
      const outcome = this.nodes[member].takeOutcome(id);
      if (outcome !== undefined) return outcome;
      if (this.local(member) >= start + DEADLINE) {
        this.nodes[member].forget(id);
        throw injected('deadline exceeded');
      }
      assert.ok(this.now - started < CALL_LIMIT, 'a call never ended');
      this.step();
    }
  }
}

/** One member's atomic journal store: its own replica of the Raft group. */
class Member {
  supportsDurableFlush = false;
  constructor(sim, id) {
    this.sim = sim;
    this.id = id;
  }
  async read(keys) {
    return this.sim.call(this.id, (id) => ({ type: 'read', id, keys: [...keys] })).versions;
  }
  async compareExchange(keys, expected, updates) {
    return this.sim.call(this.id, (id) => ({
      type: 'compareExchange',
      id,
      keys: [...keys],
      expected: structuredClone(expected),
      updates: structuredClone(updates),
    })).applied;
  }
}

// ---------------------------------------------------------------------------
// Client histories
// ---------------------------------------------------------------------------

const SHARDS = ['a', 'b'];
const SHARD_LINKS = 2;
const CLIENTS = 5;
const TRANSACTIONS = 80;
const FIRST_WRITE = 1000n;

/** One key per link of every shard, as the checker sees it. */
const key = (shard, link) => 10n * BigInt(shard + 1) + BigInt(link);

const transaction = (began = 0) => ({
  reads: [],
  writes: [],
  ownReads: [],
  revision: undefined,
  began,
  ended: 0,
});

function observe(record, link, target) {
  const own = record.writes.findLast(([written]) => written === link);
  if (own) record.ownReads.push([link, target, own[1]]);
  else record.reads.push([link, target]);
}

const ACKNOWLEDGED = 0;
const REFUSED = 1;
const UNKNOWN = 2;

async function history(seed, mode, preVoteAndCheckQuorum) {
  const sim = new Sim(seed, mode, preVoteAndCheckQuorum);
  // One member rewrites journals as checkpoints; the others read them.
  const members = Array.from(
    { length: REPLICAS },
    (_, id) =>
      new ClusterEngine(
        binding,
        new Member(sim, id),
        id === 1 ? { journalCheckpointInterval: 3 } : {},
      ),
  );
  // A fault-free start: elect a leader, then install the initial values.
  const initial = transaction();
  const setup = await members[0].begin(SHARDS);
  for (const [shard, name] of SHARDS.entries())
    for (let link = 1; link <= SHARD_LINKS; link++) {
      const session = setup.shard(name);
      assert.equal((await session.create(0n, 0n)).index, BigInt(link));
      await session.update(BigInt(link), BigInt(link), key(shard, link));
      initial.writes.push([key(shard, link), key(shard, link)]);
    }
  sim.caller = 0;
  await setup.commit();
  const records = [initial];
  const outcomes = [ACKNOWLEDGED];
  sim.chaos = true;
  sim.lastAnswer = sim.now;
  const source = rng(seed);
  const stats = {
    committed: 0,
    conflicts: 0,
    unknownCommitted: 0,
    unknownAborted: 0,
    unavailable: 0,
  };
  const clients = Array(CLIENTS).fill(undefined);
  let started = 0;
  let finished = 0;
  let value = FIRST_WRITE;
  while (finished < TRANSACTIONS) {
    for (let steps = 1 + below(source, 3); steps > 0; steps--) sim.step();
    const index = below(source, CLIENTS);
    if (clients[index] === undefined) {
      if (started === TRANSACTIONS) continue;
      started++;
      const shards = [[0], [1], [0, 1]][below(source, 3)];
      const ops = [];
      for (let count = 1 + below(source, 4); count > 0; count--) {
        const shard = shards[below(source, shards.length)];
        const link = 1 + below(source, SHARD_LINKS);
        const kind = below(source, 10);
        ops.push(
          kind <= 3
            ? { read: [shard, link] }
            : kind <= 7
              ? { write: [shard, link] }
              : { scan: shard },
        );
      }
      ops.push(below(source, 20) === 0 ? { rollback: true } : { commit: true });
      records.push(transaction(sim.now));
      outcomes.push(REFUSED);
      clients[index] = {
        member: index % REPLICAS,
        id: records.length - 1,
        shards,
        session: undefined,
        ops,
      };
    }
    const client = clients[index];
    const record = records[client.id];
    if (client.session === undefined) {
      try {
        client.session = await members[client.member].begin(
          client.shards.map((shard) => SHARDS[shard]),
        );
      } catch {
        stats.unavailable++;
        record.ended = sim.now;
        clients[index] = undefined;
        finished++;
        continue;
      }
    }
    const session = client.session;
    const op = client.ops.shift();
    let done = Boolean(op.commit || op.rollback);
    if (op.read) {
      const [shard, link] = op.read;
      const [found] = await session.shard(SHARDS[shard]).each([BigInt(link)]);
      observe(record, key(shard, link), found.target);
    } else if (op.scan !== undefined) {
      for (const found of await session.shard(SHARDS[op.scan]).each())
        observe(record, key(op.scan, found.index), found.target);
    } else if (op.write) {
      const [shard, link] = op.write;
      value++;
      await session.shard(SHARDS[shard]).update(BigInt(link), BigInt(link), value);
      record.writes.push([key(shard, link), value]);
    } else if (op.commit) {
      sim.caller = client.id;
      try {
        await session.commit();
        outcomes[client.id] = ACKNOWLEDGED;
      } catch (error) {
        if (String(error).includes('conflict')) stats.conflicts++;
        else {
          assert.match(String(error), /indeterminate/);
          outcomes[client.id] = UNKNOWN;
        }
      }
    } else await session.rollback();
    if (client.ops.length === 0) done = true;
    if (done) {
      record.ended = sim.now;
      clients[index] = undefined;
      finished++;
    }
  }
  // Heal everything, let the replicas converge, and read the outcome of every
  // exchange from the replicated log.
  sim.chaos = false;
  for (let node = 0; node < REPLICAS; node++) sim.heal(node);
  const healed = sim.now;
  for (;;) {
    sim.step();
    const last = Math.max(...sim.nodes.map((node) => node.lastIndex));
    if (
      sim.nodes.some((node) => node.role === RaftRole.Leader) &&
      sim.nodes.every((node) => node.appliedIndex === last)
    )
      break;
    assert.ok(sim.now - healed < 5_000, `seed ${seed}: no convergence after heal`);
  }
  for (let node = 1; node < REPLICAS; node++) {
    assert.deepEqual(
      sim.nodes[node].log,
      sim.nodes[0].log,
      `seed ${seed}: replicas disagree on the committed log`,
    );
    assert.deepEqual(sim.nodes[node].state, sim.nodes[0].state);
  }
  for (const [revision, command] of sim.nodes[0].state.exchanges) {
    const id = sim.callers.get(command);
    const record = records[id];
    assert.equal(record.revision, undefined, `seed ${seed}: transaction ${id} applied twice`);
    record.revision = BigInt(revision);
    if (outcomes[id] === ACKNOWLEDGED) stats.committed++;
    else if (outcomes[id] === UNKNOWN) {
      // Unknown to the client: it could have ended any time later.
      record.ended = Infinity;
      stats.unknownCommitted++;
    } else assert.fail(`seed ${seed}: refused transaction ${id} was applied`);
  }
  outcomes.forEach((outcome, id) => {
    if (id === 0 || records[id].revision !== undefined) return;
    if (outcome === ACKNOWLEDGED)
      assert.fail(`seed ${seed}: acknowledged transaction ${id} is not applied`);
    if (outcome === UNKNOWN) stats.unknownAborted++;
  });
  stats.sim = { ...sim.stats };
  const last = await members[2].begin(SHARDS);
  const finalState = new Map();
  for (const [shard, name] of SHARDS.entries())
    for (const link of await last.shard(name).each())
      finalState.set(key(shard, link.index), link.target);
  await last.close();
  return { history: { transactions: records, finalState }, stats };
}

const SEEDS = 20n;
/** Real-time ticks: the longest a healthy replica may wait for an answer while
 * one replica at a time keeps failing. Election timeouts are 100 to 200 local
 * units and clocks run at 0.5x to 2x, so one lost leader costs up to about 600
 * ticks; the bound leaves room for a second election when a vote is lost or a
 * clock steps backward. */
const STALL_BOUND = 1_500;

/** The Debug output of the Rust test, field for field. */
function format({ sim, ...stats }) {
  const fields = (object) =>
    Object.entries(object)
      .map(([name, count]) => `${name.replace(/[A-Z]/g, (c) => `_${c.toLowerCase()}`)}: ${count}`)
      .join(', ');
  return `Stats { ${fields(stats)}, sim: SimStats { ${fields(sim)} } }`;
}

const runs = new Map();
/** Sums the stats of every seed; memoized, since several tests share runs. */
async function run(t, mode, preVoteAndCheckQuorum) {
  const name = `${mode} pre-vote/check-quorum=${preVoteAndCheckQuorum}`;
  if (!runs.has(name)) {
    let sum;
    const stalls = [];
    for (let seed = 1n; seed <= SEEDS; seed++) {
      const { history: found, stats } = await history(seed, mode, preVoteAndCheckQuorum);
      const anomalies = check(found);
      assert.equal(anomalies.size, 0, `seed ${seed}: ${JSON.stringify([...anomalies])}`);
      if (sum === undefined) sum = structuredClone(stats);
      else {
        for (const field of Object.keys(stats)) if (field !== 'sim') sum[field] += stats[field];
        for (const field of Object.keys(stats.sim))
          sum.sim[field] = ['maxTerm', 'longestGap'].includes(field)
            ? Math.max(sum.sim[field], stats.sim[field])
            : sum.sim[field] + stats.sim[field];
      }
      stalls.push(stats.sim.longestGap);
    }
    runs.set(name, { sum, stalls });
  }
  const { sum, stalls } = runs.get(name);
  t.diagnostic(`${name}: ${format(sum)}`);
  t.diagnostic(`longest stall per seed: [${stalls.join(', ')}]`);
  return { sum, stalls };
}

/** The Rust reference, in experiments/issue-138/raft-simulation-expected.txt. */
const EXPECTED = {
  [`${MINORITY}/true`]: {
    line: 'Stats { committed: 529, conflicts: 335, unknown_committed: 6, unknown_aborted: 99, unavailable: 605, sim: SimStats { delivered: 111972, dropped: 22581, faults: 254, restarts: 60, clock_jumps: 96, leaders: 85, max_term: 8, probes: 5203, longest_gap: 1189 } }',
    stalls: [
      617, 367, 634, 636, 638, 314, 611, 624, 349, 1189, 323, 611, 628, 1103, 624, 610, 931, 640,
      469, 554,
    ],
  },
  [`${ANY}/true`]: {
    line: 'Stats { committed: 470, conflicts: 289, unknown_committed: 9, unknown_aborted: 126, unavailable: 682, sim: SimStats { delivered: 87923, dropped: 19118, faults: 330, restarts: 74, clock_jumps: 147, leaders: 106, max_term: 10, probes: 3764, longest_gap: 3656 } }',
    stalls: [
      1422, 1332, 2990, 1245, 2539, 314, 1948, 2219, 2574, 3195, 307, 3370, 1980, 1191, 1505, 1005,
      1005, 2899, 1141, 3656,
    ],
  },
  [`${MINORITY}/false`]: {
    line: 'Stats { committed: 492, conflicts: 333, unknown_committed: 9, unknown_aborted: 133, unavailable: 607, sim: SimStats { delivered: 106356, dropped: 25660, faults: 298, restarts: 69, clock_jumps: 132, leaders: 184, max_term: 64, probes: 4665, longest_gap: 2292 } }',
    stalls: [
      751, 623, 702, 648, 1313, 313, 2292, 606, 1003, 1739, 341, 476, 926, 1046, 675, 741, 1185,
      1390, 962, 635,
    ],
  },
};

async function pinned(t, mode, preVoteAndCheckQuorum) {
  const result = await run(t, mode, preVoteAndCheckQuorum);
  const expected = EXPECTED[`${mode}/${preVoteAndCheckQuorum}`];
  // The same schedules and outcomes as the Rust and C# simulations.
  assert.equal(format(result.sum), expected.line);
  assert.deepEqual(result.stalls, expected.stalls);
  return result.sum;
}

test(
  'a raft group stays strictly serializable and live while one replica keeps failing',
  { skip },
  async (t) => {
    const sum = await pinned(t, MINORITY, true);
    const context = format(sum);
    assert.ok(sum.sim.longestGap <= STALL_BOUND, `the group stalled: ${context}`);
    assert.ok(sum.committed > 400 && sum.conflicts > 50 && sum.sim.probes > 4000, context);
    assert.ok(sum.sim.restarts > 0 && sum.sim.clockJumps > 0 && sum.sim.leaders > SEEDS, context);
  },
);

test(
  'without pre-vote and check quorum one faulty replica stalls the group',
  { skip },
  async (t) => {
    // The same schedules, without the two options: still safe, but a deaf or
    // mute replica now stops progress for much longer.
    const withOptions = await pinned(t, MINORITY, true);
    const without = await pinned(t, MINORITY, false);
    assert.ok(
      without.sim.longestGap > STALL_BOUND && without.sim.maxTerm > 4 * withOptions.sim.maxTerm,
      `with: ${format(withOptions)}\nwithout: ${format(without)}`,
    );
  },
);

test('a raft group stays strictly serializable when every replica fails', { skip }, async (t) => {
  const sum = await pinned(t, ANY, true);
  const context = format(sum);
  assert.ok(sum.committed > 300, context);
  assert.ok(sum.unknownCommitted > 0 && sum.unknownAborted > 0, context);
  assert.ok(sum.unavailable > 0, context);
});

test('the same seed replays the same raft history', { skip }, async () => {
  const text = (value) =>
    JSON.stringify(value, (_, item) =>
      typeof item === 'bigint'
        ? `${item}`
        : item === Infinity
          ? 'Infinity'
          : item instanceof Map
            ? [...item]
            : item,
    );
  const first = await history(3n, ANY, true);
  const again = await history(3n, ANY, true);
  assert.equal(text(first.history.transactions), text(again.history.transactions));
  assert.deepEqual(first.stats, again.stats);
});
