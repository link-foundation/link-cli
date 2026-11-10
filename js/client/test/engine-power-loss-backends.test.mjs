// Sector-level power loss under the journal-file and LSM backends.
//
// A port of rust/tests/engine_power_loss_backends_tests.rs.
// engine-simulation.test.mjs runs the paged log over a simulated volatile
// disk: every 512-byte sector independently keeps any prefix of the writes
// issued since the last sync, sometimes half of the next one, and the file
// length comes from any prefix of the pending operations. This file puts the
// two other durable backends of the client on the same disk model
// (engine-simulation-disk.mjs).
//
// Journal file. `FileCommitLog` writes newline-framed records to a real file.
// `SimJournal` runs the real log on a temporary file and mirrors every append
// and flush to a simulated file. A failed append leaves nothing, a prefix or
// the whole line in the cache; a failed flush persists nothing. At power loss
// the simulated crash image becomes the real file, and the real
// `FileCommitLog` constructor (its torn-tail repair) and the compiled engine
// recover it. The repair truncation itself is a pending, unsynced length
// change, so a later power loss can undo it. The shared crash workload then
// requires the exact state of one revision between the last acknowledged and
// the last attempted commit. The engine is the Rust core compiled to
// WebAssembly, so these tests need CLINK_WASM or js/pkg and are skipped
// without one.
//
// LSM. `PhysicalLsmJournalStore` keeps a checkpointed paged write-ahead log (a
// root and two banks) and one paged file per sorted table. Every one of them
// is a `SimDevice`; creating and deleting a table is a durable directory
// operation, as the namespace contract requires. Random groups of keys are
// compare-exchanged and single keys compare-deleted, with stale guards for
// conflicts, between flushes and compactions. After every power loss the
// reopened store must read exactly the key state after one acknowledged or
// attempted group. This half needs no WebAssembly.
//
// SQLite is not simulated here: it runs in WAL mode with `synchronous=FULL`,
// and its crash tests replay power loss through a VFS that loses and tears
// unsynced sectors.
import test from 'node:test';
import assert from 'node:assert/strict';
import { appendFileSync, mkdtempSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import process from 'node:process';
import { FileCommitLog } from '../src/engine-file-log.js';
import { CheckpointPagedCommitLog } from '../src/engine-checkpoint-pages.js';
import { PhysicalLsmJournalStore } from '../src/engine-lsm-store.js';
import {
  Disk,
  SimDevice,
  below,
  crashRun,
  injected,
  rng,
  seeds,
  skip,
  withoutErrors,
} from './engine-simulation-disk.mjs';

const encoder = new TextEncoder();
const trace = () => process.env.SIM_TRACE !== undefined;

// ---------------------------------------------------------------------------
// Journal file
// ---------------------------------------------------------------------------

/** The real file log, with every append and flush mirrored to `disk`. */
class SimJournal {
  supportsDurableFlush = true;
  constructor(inner, disk, path) {
    this.inner = inner;
    this.disk = disk;
    this.path = path;
  }
  read() {
    return this.inner.read();
  }
  append(record) {
    const line = encoder.encode(record + '\n');
    const disk = this.disk;
    const offset = disk.files[0].view.length;
    if (disk.fail()) {
      // The failed call left nothing, a prefix or the whole line.
      const choice = below(disk.random, 3);
      const keep = choice === 0 ? 0 : choice === 1 ? below(disk.random, line.length) : line.length;
      disk.files[0].write(offset, line.subarray(0, keep));
      appendFileSync(this.path, line.subarray(0, keep));
      throw injected('append error');
    }
    // The record and its newline land as two writes.
    disk.files[0].write(offset, line.subarray(0, line.length - 1));
    disk.files[0].write(offset + line.length - 1, line.subarray(line.length - 1));
    this.inner.append(record);
  }
  flush() {
    this.disk.sync(0);
    this.inner.flush();
  }
  close() {
    this.inner.close();
  }
}

/** Makes the simulated image the real file and recovers it with the real
 * torn-tail repair. The repair's truncation is pending until a sync. */
function openJournal(path, disk) {
  const image = disk.files[0].view;
  writeFileSync(path, image);
  const inner = new FileCommitLog(path);
  const repaired = statSync(path).size;
  assert.ok(repaired <= image.length, `repair grew the journal to ${repaired}`);
  if (repaired < image.length) disk.files[0].setLength(repaired);
  return new SimJournal(inner, disk, path);
}

/** Different journal encodings, state structures and concurrency controls
 * append different records; each must survive the same faults. The file log
 * makes an append durable only at the engine's flush. */
const JOURNAL_CONFIGURATIONS = [
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
    requireDurableFlush: true,
  },
];

