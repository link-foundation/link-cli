// Deterministic simulation of an unreliable disk and an unreliable network.
//
// A port of rust/tests/engine_simulation_tests.rs. FoundationDB and
// TigerBeetle test their storage engines by running the real code against
// simulated disks and networks, driven by one seeded random source, so that
// every fault schedule replays exactly. This test does the same for the two
// places where the client depends on hardware it does not control.
//
// Disk. `SimDevice` implements the page device (and the repair device) over
// three simulated files (pages and two metadata slots). Each file keeps its
// durable bytes and the writes issued since its last sync. A simulated power
// loss keeps, for every 512-byte sector independently, the durable bytes or
// any prefix of the pending writes to that sector, sometimes half of the next
// one (a torn sector), and a file length from any prefix of the pending
// operations. That is every reordering and loss a volatile disk cache may
// perform within a flush interval. Writes and syncs also fail at random (the
// failed call may or may not have reached the cache). The real
// `PagedCommitLog` and the compiled engine run several clients on top; after
// every crash the test reopens the database and requires the exact committed
// state of one revision between the last acknowledged and the last attempted
// commit.
//
// A lying disk acknowledges syncs without persisting anything. Over such a
// disk no engine can keep its promise, so the test requires the next best
// thing: recovery either refuses the damaged database or returns the exact
// state of an earlier revision, never a mixture. A mirror that includes a
// lying disk must recover every acknowledged commit; with an honest primary
// it must also always open, because power loss alone never tears both copies
// of a metadata decision.
//
// Network. Several `ClusterEngine` members reach one atomic journal store
// through simulated links that drop requests, lose replies, deliver requests
// late (after other transactions ran), and are cut or made one-way by
// partitions. Clients on every member run random transactions over two
// shards. Every client observation goes into one history, which the
// Elle-style checker shared with the history conformance scenario searches for
// anomalies, including real-time edges between members: the cluster must be
// strictly serializable. Commits with unknown outcomes are resolved from the
// store's own log, as a client would read fresh journals after reconnecting.
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
import { WasmEngine } from '../src/engine.js';
import { ClusterEngine } from '../src/cluster.js';
import { PagedCommitLog } from '../src/engine-paged-log.js';
import { MirroredPageDevice } from '../src/engine-mirrored-pages.js';
import { check, random } from './conformance/history.mjs';

const MASK = (1n << 64n) - 1n;
const PAGE = 4096;

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
// Simulated disk
// ---------------------------------------------------------------------------

const SECTOR = 512;

function resized(bytes, length) {
  const result = new Uint8Array(length);
  result.set(bytes.subarray(0, Math.min(length, bytes.length)));
  return result;
}

/** One file: what survives power loss, what the page cache shows, and the
 * operations between them: `{ offset, bytes }` writes and `{ length }` cuts. */
