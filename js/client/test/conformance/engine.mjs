import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { WasmEngine, MemoryCommitLog, ReplicatedCommitLog } from '../../src/index.js';

/** The actual compiled Rust engine, run in Node, Bun and Deno by existing CI. */
export async function engineConformance(binding) {
  const engine = new WasmEngine(binding, { history: 'all', isolation: 'snapshot' });
  const first = engine.begin(); const observer = engine.begin();
  try {
    const created = first.create(0n, 0n);
    // Deliberately do not await create before committing: controls share its queue.
    const committed = first.commit();
    const link = await created;
    assert.equal(await committed, 1n);
    assert.equal(await observer.count(), 0n);
    assert.equal(engine.snapshot().rows.length, 1);
    assert.equal(engine.snapshot(0n).rows.length, 0);
    const a = engine.begin(); const b = engine.begin();
    try {
      await a.setName(link.index, '雪;|😀'); await b.setName(link.index, 'conflict');
      await a.commit(); await assert.rejects(b.commit(), /conflict/i);
    } finally { await a.close(); await b.close(); }
    const reopened = new WasmEngine(binding, {}, engine.journal());
    const reader = reopened.begin();
    try { assert.equal(await reader.getName(link.index), '雪;|😀'); }
    finally { await reader.close(); reopened.close(); }
    const rollback = engine.begin();
    await rollback.create(0n, 0n); await rollback.rollback(); await rollback.close();
    assert.equal(engine.snapshot().rows.length, 1);
  } finally { await first.close(); await observer.close(); engine.close(); }
  const exclusive = new WasmEngine(binding, { concurrency: 'exclusive' });
  const held = exclusive.begin();
  assert.throws(() => exclusive.begin(), /exclusive/);
  await held.close();
  await exclusive.begin().close(); exclusive.close();
  for (const options of [{ isolation: 'unknown' }, { history: 0 }, { constraints: ['unknown'] }])
    assert.throws(() => new WasmEngine(binding, options));
  const vectors = JSON.parse(readFileSync(new URL('../../../../docs/protocol/engine-revision-vectors.json', import.meta.url), 'utf8'));
  const portable = new WasmEngine(binding, { history: 'all' }, vectors.valid);
  assert.equal(portable.snapshot(1n).rows[0].name, '雪;|😀');
  assert.equal(portable.snapshot().rows[0].index, 4294967295n);
  assert.deepEqual(portable.journal(), vectors.valid); portable.close();
  for (const record of vectors.invalidU32)
    assert.throws(() => new WasmEngine(binding, {}, [record]));

  const primary = new MemoryCommitLog(); const follower = new MemoryCommitLog();
  let partition = true;
  const replica = { read: () => follower.read(), append: (record) => follower.append(record),
    flush() { if (partition) throw Error('partition'); }, supportsDurableFlush: false };
  const replication = new ReplicatedCommitLog(primary, [replica], { acknowledgements: 2 });
  const replicated = new WasmEngine(binding, {}, replication);
  const pending = replicated.begin();
  await pending.create(0n, 0n);
  await assert.rejects(pending.commit(), (error) => error.outcome === 'unknown');
  assert.throws(() => replicated.begin(), /indeterminate/);
  await pending.close(); replicated.close(); partition = false;
  assert.equal(replication.synchronize(), 2);
  const recovered = new WasmEngine(binding, {}, follower);
  assert.equal(recovered.snapshot().rows.length, 1); recovered.close();
  assert.throws(() => new WasmEngine(binding, { requireDurableFlush: true }, primary), /durable/);
  const asynchronousLog = { read: () => '[]', append: () => Promise.resolve(), flush() {} };
  const invalid = new WasmEngine(binding, {}, asynchronousLog);
  const invalidSession = invalid.begin(); await invalidSession.create(0n, 0n);
  await assert.rejects(invalidSession.commit(), /synchronously/);
  await invalidSession.close(); invalid.close();
  // Native bindings must catch throwing capability getters and require a
  // boolean, rather than treating an arbitrary truthy value as durability.
  for (const capability of [1, 'true', null]) {
    const log = { read: () => '[]', append() {}, flush() {}, supportsDurableFlush: capability };
    assert.throws(() => binding.StagedEngine.withLog('{"requireDurableFlush":true}', log), /durable/);
  }
  const throwing = { read: () => '[]', append() {}, flush() {},
    get supportsDurableFlush() { throw Error('capability unavailable'); } };
  assert.throws(() => binding.StagedEngine.withLog('{"requireDurableFlush":true}', throwing), /durable/);
  for (const log of [
    { read: () => { throw 'read failure'; }, append() {}, flush() {} },
    { read: () => '{}', append() {}, flush() {} },
  ]) assert.throws(() => binding.StagedEngine.withLog('{}', log));
  console.log('ok wasm engine: staged requests, commit ordering, isolation, history, conflict, Unicode, rollback, reopen');
}
