#!/usr/bin/env node
// Starts the engine in a Docker container, kills it with SIGKILL at random
// moments, damages its files on purpose and repairs them, then checks that no
// acknowledged commit was lost or torn. Each language ships a writer that
// speaks one protocol (rust/examples/engine_crash_writer.rs documents it):
//
//   node .github/scripts/docker-crash.mjs --target rust|csharp|js
//     [--rounds N] [--seed S] [--artifact PATH] [--image IMAGE] [--report FILE]
//     [--work DIR]
//
// Build the writer first (cargo build --release --example engine_crash_writer,
// dotnet build -c Release examples/engine-crash-csharp, or wasm-pack into
// js/pkg). The driver only needs a Docker client: it runs the same on a CI
// runner's daemon and inside a docker:dind container, where the bind-mounted
// paths are the dind container's own. Every random choice comes from --seed,
// which is printed first, so a failure can be replayed; the damaged files of a
// failed run stay in a directory under --work.
import { spawnSync } from 'node:child_process';
import {
  appendFileSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  truncateSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import process from 'node:process';
import { setTimeout as sleep } from 'node:timers/promises';
import { fileURLToPath } from 'node:url';
import { parseArgs } from 'node:util';

const repository = resolve(fileURLToPath(new URL('../..', import.meta.url)));

export const targets = {
  rust: {
    image: 'ubuntu:24.04',
    artifact: join(repository, 'rust/target/release/examples/engine_crash_writer'),
    mounts: (artifact) => [[artifact, '/app/writer']],
    command: ['/app/writer'],
    env: {},
  },
  csharp: {
    image: 'mcr.microsoft.com/dotnet/runtime:10.0',
    artifact: join(repository, 'examples/engine-crash-csharp/bin/Release/net10.0'),
    mounts: (artifact) => [[artifact, '/app']],
    command: ['dotnet', '/app/EngineCrashWriter.dll'],
    env: { HOME: '/tmp', DOTNET_CLI_TELEMETRY_OPTOUT: '1', DOTNET_NOLOGO: '1' },
  },
  js: {
    image: 'node:26-slim',
    artifact: join(repository, 'js/pkg'),
    mounts: (artifact) => [
      [join(repository, 'js/client'), '/app/client'],
      [artifact, '/app/pkg'],
    ],
    command: ['node', '/app/client/examples/engine-crash-writer.mjs'],
    env: { CLINK_WASM: '/app/pkg' },
    // A mkdir lock survives SIGKILL; it is removed once `docker wait` has
    // proved the writer dead, which is the fencing a deployment must do too.
    writerLocks: true,
  },
};

/** A small seeded generator (mulberry32): the same seed replays a run. */
export function random(seed) {
  let state = seed >>> 0;
  const next = () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let value = Math.imul(state ^ (state >>> 15), state | 1);
    value ^= value + Math.imul(value ^ (value >>> 7), value | 61);
    return ((value ^ (value >>> 14)) >>> 0) / 4294967296;
  };
  next.below = (bound) => Math.floor(next() * bound);
  return next;
}

/** Splits writer output into acknowledged revisions and anything else. */
export function parseOutput(text) {
  const acks = [];
  const other = [];
  for (const line of text.split('\n')) {
    if (!line) continue;
    const ack = /^ack (\d+)$/.exec(line);
    if (ack) acks.push(Number(ack[1]));
    else other.push(line);
  }
  return { acks, other };
}

/** Acknowledgements must continue the verified revision one by one. */
export function checkAcks(acks, from) {
  acks.forEach((revision, offset) => {
    if (revision !== from + offset + 1)
      throw new Error(`acknowledged ${acks.join(',')} after revision ${from}`);
  });
  return acks.length ? acks.at(-1) : from;
}

/**
 * After a crash the primary keeps everything it held before (`last` is that
 * revision or the newest acknowledgement) and at most the one commit that was
 * being written. The replica, written after the primary, holds at least every
 * acknowledged commit: a writer killed before its first acknowledgement may
 * not have caught it up with an earlier unacknowledged commit yet.
 */