class SimFile {
  durable = new Uint8Array(0);
  view = new Uint8Array(0);
  pending = [];
  write(offset, bytes) {
    if (this.view.length < offset + bytes.length)
      this.view = resized(this.view, offset + bytes.length);
    this.view.set(bytes, offset);
    this.pending.push({ offset, bytes: bytes.slice() });
  }
  setLength(length) {
    this.view = resized(this.view, length);
    this.pending.push({ length });
  }
  sync() {
    this.durable = this.view.slice();
    this.pending = [];
  }
  /** Applies `ops` to the bytes of one sector, as they would land on the disk. */
  static replay(sector, start, ops) {
    const end = start + sector.length;
    for (const op of ops) {
      if (op.bytes) {
        const from = Math.max(op.offset, start);
        const to = Math.min(op.offset + op.bytes.length, end);
        if (from < to)
          sector.set(op.bytes.subarray(from - op.offset, to - op.offset), from - start);
      } else if (op.length < end) sector.fill(0, Math.max(op.length - start, 0));
    }
  }
  static touches(op, start, end) {
    return op.bytes ? op.offset < end && op.offset + op.bytes.length > start : op.length < end;
  }
  /** Power loss. Returns how many sectors lost or tore a pending write. */
  crash(source) {
    let dropped = 0;
    let torn = 0;
    const pending = this.pending;
    if (pending.length > 0) {
      // The length after any prefix of the pending operations.
      const cut = below(source, pending.length + 1);
      let length = this.durable.length;
      for (const op of pending.slice(0, cut))
        length = op.bytes ? Math.max(length, op.offset + op.bytes.length) : op.length;
      const result = resized(this.durable, length);
      for (let start = 0; start < length; start += SECTOR) {
        const end = Math.min(start + SECTOR, length);
        const touching = [];
        pending.forEach((op, at) => {
          if (SimFile.touches(op, start, end)) touching.push(at);
        });
        if (touching.length === 0) continue;
        // Each sector independently keeps any prefix of its writes.
        const keep = below(source, touching.length + 1);
        const sector = result.slice(start, end);
        if (keep > 0) SimFile.replay(sector, start, pending.slice(0, touching[keep - 1] + 1));
        if (keep < touching.length) {
          dropped++;
          if (source.below(4n) === 0n) {
            // The next write lands halfway.
            const next = sector.slice();
            SimFile.replay(next, start, [pending[touching[keep]]]);
            const half = Math.floor(sector.length / 2);
            sector.set(next.subarray(0, half));
            torn++;
          }
        }
        result.set(sector, start);
      }
      this.durable = result;
    }
    this.view = this.durable.slice();
    this.pending = [];
    return [dropped, torn];
  }
}

/** Pages, metadata slot 0 and metadata slot 1, as the file page device keeps them. */
class Disk {
  files = [new SimFile(), new SimFile(), new SimFile()];
  /** After one failure the device fails every mutation until power loss. */
  failed = false;
  /** Mutations left before the power fails in the middle of the work. */
  countdown = undefined;
  lied = 0;
  dropped = 0;
  torn = 0;
  /** `lies`: per mille of syncs acknowledged without persisting anything;
   * `errors`: per mille of writes and syncs that fail. */
  constructor(seed, lies, errors) {
    this.random = rng(seed);
    this.lies = lies;
    this.errors = errors;
  }
  /** Whether this mutation fails; a failed write may still reach the cache. */
  fail() {
    if (this.countdown === 0) {
      this.countdown = undefined;
      this.failed = true;
    } else if (this.countdown !== undefined) this.countdown--;
    if (!this.failed && this.errors > 0 && this.random.perMille(this.errors)) {
      this.failed = true;
      return true;
    }
    return this.failed;
  }
  mutate(file, apply) {
    if (this.fail()) {
      if (this.random.below(2n) === 0n) apply(this.files[file]);
      throw injected('write error');
    }
    apply(this.files[file]);
  }
  sync(file) {
    if (this.fail()) throw injected('sync error');
    if (this.lies > 0 && this.random.perMille(this.lies)) {
      this.lied++;
      return;
    }
    this.files[file].sync();
  }
  powerLoss() {
    for (const file of this.files) {
      const [lost, half] = file.crash(this.random);
      this.dropped += lost;
      this.torn += half;
    }
    this.failed = false;
    this.countdown = undefined;
  }
}

/** The page device and repair device over one simulated disk. */
class SimDevice {
  supportsDurableFlush = true;
  constructor(disk) {
    this.disk = disk;
  }
  newDatabaseId() {
    const id = new Uint8Array(16);
    const data = new DataView(id.buffer);
    data.setBigUint64(0, this.disk.random.next(), true);
    data.setBigUint64(8, this.disk.random.next(), true);
    return id;
  }
  get dataLength() {
    return this.disk.files[0].view.length;
  }
  readMetadata(slot) {
    const view = this.disk.files[1 + slot].view;
    if (view.length === 0) return undefined;
    return view.length === 128 ? view.slice() : new Uint8Array(1);
  }
  writeMetadata(slot, bytes) {
    this.disk.mutate(1 + slot, (file) => file.write(0, bytes));
  }
  syncMetadata(slot) {
    this.disk.sync(1 + slot);
  }
  readPage(index) {
    const view = this.disk.files[0].view;
    const start = index * PAGE;
    if (view.length < start + PAGE) throw injected('short read');
    return view.slice(start, start + PAGE);
  }
  writePage(index, bytes) {
    this.disk.mutate(0, (file) => file.write(index * PAGE, bytes));
  }
  syncData() {
    this.disk.sync(0);
  }
  truncateData(length) {
    this.disk.mutate(0, (file) => file.setLength(length));
  }
  repairPage(index, bytes) {
    if (this.disk.files[0].view.length < index * PAGE) throw injected('repair hole');
    this.writePage(index, bytes);
  }
}