// An entry is a few hundred bytes and is synced at once, so a power cut finds
// unsynced sectors less often than under the paged log.
const JOURNAL_SEEDS = 48n;
const CYCLES = 8;

async function single(seed, options, lies, errors) {
  const directory = mkdtempSync(join(tmpdir(), 'clink-power-loss-'));
  const path = join(directory, 'journal.log');
  const disk = new Disk(seed * 41n, lies, errors);
  // An entry is one write and one sync, not a page set: lose power within
  // fewer mutations, so it lands between them.
  disk.reach = 8;
  // The previous owner's writer lock goes with the engine that used it.
  let current;
  const open = () => {
    current?.close();
    current = undefined;
    current = openJournal(path, disk);
    return current;
  };
  try {
    return await crashRun(seed, options, CYCLES, open, [disk]);
  } finally {
    current?.close();
    rmSync(directory, { recursive: true, force: true });
  }
}

test(
  'power loss under the journal file recovers exactly an acknowledged or attempted commit',
  { skip },
  async (t) => {
    for (const options of JOURNAL_CONFIGURATIONS) {
      const sum = await seeds(JOURNAL_SEEDS, (seed) => single(seed, options, 0, 15));
      const context = `${JSON.stringify(options)}: ${JSON.stringify(sum)}`;
      t.diagnostic(`journal file ${context}`);
      // An honest disk never loses an acknowledged commit or refuses to open;
      // the faults happened and failed commits resolved both ways.
      assert.deepEqual([sum.refused, sum.lost], [0, 0], context);
      assert.ok(sum.commits > 250, context);
      assert.ok(sum.dropped > 20 && sum.torn > 4, context);
      assert.ok(sum.unknownCommitted > 0 && sum.unknownLost > 0, context);
    }
  },
);

test(
  'a lying disk under the journal file rolls back to an exact revision or is refused',
  { skip },
  async (t) => {
    const sum = await seeds(JOURNAL_SEEDS, (seed) => single(seed, {}, 500, 0));
    t.diagnostic(`journal file, lying disk: ${JSON.stringify(sum)}`);
    // `crashRun` checked that every opened state is the exact state of a
    // committed revision; acknowledged commits were lost to the lies.
    assert.ok(sum.lied > 100 && sum.lost > 0, JSON.stringify(sum));
  },
);

test('the same seed replays the same journal file crashes', { skip }, async () => {
  assert.deepEqual(await single(7n, {}, 300, 15), await single(7n, {}, 300, 15));
});

// ---------------------------------------------------------------------------
// LSM
// ---------------------------------------------------------------------------

/** Table files by id. Creating and deleting one is durable at once. */
class SimTables {
  files = new Map();
  created = 0;
  constructor(seed, errors) {
    this.seed = seed;
    this.errors = errors;
  }
  disks() {
    return [...this.files.values()];
  }
  /** The namespace the store owns. */
  provider() {
    const tables = this;
    return {
      supportsDurableFlush: true,
      list(maxEntries) {
        if (tables.files.size > maxEntries) throw new Error('too many tables');
        return [...tables.files.keys()].sort((a, b) => (BigInt(a) < BigInt(b) ? -1 : 1));
      },
      create(id) {
        assert.equal(tables.files.has(id), false, `table ${id} reused`);
        tables.created++;
        const disk = new Disk(tables.seed * 1009n + BigInt(id), 0, tables.errors);
        tables.files.set(id, disk);
        return new SimDevice(disk);
      },
      open(id) {
        const disk = tables.files.get(id);
        if (disk === undefined) throw new Error(`table ${id} is gone`);
        return new SimDevice(disk);
      },
      close() {},
      delete(id) {
        tables.files.delete(id);
      },
    };
  }
}

