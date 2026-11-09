import { causalSessionConformance } from './causal-session.mjs';
import { nativePageConformance } from './native-pages.mjs';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import {
  WasmEngine,
  MemoryCommitLog,
  ReplicatedCommitLog,
  DeltaCommitLog,
  TimedCommitLog,
  revisionAt,
} from '../../src/index.js';
import { groupCommitConformance } from './group-commit.mjs';
import { sharedStateConformance } from './shared-state.mjs';
import { changeRecordConformance } from './change-records.mjs';
import { constraintIndexConformance } from './constraint-index.mjs';
import { journalRecoveryConformance } from './journal-recovery.mjs';

async function savepointConformance(binding) {
  const engine = new WasmEngine(binding, { isolation: 'snapshot' });
  const session = engine.begin();
  try {
    const kept = await session.create(0n, 0n);
    const savepoint = await session.savepoint();
    assert.equal(typeof savepoint, 'bigint');
    const dropped = await session.create(0n, 0n);
    assert.equal(await session.count(), 2n);
    await session.rollbackTo(savepoint);
    assert.equal(await session.count(), 1n);
    // The discarded address stays retired and the savepoint stays usable.
    assert.equal((await session.create(0n, 0n)).index, dropped.index + 1n);
    await session.rollbackTo(savepoint);
    await session.releaseSavepoint(savepoint);
    await assert.rejects(session.rollbackTo(savepoint), /unknown savepoint/);
    await session.commit();
    const { rows } = engine.snapshot();
    assert.deepEqual(
      rows.map((row) => row.index),
      [kept.index],
    );
    await assert.rejects(session.savepoint(), /closed/);
  } finally {
    await session.close();
    engine.close();
  }
}

async function incrementalBackupConformance(binding) {
  const options = { history: 2 };
  const engine = new WasmEngine(binding, options);
  const rename = async (name) => {
    const tx = engine.begin();
    if (engine.snapshot().revision === 0n) await tx.create(0n, 0n);
    await tx.setName(1n, name);
    await tx.commit();
    await tx.close();
  };
  try {
    await rename('first');
    const log = new MemoryCommitLog();
    assert.equal(engine.backupIncremental(log), 1n);
    const full = JSON.parse(log.read());
    await rename('second');
    assert.equal(engine.backupIncremental(log), 2n);
    const added = JSON.parse(log.read()).slice(full.length);
    assert.equal(added.length, 1);
    assert.ok(added[0].startsWith('engine2|'));
    assert.equal(engine.backupIncremental(log), 2n);
    const restored = new WasmEngine(binding, options, log);
    assert.equal(restored.snapshot().revision, 2n);
    assert.equal(restored.snapshot().rows[0].name, 'second');
    restored.close();

    for (const name of ['third', 'fourth', 'fifth']) await rename(name);
    assert.throws(() => engine.backupIncremental(log), /not retained/);
    assert.equal(JSON.parse(log.read()).length, full.length + 1);

    const other = new WasmEngine(binding, options);
    const elsewhere = new MemoryCommitLog();
    other.backup(elsewhere);
    assert.throws(() => engine.backupIncremental(elsewhere), /not retained|not a copy/);
    other.close();
  } finally {
    engine.close();
  }
}

async function timedRestoreConformance(binding) {
  let now = 0;
  const [journal, timeline] = [new MemoryCommitLog(), new MemoryCommitLog()];
  const log = new TimedCommitLog(journal, timeline, { clock: () => now });
  const engine = new WasmEngine(binding, {}, log);
  try {
    for (const [time, name] of [
      [100, 'first'],
      [200, 'second'],
    ]) {
      now = time;
      const tx = engine.begin();
      if (name === 'first') await tx.create(0n, 0n);
      await tx.setName(1n, name);
      await tx.commit();
      await tx.close();
    }
  } finally {
    engine.close();
  }
  const copy = new MemoryCommitLog();
  const revision = revisionAt(timeline, 199);
  assert.equal(WasmEngine.restoreBackup(binding, journal, copy, { revision }), 1n);
  const restored = new WasmEngine(binding, {}, copy);
  assert.equal(restored.snapshot().rows[0].name, 'first');
  restored.close();
}