export function checkSurvivors({ last, acknowledged, primary, replica }) {
  if (acknowledged > last) throw new Error(`acknowledged ${acknowledged} beyond revision ${last}`);
  if (primary < last || primary > last + 1)
    throw new Error(`primary at ${primary} after revision ${last}`);
  if (replica < acknowledged || replica > primary)
    throw new Error(`replica at ${replica}, primary at ${primary}, acknowledged ${acknowledged}`);
}

/** Byte offsets just past each newline: record i is [starts[i], starts[i + 1]). */
export function recordStarts(bytes) {
  const starts = [0];
  bytes.forEach((byte, offset) => {
    if (byte === 10) starts.push(offset + 1);
  });
  return starts;
}

/** The record that contains a byte offset. */
export function recordAt(starts, offset) {
  let record = 0;
  while (starts[record + 1] <= offset) record++;
  return record;
}

function docker(args, { allowFailure = false } = {}) {
  const result = spawnSync('docker', args, { encoding: 'utf8', timeout: 300_000 });
  if (result.error) throw result.error;
  if (result.status !== 0 && !allowFailure)
    throw new Error(`docker ${args.slice(0, 2).join(' ')} failed: ${result.stderr.trim()}`);
  return result;
}

class Harness {
  constructor({ target, artifact, image, seed, label, work }) {
    this.target = target;
    this.artifact = artifact;
    this.image = image;
    this.next = random(seed);
    this.label = label;
    mkdirSync(work, { recursive: true });
    this.data = mkdtempSync(join(work, 'engine-crash-'));
    this.primary = join(this.data, 'primary');
    this.replica = join(this.data, 'replica');
    this.revision = 0;
    // The newest revision some writer printed `ack` for: every revision up to
    // it was durable on both files when the caller was told so.
    this.acknowledged = 0;
    this.containers = 0;
  }

  run({ detach = false, tmpfs } = {}) {
    const args = ['run', '--network', 'none', '--label', this.label];
    const user = process.getuid?.();
    if (user !== undefined) args.push('--user', `${user}:${process.getgid()}`);
    args.push(detach ? '--detach' : '--rm');
    if (detach) args.push('--name', `${this.label.replace('=', '-')}-${++this.containers}`);
    for (const [source, destination] of this.target.mounts(this.artifact))
      args.push('--mount', `type=bind,source=${source},target=${destination},readonly`);
    args.push(
      '--mount',
      tmpfs
        ? `type=tmpfs,destination=/data,tmpfs-size=${tmpfs},tmpfs-mode=1777`
        : `type=bind,source=${this.data},target=/data`,
    );
    for (const [name, value] of Object.entries(this.target.env))
      args.push('--env', `${name}=${value}`);
    args.push(this.image);
    return args;
  }

  writer(...command) {
    const result = docker([...this.run(), ...this.target.command, ...command], {
      allowFailure: true,
    });
    return { status: result.status, ...parseOutput(result.stdout), stderr: result.stderr };
  }

  verify(file) {
    const result = this.writer('verify', `/data/${file}`);
    const line = result.other.join(' ');
    if (result.status === 0 && /^revision \d+$/.test(line)) return Number(line.split(' ')[1]);
    if (result.status === 2 && line.startsWith('refused ')) return { refused: line.slice(8) };
    throw new Error(`verify ${file}: status ${result.status} ${line} ${result.stderr.trim()}`);
  }

  revisionOf(file) {
    const revision = this.verify(file);
    if (typeof revision !== 'number') throw new Error(`${file} refused: ${revision.refused}`);
    return revision;
  }

  repair(...copies) {
    const result = this.writer('repair', '/data/primary', ...copies.map((copy) => `/data/${copy}`));
    const match = /^repaired (\d+) changed (true|false)$/.exec(result.other.join(' '));
    if (result.status !== 0 || !match)
      throw new Error(
        `repair: status ${result.status} ${result.other.join(' ')} ${result.stderr.trim()}`,
      );
    return { revision: Number(match[1]), changed: match[2] === 'true' };
  }

  write(commits) {
    const result = this.writer('write', '/data/primary', '/data/replica', String(commits));
    if (result.status !== 0)
      throw new Error(
        `write: status ${result.status} ${result.other.join(' ')} ${result.stderr.trim()}`,
      );
    if (result.other.length) throw new Error(`write printed ${result.other.join(' ')}`);
    this.revision = checkAcks(result.acks, this.revision);
    if (result.acks.length !== commits) throw new Error(`write acknowledged ${result.acks}`);
    this.acknowledged = this.revision;
  }