// ---------------------------------------------------------------------------
// Crash workload
// ---------------------------------------------------------------------------

const LINKS = 4;
const DISK_CLIENTS = 3;
const FIRST_WRITE = 1000;

const crashStats = () => ({
  commits: 0,
  crashes: 0,
  /** Opens that refused a damaged database. */
  refused: 0,
  /** Recoveries to a revision before the last acknowledged one. */
  lost: 0,
  /** Commits whose flush failed, then recovered as committed / not committed. */
  unknownCommitted: 0,
  unknownLost: 0,
  /** Recoveries interrupted by an injected error, then retried. */
  recoveryErrors: 0,
  lied: 0,
  dropped: 0,
  torn: 0,
});
const sameState = (left, right) =>
  left !== undefined &&
  left.size === right.size &&
  [...left].every(([link, target]) => right.get(link) === target);
const show = (state) => JSON.stringify([...state]);

/** Every disk's injected error rate is zero while `body` runs. */
async function withoutErrors(disks, body) {
  const errors = disks.map((disk) => disk.errors);
  for (const disk of disks) disk.errors = 0;
  try {
    return await body();
  } finally {
    disks.forEach((disk, at) => (disk.errors = errors[at]));
  }
}

/** Runs `cycles` rounds of work, each ended by power loss, and checks every
 * recovery. Throws for any recovery that is not the exact state of one
 * committed revision. */