async function restoreBackupConformance(binding) {
  const engine = new WasmEngine(binding, { history: 'all' });
  try {
    const archive = new MemoryCommitLog();
    for (const name of ['first', 'second', 'third']) {
      const tx = engine.begin();
      if (name === 'first') await tx.create(0n, 0n);
      await tx.setName(1n, name);
      await tx.commit();
      await tx.close();
      engine.backupIncremental(archive);
    }
    const copy = new MemoryCommitLog();
    assert.equal(WasmEngine.restoreBackup(binding, archive, copy, { revision: 2n }), 2n);
    const restored = new WasmEngine(binding, {}, copy);
    assert.equal(restored.snapshot().revision, 2n);
    assert.equal(restored.snapshot().rows[0].name, 'second');
    restored.close();
    const whole = new MemoryCommitLog();
    assert.equal(WasmEngine.restoreBackup(binding, archive, whole), 3n);
    assert.equal(whole.read(), archive.read());
    assert.throws(() => WasmEngine.restoreBackup(binding, archive, whole), /must be empty/);
    const refused = new MemoryCommitLog();
    assert.throws(
      () => WasmEngine.restoreBackup(binding, archive, refused, { revision: 9n }),
      /not in the backup/,
    );
    assert.deepEqual(JSON.parse(refused.read()), []);
    await timedRestoreConformance(binding);
  } finally {
    engine.close();
  }
}