  removeWriterLocks() {
    if (!this.target.writerLocks) return;
    for (const path of [this.primary, this.replica])
      rmSync(`${path}.writer`, { recursive: true, force: true });
  }

  /** Starts a writer, kills it with SIGKILL and checks what survived. */
  async crash(round) {
    const started = docker([
      ...this.run({ detach: true }),
      ...this.target.command,
      'write',
      '/data/primary',
      '/data/replica',
    ]);
    const container = started.stdout.trim();
    // A quarter of the kills land while the writer opens, replays and catches
    // its replica up; the rest land between and inside commits.
    const early = this.next() < 0.25;
    if (early) {
      await sleep(this.next.below(400));
    } else {
      const deadline = Date.now() + 120_000;
      while (!parseOutput(docker(['logs', container]).stdout).acks.length) {
        if (Date.now() > deadline) throw new Error(`round ${round}: no commit within 120 s`);
        if (docker(['inspect', '-f', '{{.State.Running}}', container]).stdout.trim() !== 'true')
          break;
        await sleep(20);
      }
      await sleep(this.next.below(150));
    }
    docker(['kill', '--signal', 'KILL', container], { allowFailure: true });
    const status = Number(docker(['wait', container]).stdout.trim());
    const output = parseOutput(docker(['logs', container]).stdout);
    docker(['rm', '--force', container]);
    if (status !== 137 || output.other.length)
      throw new Error(`round ${round}: writer exited ${status}: ${output.other.join(' ')}`);
    this.removeWriterLocks();
    const last = checkAcks(output.acks, this.revision);
    const acknowledged = output.acks.length ? last : this.acknowledged;
    const primary = this.revisionOf('primary');
    const replica = this.revisionOf('replica');
    checkSurvivors({ last, acknowledged, primary, replica });
    this.revision = primary;
    this.replicaRevision = replica;
    this.acknowledged = Math.max(this.acknowledged, acknowledged);
    return { round, early, acks: output.acks.length, acknowledged, primary, replica };
  }

  /** Damage written by `damage(bytes)` must be refused, then repaired. */
  damaged(name, damage, { copies = ['replica'], expected }) {
    const bytes = readFileSync(this.primary);
    const starts = recordStarts(bytes);
    const where = damage(bytes, starts);
    const refused = this.verify('primary');
    if (typeof refused === 'number')
      throw new Error(`${name}: damage at record ${where.record} opened at revision ${refused}`);
    const damagedBytes = readFileSync(this.primary);
    const repair = this.repair(...copies);
    const want = expected(where);
    if (repair.revision !== want || !repair.changed)
      throw new Error(`${name}: repaired to ${repair.revision}, expected ${want}`);
    const preserved = readFileSync(`${this.primary}.damaged-${++this.preserved}`);
    if (!preserved.equals(damagedBytes)) throw new Error(`${name}: damaged bytes were not kept`);
    if (this.revisionOf('primary') !== want) throw new Error(`${name}: repair did not reopen`);
    // Revisions past the last acknowledgement were never promised to anyone;
    // an acknowledged one may only go when the damage destroyed every copy.
    const lost = Math.max(0, this.acknowledged - want);
    if (lost && copies.length)
      throw new Error(`${name}: lost acknowledged revisions ${want + 1}..${this.acknowledged}`);
    const dropped = Math.max(0, this.revision - Math.max(want, this.acknowledged));
    this.revision = want;
    this.acknowledged = Math.min(this.acknowledged, want);
    return { phase: name, ...where, refused: refused.refused, revision: want, lost, dropped };
  }