async function crashRun(seed, options, cycles, device, disks) {
  const failed = () => disks.some((disk) => disk.failed);
  const powerLoss = () => disks.forEach((disk) => disk.powerLoss());
  const source = rng(seed);
  const stats = crashStats();
  const states = new Map([[0, new Map()]]);
  let acked = 0;
  let attempted = 0;
  let value = FIRST_WRITE;
  // Create the database without injected errors: power loss during the very
  // first metadata write leaves no decision at all, and an unreadable log is
  // refused rather than treated as new.
  await withoutErrors(disks, () => new PagedCommitLog(device()));
  for (let cycle = 0; cycle < cycles; cycle++) {
    let db;
    try {
      db = new WasmEngine(binding, options, new PagedCommitLog(device()));
    } catch (error) {
      // Recovery itself truncates and repairs, so it can hit an injected
      // error too; the machine loses power again and retries.
      if (failed()) {
        powerLoss();
        stats.crashes++;
        stats.recoveryErrors++;
        continue;
      }
      if (process.env.SIM_TRACE) console.error(`seed ${seed}: open refused: ${error}`);
      stats.refused++;
      break;
    }
    const clients = Array(DISK_CLIENTS).fill(undefined);
    try {
      const snapshot = db.snapshot();
      const revision = Number(snapshot.revision);
      const state = new Map(snapshot.rows.map((row) => [Number(row.index), Number(row.target)]));
      if (revision > attempted || !sameState(states.get(revision), state))
        assert.fail(
          `seed ${seed}: recovered revision ${revision} ${show(state)}, expected one of ` +
            `${JSON.stringify([...states].map(([at, s]) => [at, [...s]]))} ` +
            `between acknowledged ${acked} and attempted ${attempted}`,
        );
      if (revision < acked) stats.lost++;
      if (attempted > acked) {
        if (revision === attempted) stats.unknownCommitted++;
        else stats.unknownLost++;
      }
      for (const at of [...states.keys()]) if (at > revision) states.delete(at);
      acked = revision;
      attempted = revision;
      if (revision === 0) {
        // Set up every link in one commit, without injected errors.
        await withoutErrors(disks, async () => {
          const setup = db.begin();
          try {
            for (let link = 1; link <= LINKS; link++) {
              assert.equal((await setup.create(0n, 0n)).index, BigInt(link));
              await setup.update(link, link, link);
            }
            const at = Number(await setup.commit());
            states.set(at, new Map(Array.from({ length: LINKS }, (_, i) => [i + 1, i + 1])));
            acked = at;
            attempted = at;
          } finally {
            await setup.close();
          }
        });
      }
      // Half of the rounds lose power in the middle of a write or sync,
      // leaving whatever the cache had not yet persisted.
      if (source.below(2n) === 0n) disks[0].countdown = below(source, 60);
      const steps = 5 + below(source, 40);
      for (let step = 0; step < steps; step++) {
        if (failed()) break;
        const slot = below(source, DISK_CLIENTS);
        if (clients[slot] === undefined) {
          let session;
          try {
            session = db.begin();
          } catch {
            continue;
          }
          const length = 1 + below(source, 4);
          const ops = Array.from({ length }, () => ({
            link: 1 + below(source, LINKS),
            write: source.below(2n) === 0n,
          }));
          ops.push(undefined);
          clients[slot] = { session, ops, writes: new Map() };
        }
        const client = clients[slot];
        const op = client.ops.shift();
        let finished;
        if (op && !op.write) {
          assert.equal((await client.session.each([op.link])).length, 1, 'links are never deleted');
          finished = false;
        } else if (op) {
          value++;
          finished = await client.session.update(op.link, op.link, value).then(
            () => false,
            () => true,
          );
          client.writes.set(op.link, value);
        } else {
          const latest = Math.max(...states.keys());
          const next = new Map([...states.get(latest), ...client.writes]);
          let at;
          try {
            at = Number(await client.session.commit());
          } catch {
            at = undefined;
          }
          if (at === undefined) {
            if (failed()) {
              states.set(latest + 1, next);
              attempted = latest + 1;
            }
          } else if (client.writes.size > 0 || at !== latest) {
            // A read-only commit reports the revision it read.
            if (at !== latest + 1) assert.fail(`seed ${seed}: revision ${at} after ${latest}`);
            states.set(at, next);
            acked = at;
            attempted = at;
            stats.commits++;
          }
          finished = true;
        }
        if (finished) {
          await client.session.close();
          clients[slot] = undefined;
        }
      }
    } finally {
      for (const client of clients) await client?.session.close();
      db.close();
    }
    powerLoss();
    stats.crashes++;
  }
  for (const disk of disks) {
    stats.lied += disk.lied;
    stats.dropped += disk.dropped;
    stats.torn += disk.torn;
  }
  return stats;
}

/** Different journal encodings, state structures and concurrency controls
 * write different pages; each must survive the same faults. */
const DISK_CONFIGURATIONS = [
  {},
  {
    concurrency: 'pessimistic',
    isolation: 'snapshot',
    stateStructure: 'shared',
    revisionEncoding: 'changes',
  },
  {
    serializableValidation: 'read-set',
    stateStructure: 'sorted-runs',
    revisionEncoding: 'changes',
    history: 4,
  },
  { concurrency: 'exclusive', requireDurableFlush: true },
];

const DISK_SEEDS = 24n;
const CYCLES = 8;

function total(runs) {
  const sum = crashStats();
  for (const run of runs) for (const name of Object.keys(sum)) sum[name] += run[name];
  return sum;
}
async function seeds(count, run) {
  const runs = [];
  for (let seed = 1n; seed <= count; seed++) runs.push(await run(seed));
  return total(runs);
}

function single(seed, options, lies, errors) {
  const disk = new Disk(seed * 31n, lies, errors);
  return crashRun(seed, options, CYCLES, () => new SimDevice(disk), [disk]);
}