class ReplaceableMemoryLog extends MemoryCommitLog {
  replaced = 0;
  #records;
  get supportsReplacement() {
    return true;
  }
  replace(records) {
    this.replaced += 1;
    this.#records = [...records];
    return true;
  }
  read() {
    return this.#records ? JSON.stringify(this.#records) : super.read();
  }
  append(record) {
    if (this.#records) this.#records.push(record);
    else super.append(record);
  }
}

async function thresholdCompactionConformance(binding) {
  const log = new ReplaceableMemoryLog();
  const engine = new WasmEngine(binding, {}, log);
  const commit = async () => {
    const tx = engine.begin();
    await tx.create(0n, 0n);
    await tx.commit();
    await tx.close();
  };
  try {
    assert.throws(() => engine.compactJournalAfter(0n), /at least one commit/);
    await commit();
    assert.equal(engine.compactJournalAfter(2n), false);
    assert.equal(log.replaced, 0);
    await commit();
    assert.equal(engine.compactJournalAfter(2n), true);
    assert.equal(log.replaced, 1);
    assert.equal(engine.compactJournalAfter(1n), false);
    const worker = engine.compactEvery({ commits: 1n, periodMs: 1 });
    await commit();
    await new Promise((resolve) => {
      const wait = () => (log.replaced > 1 ? resolve() : setTimeout(wait, 1));
      wait();
    });
    worker.stop();
    const reopened = new WasmEngine(binding, {}, log);
    assert.equal(reopened.snapshot().revision, 3n);
    assert.equal(reopened.snapshot().rows.length, 3);
    reopened.close();
  } finally {
    engine.close();
  }
  const fixed = new WasmEngine(binding);
  assert.throws(() => fixed.compactJournalAfter(9n), /does not support/);
  assert.throws(() => fixed.compactEvery({ commits: 9n, periodMs: 1 }), /does not support/);
  fixed.close();
}

async function backupConformance(binding) {
  const options = { history: 'all' };
  const engine = new WasmEngine(binding, options);
  try {
    for (const name of ['first', 'second']) {
      const tx = engine.begin();
      const row = name === 'first' ? await tx.create(0n, 0n) : { index: 1n };
      await tx.setName(row.index, name);
      await tx.commit();
      await tx.close();
    }
    const log = new MemoryCommitLog();
    assert.equal(engine.backup(log), 2n);
    const restored = new WasmEngine(binding, options, log);
    assert.equal(restored.snapshot().revision, 2n);
    assert.equal(restored.snapshot().rows[0].name, 'second');
    assert.equal(restored.snapshot(1n).rows[0].name, 'first');
    restored.close();

    const pointInTime = new MemoryCommitLog();
    assert.equal(engine.backup(pointInTime, { revision: 1n }), 1n);
    const earlier = new WasmEngine(binding, options, pointInTime);
    assert.equal(earlier.snapshot().rows[0].name, 'first');
    earlier.close();
    assert.equal(engine.exportCheckpoint(1n).length, 2);
    assert.equal(engine.exportCheckpoint().length, 3);

    assert.throws(() => engine.backup(new MemoryCommitLog(), { revision: 9n }), /not retained/);
    assert.throws(() => engine.backup(log), /must be empty/);

    // A failing destination surfaces and leaves the engine writable.
    const destination = (overrides) => ({
      supportsDurableFlush: false,
      read: () => '[]',
      append() {},
      flush() {},
      ...overrides,
    });
    assert.throws(
      () =>
        engine.backup(
          destination({
            append() {
              throw 'append failed';
            },
          }),
        ),
      /append failed/,
    );
    assert.throws(() => engine.backup(destination({})), /verification failed/);
    // A log may use the engine: this would not return if the commit lock were held.
    const copy = new MemoryCommitLog();
    const seen = [];
    assert.equal(
      engine.backup(
        destination({
          read: () => copy.read(),
          append(record) {
            seen.push(engine.snapshot().revision);
            copy.append(record);
          },
        }),
      ),
      2n,
    );
    assert.deepEqual(seen, [2n, 2n, 2n]);
    const writing = engine.begin();
    await writing.setName(1n, 'after');
    assert.equal(await writing.commit(), 3n);
    await writing.close();
  } finally {
    engine.close();
  }
}

async function lockWaitConformance(binding) {
  const options = { concurrency: 'pessimistic', isolation: 'snapshot', lockWaitMs: 60_000 };
  const engine = new WasmEngine(binding, options);
  const seed = engine.begin();
  const first = await seed.create(0n, 0n);
  const second = await seed.create(0n, 0n);
  await seed.commit();
  await seed.close();
  const a = engine.begin();
  const b = engine.begin();
  try {
    await a.setName(first.index, 'a');
    await b.setName(second.index, 'b');
    // Queued behind nothing else, so it waits for a's address without blocking this test.
    const waiting = a.setName(second.index, 'a2');
    await new Promise((resolve) => setTimeout(resolve, 20));
    await assert.rejects(b.setName(first.index, 'b2'), /deadlock/);
    assert.equal(await b.getName(second.index), 'b');
    await b.rollback();
    await waiting;
    await a.commit();
    assert.deepEqual(
      engine.snapshot().rows.map((row) => row.name),
      ['a', 'a2'],
    );
    const stale = engine.begin();
    const owner = engine.begin();
    await owner.setName(first.index, 'owner');
    const refused = assert.rejects(stale.setName(first.index, 'stale'), /conflict/);
    await new Promise((resolve) => setTimeout(resolve, 20));
    await owner.commit();
    await refused;
    await stale.close();
    await owner.close();
  } finally {
    await a.close();
    await b.close();
    engine.close();
  }
  const short = new WasmEngine(binding, { ...options, lockWaitMs: 5 });
  const seeded = short.begin();
  const link = await seeded.create(0n, 0n);
  await seeded.commit();
  await seeded.close();
  const holder = short.begin();
  const busy = short.begin();
  await holder.setName(link.index, 'held');
  await assert.rejects(busy.setName(link.index, 'late'), /busy/);
  await holder.close();
  await busy.close();
  short.close();
}

/** The actual compiled Rust engine, run in Node, Bun and Deno by existing CI. */
export async function engineConformance(binding) {
  await causalSessionConformance(binding);
  await nativePageConformance(binding);
  await sharedStateConformance(binding);
  await constraintIndexConformance(binding);
  await changeRecordConformance(binding);
  await groupCommitConformance(binding);
  await pessimisticConformance(binding);
  await savepointConformance(binding);
  await backupConformance(binding);
  await incrementalBackupConformance(binding);
  await restoreBackupConformance(binding);
  await thresholdCompactionConformance(binding);
  await lockWaitConformance(binding);
  await journalRecoveryConformance(binding);
  const engine = new WasmEngine(binding, { history: 'all', isolation: 'snapshot' });
  const first = engine.begin();
  const observer = engine.begin();
  try {
    const created = first.create(0n, 0n);
    // Deliberately do not await create before committing: controls share its queue.
    const committed = first.commit();
    const link = await created;
    assert.equal(await committed, 1n);
    assert.equal(await observer.count(), 0n);
    assert.equal(engine.snapshot().rows.length, 1);
    assert.equal(engine.snapshot(0n).rows.length, 0);
    const a = engine.begin();
    const b = engine.begin();
    try {
      await a.setName(link.index, '雪;|😀');
      await b.setName(link.index, 'conflict');
      await a.commit();
      await assert.rejects(b.commit(), /conflict/i);
    } finally {
      await a.close();
      await b.close();
    }
    const reopened = new WasmEngine(binding, {}, engine.journal());
    const reader = reopened.begin();
    try {
      assert.equal(await reader.getName(link.index), '雪;|😀');
    } finally {
      await reader.close();
      reopened.close();
    }
    const rollback = engine.begin();
    await rollback.create(0n, 0n);
    await rollback.rollback();
    await rollback.close();
    assert.equal(engine.snapshot().rows.length, 1);
  } finally {
    await first.close();
    await observer.close();
    engine.close();
  }
  const exclusive = new WasmEngine(binding, { concurrency: 'exclusive' });
  const held = exclusive.begin();
  assert.throws(() => exclusive.begin(), /exclusive/);
  await held.close();
  await exclusive.begin().close();
  exclusive.close();
  for (const options of [
    [],
    ['optimistic'],
    null,
    { isolation: 'unknown' },
    { history: 0 },
    { constraints: ['unknown'] },
  ])
    assert.throws(() => new WasmEngine(binding, options));
  const vectors = JSON.parse(
    readFileSync(
      new URL('../../../../docs/protocol/engine-revision-vectors.json', import.meta.url),
      'utf8',
    ),
  );
  const portable = new WasmEngine(binding, { history: 'all' }, vectors.valid);
  assert.equal(portable.snapshot(1n).rows[0].name, '雪;|😀');
  assert.equal(portable.snapshot().rows[0].index, 4294967295n);
  assert.deepEqual(portable.journal(), vectors.valid);
  portable.close();
  for (const record of vectors.invalidU32)
    assert.throws(() => new WasmEngine(binding, {}, [record]));

  const primary = new MemoryCommitLog();
  const follower = new MemoryCommitLog();
  let partition = true;
  const replica = {
    read: () => follower.read(),
    append: (record) => follower.append(record),
    flush() {
      if (partition) throw Error('partition');
    },
    supportsDurableFlush: false,
  };
  const replication = new ReplicatedCommitLog(primary, [replica], { acknowledgements: 2 });
  const replicated = new WasmEngine(binding, {}, replication);
  const pending = replicated.begin();
  await pending.create(0n, 0n);
  await assert.rejects(pending.commit(), (error) => error.outcome === 'unknown');
  assert.throws(() => replicated.begin(), /indeterminate/);
  await pending.close();
  replicated.close();
  partition = false;
  assert.equal(replication.synchronize(), 2);
  const recovered = new WasmEngine(binding, {}, follower);
  assert.equal(recovered.snapshot().rows.length, 1);
  recovered.close();
  assert.throws(() => new WasmEngine(binding, { requireDurableFlush: true }, primary), /durable/);
  const asynchronousLog = { read: () => '[]', append: () => Promise.resolve(), flush() {} };
  const invalid = new WasmEngine(binding, {}, asynchronousLog);
  const invalidSession = invalid.begin();
  await invalidSession.create(0n, 0n);
  await assert.rejects(invalidSession.commit(), /synchronously/);
  await invalidSession.close();
  invalid.close();
  // Native bindings must catch throwing capability getters and require a
  // boolean, rather than treating an arbitrary truthy value as durability.
  for (const capability of [1, 'true', null]) {
    const log = { read: () => '[]', append() {}, flush() {}, supportsDurableFlush: capability };
    assert.throws(
      () => binding.StagedEngine.withLog('{"requireDurableFlush":true}', log),
      /durable/,
    );
  }
  const throwing = {
    read: () => '[]',
    append() {},
    flush() {},
    get supportsDurableFlush() {
      throw Error('capability unavailable');
    },
  };
  assert.throws(
    () => binding.StagedEngine.withLog('{"requireDurableFlush":true}', throwing),
    /durable/,
  );
  for (const log of [
    {
      read: () => {
        throw 'read failure';
      },
      append() {},
      flush() {},
    },
    { read: () => '{}', append() {}, flush() {} },
  ])
    assert.throws(() => binding.StagedEngine.withLog('{}', log));
  // The adapter's reason reaches the caller: a string, or an Error's message.
  for (const [thrown, reason] of [
    ['read failure', /read failure/],
    [new Error('replica 0 holds 1 records the primary lacks'), /replica 0 holds 1 records/],
    [new Error(''), /JavaScript log adapter failed/],
    [{ code: 5 }, /JavaScript log adapter failed/],
  ]) {
    const log = {
      read: () => {
        throw thrown;
      },
      append() {},
      flush() {},
    };
    assert.throws(() => binding.StagedEngine.withLog('{}', log), reason);
  }

  const parent = new WasmEngine(binding);
  let seed = parent.begin();
  const row = await seed.create(0n, 0n);
  await seed.commit();
  await seed.close();
  const branch = parent.fork({ history: 1 });
  const base = branch.mergeBaseRecord();
  let remote = branch.engine.begin();
  await remote.setName(row.index, 'branch');
  await remote.commit();
  await remote.close();
  seed = parent.begin();
  await seed.create(0n, 0n);
  await seed.commit();
  await seed.close();
  assert.equal(branch.merge(), 3n);
  assert.equal(branch.merge(), 3n);
  assert.equal(parent.snapshot().rows[0].name, 'branch');
  assert.equal(parent.snapshot().rows.length, 2);
  const branchJournal = branch.engine.journal();
  branch.close();
  branch.close();
  assert.throws(() => branch.merge(), /closed/);
  const resumed = parent.resumeBranch(branchJournal, base);
  assert.equal(resumed.merge(), 3n);
  seed = parent.begin();
  await seed.setName(row.index, 'parent');
  await seed.commit();
  await seed.close();
  remote = resumed.engine.begin();
  await remote.setName(row.index, 'conflict');
  await remote.commit();
  await remote.close();
  assert.throws(
    () => resumed.merge(),
    (error) => error.outcome === 'failed' && /conflict/i.test(error.message),
  );
  resumed.close();
  const branchLog = new MemoryCommitLog();
  const external = parent.fork({}, branchLog);
  const saved = external.mergeBaseRecord();
  external.close();
  const reopenedBranch = parent.resumeBranch(branchLog, saved);
  reopenedBranch.close();
  parent.close();

  const parentJournal = new MemoryCommitLog();
  const forkJournal = new MemoryCommitLog();
  let failFlush = false;
  const failingLog = {
    read: () => parentJournal.read(),
    append: (record) => parentJournal.append(record),
    flush() {
      if (failFlush) throw Error('merge flush failed');
    },
  };
  const uncertainParent = new WasmEngine(binding, {}, failingLog);
  const uncertainFork = uncertainParent.fork({}, forkJournal);
  const initialBase = uncertainFork.mergeBaseRecord();
  const branchWrite = uncertainFork.engine.begin();
  await branchWrite.create(0n, 0n);
  await branchWrite.commit();
  await branchWrite.close();
  failFlush = true;
  assert.throws(
    () => uncertainFork.merge(),
    (error) => error.outcome === 'unknown',
  );
  assert.throws(() => uncertainParent.snapshot(), /indeterminate/);
  uncertainFork.close();
  uncertainParent.close();
  failFlush = false;
  const mergeRecovered = new WasmEngine(binding, {}, parentJournal);
  assert.equal(mergeRecovered.snapshot().rows.length, 1);
  const retry = mergeRecovered.resumeBranch(forkJournal, initialBase);
  assert.equal(retry.merge(), 1n);
  retry.close();
  mergeRecovered.close();
  const deltas = JSON.parse(
    readFileSync(
      new URL('../../../../docs/protocol/engine-delta-vectors.json', import.meta.url),
      'utf8',
    ),
  );
  const physical = new MemoryCommitLog(deltas.validSequences[0].physical);
  const compressed = new WasmEngine(binding, { history: 'all' }, new DeltaCommitLog(physical));
  assert.equal(compressed.snapshot(2n).rows.find((row) => row.index === 100n).name, '雪;|😀');
  const update = compressed.begin();
  await update.setName(100n, 'after compressed reopen');
  await update.commit();
  await update.close();
  assert.match(JSON.parse(physical.read()).at(-1), /^delta1\|/);
  compressed.close();
  const decoded = new WasmEngine(binding, {}, new DeltaCommitLog(physical));
  assert.equal(
    decoded.snapshot().rows.find((row) => row.index === 100n).name,
    'after compressed reopen',
  );
  const compressedFork = decoded.fork({}, new DeltaCommitLog(new MemoryCommitLog()));
  compressedFork.close();
  decoded.close();
  await conflictGenerationSchedules(binding);
  await readSetSchedules(binding);
  console.log(
    'ok wasm engine: staged requests, commit ordering, isolation, history, conflict, Unicode, rollback, reopen, delta journals, damaged-journal check and repair',
  );
}

// The native core may reclaim conflict generations only after the final session
// validates. Exercise ABA, disjoint commits and branch publications in real WASM.
async function conflictGenerationSchedules(binding) {
  for (const isolation of ['snapshot', 'read-committed']) {
    const engine = new WasmEngine(binding, { isolation });
    try {
      const seed = engine.begin();
      const one = await seed.create(0n, 0n);
      const two = await seed.create(one.index, one.index);
      await seed.commit();
      await seed.close();
      const stale = engine.begin();
      await stale.setName(one.index, 'stale');
      const writer = engine.begin();
      await writer.setName(one.index, 'temporary');
      await writer.commit();
      await writer.close();
      const restored = engine.begin();
      await restored.removeName(one.index);
      await restored.commit();
      await restored.close();
      await assert.rejects(stale.commit(), /conflict/i);
      await stale.close();
      assert.equal(engine.snapshot().rows.find((row) => row.index === one.index).name, null);

      const first = engine.begin();
      const second = engine.begin();
      await first.setName(one.index, 'first');
      await second.setName(two.index, 'second');
      await first.commit();
      await second.commit();
      await first.close();
      await second.close();
      assert.equal(engine.snapshot().rows.find((row) => row.index === two.index).name, 'second');
      const branch = engine.fork();
      const pending = engine.begin();
      await pending.setName(one.index, 'pending');
      const child = branch.engine.begin();
      await child.setName(one.index, 'branch');
      await child.commit();
      await child.close();
      branch.merge();
      branch.close();
      await assert.rejects(pending.commit(), /conflict/i);
      await pending.close();
      assert.equal(engine.snapshot().rows.find((row) => row.index === one.index).name, 'branch');
      const reopened = new WasmEngine(binding, { isolation }, engine.journal());
      const next = reopened.begin();
      await next.setName(one.index, 'after reopen');
      await next.commit();
      await next.close();
      assert.equal(
        reopened.snapshot().rows.find((row) => row.index === one.index).name,
        'after reopen',
      );
      reopened.close();
    } finally {
      engine.close();
    }
  }
}

// Only point APIs use address read sets. Query/count/name scans retain the
// conservative whole-revision check so missing predicates cannot admit phantoms.
async function readSetSchedules(binding) {
  for (const options of [
    { serializableValidation: 'invalid' },
    { serializableValidation: 1 },
    { isolation: 'snapshot', serializableValidation: 'read-set' },
    { isolation: 'read-committed', serializableValidation: 'read-set' },
  ])
    assert.throws(() => new WasmEngine(binding, options));
  const engine = new WasmEngine(binding, { serializableValidation: 'read-set' });
  const sessions = [];
  const begin = () => {
    const session = engine.begin();
    sessions.push(session);
    return session;
  };
  try {
    const seed = begin();
    const one = await seed.create(0n, 0n);
    const two = await seed.create(0n, 0n);
    await seed.commit();
    const a = begin();
    const b = begin();
    await a.setName(one.index, 'one');
    await b.setName(two.index, 'two');
    assert.equal(await a.commit(), 2n);
    assert.equal(await b.commit(), 3n);
    const c = begin();
    const d = begin();
    assert.equal(await c.getName(two.index), 'two');
    assert.equal(await d.getName(one.index), 'one');
    await c.removeName(one.index);
    await d.removeName(two.index);
    await c.commit();
    await assert.rejects(d.commit(), /read.*changed/i);
    const readonly = begin();
    await readonly.getName(one.index);
    const unrelated = begin();
    await unrelated.setName(two.index, 'later');
    await unrelated.commit();
    assert.equal(await readonly.commit(), engine.snapshot().revision);
    const scan = begin();
    assert.equal(await scan.count(), 2n);
    const writer = begin();
    await writer.setName(two.index, 'after scan');
    await writer.commit();
    await assert.rejects(scan.commit(), /revision.*changed/i);
    const missing = begin();
    assert.equal(await missing.getName(3n), null);
    const insert = begin();
    assert.equal((await insert.create(0n, 0n)).index, 3n);
    await insert.commit();
    await assert.rejects(missing.commit(), /read.*changed/i);
    const aba = begin();
    await aba.getName(one.index);
    const temporary = begin();
    await temporary.setName(one.index, 'temporary');
    await temporary.commit();
    const restored = begin();
    await restored.removeName(one.index);
    await restored.commit();
    await assert.rejects(aba.commit(), /read.*changed/i);
    const branch = engine.fork();
    try {
      const stale = begin();
      await stale.getName(one.index);
      const child = branch.engine.begin();
      try {
        await child.setName(one.index, 'branch');
        await child.commit();
      } finally {
        await child.close();
      }
      branch.merge();
      await assert.rejects(stale.commit(), /read.*changed/i);
    } finally {
      branch.close();
    }
    console.log(
      'ok wasm serializable read sets: disjoint points, cycles, scans, missing rows, ABA, branches',
    );
  } finally {
    for (const session of sessions) await session.close();
    engine.close();
  }
}

async function pessimisticConformance(binding) {
  for (const validation of [
    { isolation: 'snapshot' },
    { isolation: 'serializable', serializableValidation: 'read-set' },
  ]) {
    const engine = new WasmEngine(binding, { concurrency: 'pessimistic', ...validation });
    const sessions = [];
    const begin = () => {
      const session = engine.begin();
      sessions.push(session);
      return session;
    };
    try {
      const seed = begin();
      const one = await seed.create(0n, 0n);
      const two = await seed.create(0n, 0n);
      await seed.commit();
      const first = begin();
      const second = begin();
      await first.setName(one.index, 'one');
      await second.setName(two.index, 'two');
      await assert.rejects(second.setName(one.index, 'busy'), /busy/i);
      await assert.rejects(first.setName(two.index, 'busy'), /busy/i);
      assert.equal(await second.getName(two.index), 'two');
      assert.equal(engine.snapshot().rows[0].name, null);
      await first.commit();
      if (validation.isolation === 'snapshot') await second.commit();
      else {
        // The rejected writes read those rows before checking write ownership;
        // retained point dependencies still require serializable validation.
        await assert.rejects(second.commit(), /conflict/i);
      }
      const owner = begin();
      const retry = begin();
      await owner.setName(one.index, 'private');
      await assert.rejects(retry.setName(one.index, 'retry'), /busy/i);
      await owner.close();
      await retry.setName(one.index, 'retry');
      await retry.commit();
      const recovered = new WasmEngine(binding, { concurrency: 'pessimistic' }, engine.journal());
      try {
        assert.equal(recovered.snapshot().rows[0].name, 'retry');
      } finally {
        recovered.close();
      }
    } finally {
      for (const session of sessions) await session.close();
      engine.close();
    }
  }
}