  phases() {
    this.preserved = 0;
    const results = [];
    const nonFinal = (starts) => this.next.below(starts.length - 2);
    // One flipped bit inside a record that is not the last one.
    results.push(
      this.damaged(
        'bit-flip',
        (bytes, starts) => {
          const record = nonFinal(starts);
          const offset = starts[record] + this.next.below(starts[record + 1] - starts[record]);
          bytes[offset] ^= 1 << this.next.below(8);
          writeFileSync(this.primary, bytes);
          return { record, offset };
        },
        { expected: ({ record }) => Math.max(record, this.replicaRevision) },
      ),
    );
    // A zeroed 512-byte sector before the last record, as a lost write leaves.
    results.push(
      this.damaged(
        'zeroed-sector',
        (bytes, starts) => {
          const last = starts.at(-2);
          const length = Math.min(512, last);
          const offset = this.next.below(last - length + 1);
          bytes.fill(0, offset, offset + length);
          writeFileSync(this.primary, bytes);
          return { record: recordAt(starts, offset), offset, length };
        },
        { expected: ({ record }) => Math.max(record, this.replicaRevision) },
      ),
    );
    // A lost tail cannot be told from a torn write, so the primary opens
    // shorter; a writer must refuse to start while its replica holds commits
    // the primary lacks, and the replica restores them.
    const truncated = (() => {
      const bytes = readFileSync(this.primary);
      const starts = recordStarts(bytes);
      const offset = this.next.below(starts[this.replicaRevision]);
      truncateSync(this.primary, offset);
      const record = recordAt(starts, offset);
      const opened = this.revisionOf('primary');
      if (opened !== record) throw new Error(`truncated at record ${record}, opened at ${opened}`);
      const refused = this.writer('write', '/data/primary', '/data/replica', '1');
      if (
        refused.status !== 1 ||
        refused.acks.length ||
        !/records the primary lacks/i.test(refused.other.join(' '))
      )
        throw new Error(`a writer started behind its replica: ${refused.other.join(' ')}`);
      this.removeWriterLocks();
      if (this.revisionOf('primary') !== record)
        throw new Error('the refused writer changed the primary');
      const repair = this.repair('replica');
      if (repair.revision !== this.replicaRevision)
        throw new Error(`truncation repaired to ${repair.revision}`);
      this.preserved++;
      this.revision = this.revisionOf('primary');
      return {
        phase: 'truncated-tail',
        record,
        offset,
        refused: refused.other.join(' '),
        revision: this.revision,
        lost: 0,
        dropped: 0,
      };
    })();
    results.push(truncated);
    // With the replica gone too, repair keeps the valid prefix and says what it
    // dropped; the restarted writer rebuilds the replica from the primary.
    rmSync(this.replica, { force: true });
    rmSync(`${this.replica}.lock`, { force: true });
    this.replicaRevision = 0;
    results.push(
      this.damaged(
        'no-copy',
        (bytes, starts) => {
          const record = nonFinal(starts);
          bytes[starts[record]] ^= 0x40;
          writeFileSync(this.primary, bytes);
          return { record, offset: starts[record] };
        },
        { copies: [], expected: ({ record }) => record },
      ),
    );
    this.write(3);
    if (this.revisionOf('replica') !== this.revision)
      throw new Error('the restarted writer did not rebuild the replica');
    return results;
  }

  /** A writer on a full disk must fail cleanly and resume once space returns. */
  fullDisk() {
    const writer = this.target.command.join(' ');
    const script = [
      'head -c 40960 /dev/zero > /data/filler',
      `${writer} write /data/primary /data/replica; echo "status $?"`,
      'rm /data/filler',
      `${writer} verify /data/primary; echo "status $?"`,
      `${writer} verify /data/replica; echo "status $?"`,
      `${writer} write /data/primary /data/replica 3; echo "status $?"`,
      `${writer} verify /data/replica; echo "status $?"`,
    ].join('\n');
    const result = docker([...this.run({ tmpfs: 65536 }), 'sh', '-c', script]);
    const lines = result.stdout.split('\n').filter(Boolean);
    const statuses = lines
      .filter((line) => line.startsWith('status '))
      .map((line) => line.slice(7));
    const acks = lines
      .filter((line) => line.startsWith('ack '))
      .map((line) => Number(line.slice(4)));
    const revisions = lines
      .filter((line) => line.startsWith('revision '))
      .map((line) => Number(line.slice(9)));
    const error = lines.find((line) => line.startsWith('error '));
    if (statuses.join(',') !== '1,0,0,0,0' || !error || revisions.length !== 3)
      throw new Error(`full disk: ${lines.join(' | ')} ${result.stderr.trim()}`);
    const full = acks.length - 3;
    const acknowledged = checkAcks(acks.slice(0, full), 0);
    checkSurvivors({
      last: acknowledged,
      acknowledged,
      primary: revisions[0],
      replica: revisions[1],
    });
    checkAcks(acks.slice(full), revisions[0]);
    if (revisions[2] !== revisions[0] + 3) throw new Error(`full disk: replica at ${revisions[2]}`);
    return {
      acknowledged,
      error,
      primary: revisions[0],
      replica: revisions[1],
      resumed: revisions[2],
    };
  }
}

