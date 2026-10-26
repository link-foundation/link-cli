import assert from 'node:assert/strict';
import test from 'node:test';
import { ClusterEngine } from '../src/cluster.js';

test('checkpoint interval is independent, positive and checked before allocating work', () => {
  const store = { read() {}, compareExchange() {} };
  for (const journalCheckpointInterval of [0, -1, NaN, Infinity, 2 ** 53, null, 1n, '1'])
    assert.throws(() => new ClusterEngine({}, store, { journalCheckpointInterval }), TypeError);
});

test('checkpoint publication detects changed work before compaction and preserves cleanup', async () => {
  for (const mode of ['default', 'not-due', 'due', 'read-only', 'failure']) {
    const events = []; let updates;
    const store = { read: async () => [{ generation: '4', entries: ['old'] }],
      compareExchange: async (keys, expected, changed) => { events.push('CAS'); assert.deepEqual(keys, ['a']); assert.equal(expected[0].generation, '4'); updates = changed; return true; } };
    class Native {
      constructor(options) { assert.deepEqual(JSON.parse(options), { history: 2, requireDurableFlush: false }); }
      begin() { return { request() {}, commit() {}, rollback() {}, free() { events.push('session'); } }; }
      journal() { return JSON.stringify(mode === 'read-only' ? ['old'] : ['old', 'new']); }
      snapshot() { events.push('snapshot'); return '{"revision":"2","rows":[]}'; }
      exportCheckpoint() { events.push('export'); if (mode === 'failure') throw Error('cannot export'); return '["checkpoint"]'; }
      free() { events.push('engine'); }
    }
    const interval = mode === 'default' ? {} : { journalCheckpointInterval: mode === 'not-due' ? 3 : 2 };
    const tx = await new ClusterEngine({ StagedEngine: Native }, store, { history: 2, ...interval }).begin(['a']);
    if (mode === 'failure') await assert.rejects(tx.commit(), /cannot export/);
    else {
      await tx.commit();
      assert.deepEqual(updates, mode === 'read-only' ? [null] : [mode === 'due' ? ['checkpoint'] : ['old', 'new']]);
    }
    assert.equal(events.includes('export'), mode === 'due' || mode === 'failure');
    assert.equal(events.includes('snapshot'), !['default', 'read-only'].includes(mode));
    assert.equal(events.includes('CAS'), mode !== 'failure');
    assert.deepEqual(events.slice(-2), ['session', 'engine']);
    await assert.rejects(tx.commit(), /closed/);
    if (mode === 'failure') await assert.rejects(tx.close(), /cannot export/);
    else await tx.close();
  }
});