test(
  'power loss over a reordering disk recovers exactly an acknowledged or attempted commit',
  { skip },
  async (t) => {
    for (const options of DISK_CONFIGURATIONS) {
      const sum = await seeds(DISK_SEEDS, (seed) => single(seed, options, 0, 15));
      const context = `${JSON.stringify(options)}: ${JSON.stringify(sum)}`;
      t.diagnostic(context);
      // An honest disk never loses an acknowledged commit or refuses to open.
      assert.deepEqual([sum.refused, sum.lost], [0, 0], context);
      // The faults happened: unsynced sectors were lost and torn, and failed
      // flushes recovered both ways.
      assert.ok(sum.commits > 250, context);
      assert.ok(sum.dropped > 20 && sum.torn > 4, context);
      assert.ok(sum.unknownCommitted > 0 && sum.unknownLost > 0, context);
    }
  },
);

test(
  'a lying disk is detected or rolls back to an exact earlier revision, never a mixture',
  { skip },
  async (t) => {
    const sum = await seeds(DISK_SEEDS, (seed) => single(seed, {}, 500, 0));
    t.diagnostic(JSON.stringify(sum));
    // Acknowledged commits are lost, and some recoveries refuse the damage;
    // `crashRun` already checked that every opened state is exact.
    assert.ok(sum.lied > 100, JSON.stringify(sum));
    assert.ok(sum.lost > 0 && sum.refused > 0, JSON.stringify(sum));
  },
);

test('a mirror of honest and lying disks keeps every acknowledged commit', { skip }, async (t) => {
  // [primary lies, secondary lies]
  for (const lies of [
    [false, false],
    [false, true],
    [true, false],
  ]) {
    const sum = await seeds(DISK_SEEDS, (seed) => {
      const first = new Disk(seed * 31n, lies[0] ? 500 : 0, 10);
      const second = new Disk(seed * 37n, lies[1] ? 500 : 0, 10);
      return crashRun(
        seed,
        {},
        CYCLES,
        () => new MirroredPageDevice(new SimDevice(first), new SimDevice(second)),
        [first, second],
      );
    });
    const context = `${lies}: ${JSON.stringify(sum)}`;
    t.diagnostic(context);
    assert.equal(sum.lost, 0, context);
    assert.ok(sum.commits > 300, context);
    assert.equal(sum.lied > 100, lies[0] || lies[1], context);
    // The secondary copy of a decision is written only after the primary
    // copy is durable, so power loss alone never tears both copies. Only a
    // primary that lies about its sync can leave both torn; that second
    // fault is refused, never a silent rollback.
    if (!lies[0]) assert.equal(sum.refused, 0, context);
  }
});

test('the same seed replays the same crashes', { skip }, async () => {
  assert.deepEqual(await single(7n, {}, 300, 15), await single(7n, {}, 300, 15));
  assert.notDeepEqual(await single(7n, {}, 300, 15), await single(8n, {}, 300, 15));
});

// ---------------------------------------------------------------------------
// Simulated network
// ---------------------------------------------------------------------------

/** Up; Cut: nothing gets through; OneWay: requests arrive, replies are lost. */
const UP = 'up';
const CUT = 'cut';
const ONE_WAY = 'one-way';