async function main() {
  const { values } = parseArgs({
    options: {
      target: { type: 'string' },
      rounds: { type: 'string', default: '12' },
      seed: { type: 'string', default: String(Date.now() % 4294967296) },
      artifact: { type: 'string' },
      image: { type: 'string' },
      report: { type: 'string' },
      work: { type: 'string', default: tmpdir() },
    },
  });
  const target = targets[values.target];
  if (!target) throw new Error(`--target must be one of ${Object.keys(targets).join(', ')}`);
  const artifact = resolve(values.artifact ?? target.artifact);
  if (!existsSync(artifact))
    throw new Error(`build the ${values.target} writer first: ${artifact}`);
  const seed = Number(values.seed);
  const rounds = Number(values.rounds);
  const label = `link-cli-crash=${process.pid}`;
  const harness = new Harness({
    target,
    artifact,
    image: values.image ?? target.image,
    seed,
    label,
    work: resolve(values.work),
  });
  console.log(
    `target ${values.target}, image ${harness.image}, seed ${seed}, data ${harness.data}`,
  );
  const report = { target: values.target, image: harness.image, seed, crashes: [], damage: [] };
  try {
    harness.write(40);
    console.log(`wrote revisions 1..40`);
    for (let round = 1; round <= rounds; round++) {
      const result = await harness.crash(round);
      report.crashes.push(result);
      console.log(
        `crash ${round}${result.early ? ' (early)' : ''}: ${result.acks} acks, ` +
          `acknowledged ${result.acknowledged}, primary ${result.primary}, replica ${result.replica}`,
      );
    }
    for (const result of harness.phases()) {
      report.damage.push(result);
      console.log(
        `${result.phase} at record ${result.record}: ${result.refused ? `refused (${result.refused}), ` : ''}` +
          `repaired to revision ${result.revision}` +
          (result.lost ? `, ${result.lost} acknowledged revisions had no other copy` : '') +
          (result.dropped ? `, dropped ${result.dropped} unacknowledged` : ''),
      );
    }
    for (let round = rounds + 1; round <= rounds + 3; round++)
      report.crashes.push(await harness.crash(round));
    harness.write(1);
    if (!readFileSync(harness.primary).equals(readFileSync(harness.replica)))
      throw new Error('primary and replica differ after a clean commit');
    console.log(
      `after 3 more crashes the primary and replica are identical at ${harness.revision}`,
    );
    report.fullDisk = harness.fullDisk();
    console.log(
      `full disk: ${report.fullDisk.error}; kept ${report.fullDisk.primary}, ` +
        `resumed to ${report.fullDisk.resumed} once space returned`,
    );
    report.ok = true;
    rmSync(harness.data, { recursive: true, force: true });
  } catch (error) {
    report.ok = false;
    report.error = error.message;
    console.error(`FAILED (seed ${seed}, files kept in ${harness.data}): ${error.message}`);
    process.exitCode = 1;
  } finally {
    const stale = docker(['ps', '--all', '--quiet', '--filter', `label=${label}`]).stdout.split(
      '\n',
    );
    for (const id of stale.filter(Boolean)) docker(['rm', '--force', id], { allowFailure: true });
    if (values.report) writeFileSync(values.report, JSON.stringify(report, null, 2) + '\n');
    if (process.env.GITHUB_STEP_SUMMARY)
      appendFileSync(
        process.env.GITHUB_STEP_SUMMARY,
        `### Crash test: ${values.target}\n\n${report.ok ? 'passed' : `failed: ${report.error}`} ` +
          `(seed ${seed}, ${report.crashes.length} kills, ${report.damage.length} damage phases)\n\n`,
      );
  }
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) await main();
