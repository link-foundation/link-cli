// A simulated volatile disk and the crash workload shared by the engine
// simulations (engine-simulation.test.mjs, engine-power-loss-backends.test.mjs),
// as rust/tests/engine_simulation/disk.rs shares them between the Rust tests:
// every file keeps its durable bytes and the writes issued since its last
// sync, and power loss keeps any per-sector prefix of them.
//
// The engine is the Rust core compiled to WebAssembly: `binding` is the
// `wasm-pack build --target web` package of rust/wasm from CLINK_WASM, or
// js/pkg; without one it is undefined and `skip` says why.
import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import process from 'node:process';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { WasmEngine } from '../src/engine.js';
import { random } from './conformance/history.mjs';

const MASK = (1n << 64n) - 1n;
export const PAGE = 4096;

async function loadBinding() {
  const explicit = process.env.CLINK_WASM;
  const dir = explicit ?? fileURLToPath(new URL('../../pkg', import.meta.url));
  if (!explicit && !existsSync(join(dir, 'clink_wasm_bg.wasm'))) return undefined;
  const binding = await import(pathToFileURL(join(dir, 'clink_wasm.js')).href);
  binding.initSync({ module: readFileSync(join(dir, 'clink_wasm_bg.wasm')) });
  return binding;
}
export const binding = await loadBinding();
export const skip = binding
  ? false
  : 'set CLINK_WASM to a wasm-pack --target web build of rust/wasm';

/** splitmix64, shared with every port, plus a per-mille coin. */
export function rng(seed) {
  const source = random(seed & MASK);
  return { ...source, perMille: (chance) => source.below(1000n) < BigInt(chance) };
}
export const below = (source, bound) => Number(source.below(BigInt(bound)));
export const injected = (what) => new Error(`injected ${what}`);

// ---------------------------------------------------------------------------
// Simulated disk
// ---------------------------------------------------------------------------

export const SECTOR = 512;

function resized(bytes, length) {
  const result = new Uint8Array(length);
  result.set(bytes.subarray(0, Math.min(length, bytes.length)));
  return result;
}

/** One file: what survives power loss, what the page cache shows, and the
 * operations between them: `{ offset, bytes }` writes and `{ length }` cuts. */
export class SimFile {
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
export class Disk {
  files = [new SimFile(), new SimFile(), new SimFile()];
  /** After one failure the device fails every mutation until power loss. */
  failed = false;
  /** Mutations left before the power fails in the middle of the work. */
  countdown = undefined;
  /** Bound of the mutation count after which `crashRun` cuts the power. */
  reach = 60;
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
export class SimDevice {
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
export const FIRST_WRITE = 1000;

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
export async function withoutErrors(disks, body) {
  const errors = disks.map((disk) => disk.errors);
  for (const disk of disks) disk.errors = 0;
  try {
    return await body();
  } finally {
    disks.forEach((disk, at) => (disk.errors = errors[at]));
  }
}

/** Runs `cycles` rounds of work, each ended by power loss, and checks every
 * recovery. `open()` returns the commit log over the simulated `disks`.
 * Throws for any recovery that is not the exact state of one committed
 * revision. */
export async function crashRun(seed, options, cycles, open, disks) {
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
  await withoutErrors(disks, open);
  for (let cycle = 0; cycle < cycles; cycle++) {
    let db;
    try {
      db = new WasmEngine(binding, options, open());
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
      if (source.below(2n) === 0n) disks[0].countdown = below(source, disks[0].reach);
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

/** Field-wise sum of stats objects, `zero()` giving the field names. */
export function total(runs, zero = crashStats) {
  const sum = zero();
  for (const run of runs) for (const name of Object.keys(sum)) sum[name] += run[name];
  return sum;
}
/** Runs seeds 1..=count in order and sums their stats. */
export async function seeds(count, run, zero = crashStats) {
  const runs = [];
  for (let seed = 1n; seed <= count; seed++) runs.push(await run(seed));
  return total(runs, zero);
}