/** The atomic store and every member's link to it. */
class Network {
  faults = false;
  links = [];
  revision = 0;
  /** key -> { generation, entries }, as `AtomicJournalStore.read` returns them. */
  journals = new Map();
  /** The transaction id of every applied compare-exchange, by revision. */
  applied = new Map();
  /** Requests the network holds back and delivers later. */
  delayed = [];
  /** The transaction a member is committing; the store logs it. */
  caller = 0;
  deliveredLate = 0;
  constructor(seed, members) {
    this.random = rng(seed * 7919n);
    this.links = Array(members).fill(UP);
  }
  apply({ keys, expected, updates, caller }) {
    if (
      keys.some(
        (key, at) => (this.journals.get(key)?.generation ?? '0') !== expected[at].generation,
      )
    )
      return false;
    const generation = ++this.revision;
    keys.forEach((key, at) => {
      if (updates[at] !== null)
        this.journals.set(key, { generation: String(generation), entries: [...updates[at]] });
    });
    this.applied.set(generation, caller);
    return true;
  }
  deliverOneLate() {
    const request = this.delayed.shift();
    if (request && this.apply(request)) this.deliveredLate++;
  }
  /** Whether the request reaches the store. */
  send(member) {
    return !this.faults || (this.links[member] !== CUT && !this.random.perMille(40));
  }
  /** Whether the reply reaches the member. */
  reply(member) {
    return !this.faults || (this.links[member] === UP && !this.random.perMille(40));
  }
}

/** One member's atomic journal store: the shared store behind its own link. */
class Member {
  supportsDurableFlush = false;
  constructor(network, id) {
    this.network = network;
    this.id = id;
  }
  async read(keys) {
    const network = this.network;
    if (!network.send(this.id)) throw injected('dropped request');
    const values = keys.map((key) =>
      structuredClone(network.journals.get(key) ?? { generation: '0', entries: [] }),
    );
    if (!network.reply(this.id)) throw injected('lost reply');
    return values;
  }
  async compareExchange(keys, expected, updates) {
    const network = this.network;
    const request = {
      keys: [...keys],
      expected: structuredClone(expected),
      updates: structuredClone(updates),
      caller: network.caller,
    };
    if (!network.send(this.id)) throw injected('dropped request');
    if (network.faults && network.random.perMille(60)) {
      // A slow link: the client times out; the request arrives later.
      network.delayed.push(request);
      throw injected('timeout');
    }
    const applied = network.apply(request);
    if (!network.reply(this.id)) throw injected('lost reply');
    return applied;
  }
}

const SHARDS = ['a', 'b'];
const SHARD_LINKS = 2n;
const MEMBERS = 3;
const NET_CLIENTS = 5;
const NET_TRANSACTIONS = 80;
const NET_SEEDS = 30n;

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

/** A history of `NET_TRANSACTIONS` transactions by clients spread over
 * `MEMBERS` cluster members. */