/** Small limits, so buffers flush and runs compact within a few groups. A
 * group holds at most `bufferKeys` keys. */
const LSM_OPTIONS = {
  maxKeys: 32,
  maxBytes: 4096,
  bufferKeys: 2,
  bufferBytes: 128,
  maxRuns: 2,
  maxWalRecords: 4,
  maxWalBytes: 8192,
  maxNamespaceEntries: 16,
};

const KEYS = 6;
const LSM_SEEDS = 24n;

const lsmStats = () => ({
  commits: 0,
  conflicts: 0,
  flushes: 0,
  compactions: 0,
  tables: 0,
  crashes: 0,
  refused: 0,
  lost: 0,
  unknownCommitted: 0,
  unknownLost: 0,
  recoveryErrors: 0,
  lied: 0,
  dropped: 0,
  torn: 0,
});

const key = (index) => `k${index}`;
/** Present keys and their entries, in key order, as comparable text. */
const show = (state) => JSON.stringify([...state].sort(([a], [b]) => (a < b ? -1 : 1)));

function readAll(store) {
  const keys = Array.from({ length: KEYS }, (_, i) => key(i));
  const versions = store.read(keys);
  return new Map(
    keys.flatMap((name, at) => (versions[at].entries.length ? [[name, versions[at].entries]] : [])),
  );
}

async function lsmRun(seed, lies, errors) {
  const wal = [0n, 1n, 2n].map((i) => new Disk(seed * 43n + i, lies, errors));
  const tables = new SimTables(seed, errors);
  const all = () => [...wal, ...tables.disks()];
  const failed = () => all().some((disk) => disk.failed);
  const powerLoss = () => all().forEach((disk) => disk.powerLoss());
  const open = () =>
    PhysicalLsmJournalStore.open(
      new CheckpointPagedCommitLog(
        new SimDevice(wal[0]),
        [new SimDevice(wal[1]), new SimDevice(wal[2])],
        { maxPages: 128 },
      ),
      tables.provider(),
      LSM_OPTIONS,
    );
  const random = rng(seed);
  const stats = lsmStats();
  // States after each acknowledged or attempted group, oldest first.
  const states = [new Map()];
  let acked = 0;
  let attempted = 0;
  let value = 0;
  // Create the store without injected errors, as `crashRun` does.
  await withoutErrors(wal, () => open().close());
  for (let cycle = 0; cycle < CYCLES; cycle++) {
    let store;
    try {
      store = open();
    } catch (error) {
      if (failed()) {
        powerLoss();
        stats.crashes++;
        stats.recoveryErrors++;
        continue;
      }
      if (trace()) console.error(`seed ${seed}: open refused: ${error}`);
      stats.refused++;
      break;
    }
    try {
      const state = show(readAll(store));
      if (trace()) console.error(`seed ${seed}: recovered ${state}`);
      let at = attempted;
      while (at >= acked && show(states[at]) !== state) at--;
      if (at < acked)
        assert.fail(
          `seed ${seed}: recovered ${state}, expected one of ` +
            states.slice(acked, attempted + 1).map(show),
        );
      if (attempted > acked) {
        if (at === attempted) stats.unknownCommitted++;
        else stats.unknownLost++;
      }
      states.length = at + 1;
      acked = at;
      attempted = at;
      if (random.below(2n) === 0n) {
        const countdown = below(random, 80);
        wal[below(random, 3)].countdown = countdown;
      }
      // A guard read earlier, to make stale compare-exchanges.
      let stale;
      const takeStale = () => {
        const old = stale;
        stale = undefined;
        return old !== undefined && random.below(4n) === 0n ? old : undefined;
      };
      const steps = 5 + below(random, 30);
      for (let step = 0; step < steps; step++) {
        if (failed()) break;
        const roll = below(random, 100);
        // The key state if the group commits; none for maintenance.
        let next;
        let outcome;
        try {
          if (roll < 8) {
            stats.flushes++;
            store.flush();
          } else if (roll < 12) {
            stats.compactions++;
            store.compact();
          } else if (roll < 20) {
            // Delete one key at the generation read now or earlier.
            const name = key(below(random, KEYS));
            const old = takeStale();
            let generation;
            if (old !== undefined) {
              const at = old.keys.indexOf(name);
              generation = at === -1 ? '1' : old.versions[at].generation;
            } else generation = store.read([name])[0].generation;
            if (generation === '0') continue;
            next = new Map(states[attempted]);
            next.delete(name);
            outcome = store.compareDelete(name, generation);
          } else {
            const count = 1 + below(random, 2);
            const names = [
              ...new Set(Array.from({ length: count }, () => key(below(random, KEYS)))),
            ].sort();
            let guard = takeStale();
            if (guard === undefined) {
              guard = { keys: names, versions: store.read(names) };
              if (random.below(3n) === 0n) stale = structuredClone(guard);
            }
            const updates = guard.keys.map(() => {
              value++;
              // `null` only guards the key's generation.
              return random.below(6n) !== 0n ? [`v${value}`] : null;
            });
            next = new Map(states[attempted]);
            guard.keys.forEach((name, at) => {
              if (updates[at] !== null) next.set(name, updates[at]);
            });
            outcome = store.compareExchange(guard.keys, guard.versions, updates);
          }
        } catch (error) {
          if (trace()) console.error(`seed ${seed}: roll ${roll} failed: ${error}`);
          if (!failed()) assert.fail(`seed ${seed}: ${error?.stack ?? error}`);
          // A failed group may still have reached the WAL; a failed flush or
          // compaction keeps the key state.
          if (next !== undefined) {
            states.push(next);
            attempted = states.length - 1;
          }
          break;
        }
        if (trace()) console.error(`seed ${seed}: roll ${roll} -> ${outcome}`);
        if (outcome === true) {
          states.push(next);
          acked = states.length - 1;
          attempted = acked;
          stats.commits++;
        } else if (outcome === false) stats.conflicts++;
      }
    } finally {
      store.close();
    }
    powerLoss();
    stats.crashes++;
  }
  stats.tables = tables.created;
  for (const disk of all()) {
    stats.lied += disk.lied;
    stats.dropped += disk.dropped;
    stats.torn += disk.torn;
  }
  return stats;
}

test('power loss under the LSM store recovers exactly an acknowledged or attempted group', async (t) => {
  const sum = await seeds(LSM_SEEDS, (seed) => lsmRun(seed, 0, 15), lsmStats);
  // Deletes are counted with the other acknowledged groups.
  const context = JSON.stringify(sum);
  t.diagnostic(`lsm: ${context}`);
  assert.deepEqual([sum.refused, sum.lost], [0, 0], context);
  assert.ok(sum.commits > 200 && sum.conflicts > 10, context);
  assert.ok(sum.tables > 20 && sum.compactions > 10, context);
  assert.ok(sum.dropped > 20 && sum.torn > 4, context);
  assert.ok(sum.unknownCommitted > 0 && sum.unknownLost > 0, context);
});

test('the same seed replays the same LSM crashes', async () => {
  const again = await lsmRun(7n, 0, 15);
  assert.deepEqual(await lsmRun(7n, 0, 15), again);
  assert.notDeepEqual(await lsmRun(8n, 0, 15), again);
});