async function networkHistory(seed) {
  const network = new Network(seed, MEMBERS);
  // One member rewrites journals as checkpoints; the others read them.
  const members = Array.from(
    { length: MEMBERS },
    (_, id) =>
      new ClusterEngine(
        binding,
        new Member(network, id),
        id === 1 ? { journalCheckpointInterval: 3 } : {},
      ),
  );
  // Transaction 0 installs the initial values.
  const initial = transaction();
  const setup = await members[0].begin(SHARDS);
  for (const [shard, name] of SHARDS.entries())
    for (let link = 1n; link <= SHARD_LINKS; link++) {
      const session = setup.shard(name);
      assert.equal((await session.create(0n, 0n)).index, link);
      await session.update(link, link, key(shard, link));
      initial.writes.push([key(shard, link), key(shard, link)]);
    }
  await setup.commit();
  const records = [initial];
  const outcomes = [ACKNOWLEDGED];
  network.faults = true;
  const source = rng(seed);
  const stats = {
    committed: 0,
    conflicts: 0,
    unknownCommitted: 0,
    unknownAborted: 0,
    deliveredLate: 0,
    unavailable: 0,
  };
  const clients = Array(NET_CLIENTS).fill(undefined);
  let started = 0;
  let finished = 0;
  let now = 0;
  let value = BigInt(FIRST_WRITE);
  while (finished < NET_TRANSACTIONS) {
    now++;
    if (source.below(25n) === 0n) {
      const member = below(source, MEMBERS);
      network.links[member] = [CUT, ONE_WAY, UP, UP][below(source, 4)];
    }
    if (source.below(6n) === 0n) network.deliverOneLate();
    const index = below(source, NET_CLIENTS);
    if (clients[index] === undefined) {
      if (started === NET_TRANSACTIONS) continue;
      started++;
      const shards = [[0], [1], [0, 1]][below(source, 3)];
      const ops = [];
      for (let count = 1 + below(source, 4); count > 0; count--) {
        const shard = shards[below(source, shards.length)];
        const link = 1n + source.below(SHARD_LINKS);
        const kind = below(source, 10);
        ops.push(
          kind <= 3
            ? { read: [shard, link] }
            : kind <= 7
              ? { write: [shard, link] }
              : { scan: shard },
        );
      }
      ops.push(source.below(20n) === 0n ? { rollback: true } : { commit: true });
      records.push(transaction(now));
      outcomes.push(REFUSED);
      clients[index] = {
        member: index % MEMBERS,
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
        // The member could not reach the store: nothing happened.
        stats.unavailable++;
        record.ended = now;
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
      const [found] = await session.shard(SHARDS[shard]).each([link]);
      observe(record, key(shard, link), found.target);
    } else if (op.scan !== undefined) {
      for (const found of await session.shard(SHARDS[op.scan]).each())
        observe(record, key(op.scan, found.index), found.target);
    } else if (op.write) {
      const [shard, link] = op.write;
      value++;
      await session.shard(SHARDS[shard]).update(link, link, value);
      record.writes.push([key(shard, link), value]);
    } else if (op.commit) {
      network.caller = client.id;
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
      record.ended = now;
      clients[index] = undefined;
      finished++;
    }
  }
  // Heal the network; requests still in flight arrive.
  while (network.delayed.length > 0) network.deliverOneLate();
  network.faults = false;
  stats.deliveredLate = network.deliveredLate;
  for (const [revision, id] of [...network.applied].sort(([a], [b]) => a - b)) {
    const record = records[id];
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
  // Read the final state through a healed member.
  const last = await members[2].begin(SHARDS);
  const finalState = new Map();
  for (const [shard, name] of SHARDS.entries())
    for (const link of await last.shard(name).each())
      finalState.set(key(shard, link.index), link.target);
  await last.close();
  return { history: { transactions: records, finalState }, stats };
}

test('cluster members over a faulty network stay strictly serializable', { skip }, async (t) => {
  const sum = {
    committed: 0,
    conflicts: 0,
    unknownCommitted: 0,
    unknownAborted: 0,
    deliveredLate: 0,
    unavailable: 0,
  };
  const shardsPerCommit = new Set();
  for (let seed = 1n; seed <= NET_SEEDS; seed++) {
    const { history, stats } = await networkHistory(seed);
    const anomalies = check(history);
    assert.equal(anomalies.size, 0, `seed ${seed}: ${JSON.stringify([...anomalies])}`);
    for (const { writes, revision } of history.transactions)
      if (revision !== undefined)
        shardsPerCommit.add(new Set(writes.map(([written]) => written / 10n)).size);
    for (const name of Object.keys(sum)) sum[name] += stats[name];
  }
  // Every kind of outcome happened, including cross-shard commits, late
  // deliveries that still committed, and unknown outcomes both ways.
  const context = JSON.stringify(sum);
  t.diagnostic(`${context} shards per commit ${JSON.stringify([...shardsPerCommit])}`);
  assert.ok(shardsPerCommit.has(2), JSON.stringify([...shardsPerCommit]));
  assert.ok(sum.committed > 500 && sum.conflicts > 50, context);
  assert.ok(sum.unknownCommitted > 0 && sum.unknownAborted > 0, context);
  assert.ok(sum.deliveredLate > 0 && sum.unavailable > 0, context);
});

test('the same seed replays the same network history', { skip }, async () => {
  const text = (value) =>
    JSON.stringify(value, (_, item) =>
      typeof item === 'bigint' ? `${item}` : item instanceof Map ? [...item] : item,
    );
  const first = await networkHistory(3n);
  const again = await networkHistory(3n);
  assert.equal(text(first.history.transactions), text(again.history.transactions));
  assert.deepEqual(first.stats, again.stats);
});
