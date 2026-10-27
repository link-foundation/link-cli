import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { LsmWal } from '../src/engine-lsm-wal.js';
const vectors = JSON.parse(readFileSync(new URL('../../../docs/protocol/native-lsm-wal-v1.json', import.meta.url)));
test('independent native-lsm1 WAL vectors reject incomplete or inconsistent replay', () => {
  for (const fixture of vectors.cases) {
    const before = structuredClone(fixture);
    if (fixture.valid) {
      const state = LsmWal.recover(fixture.records, fixture.options);
      const encoded = LsmWal.encode(state, fixture.options);
      assert.deepEqual(JSON.parse(encoded), fixture.checkpoint, fixture.name);
      assert.deepEqual(LsmWal.recover([encoded], fixture.options), state);
    } else assert.throws(() => LsmWal.recover(fixture.records, fixture.options), undefined, fixture.name);
    assert.deepEqual(fixture, before);
  }
});
test('WAL recovery owns payloads and checkpoint encoding validates caller state', () => {
  const fixture = vectors.cases.find(c => c.name === 'unicode-tombstone-empty-and-overwrite');
  const state = LsmWal.recover(fixture.records);
  state.buffer[0].entries[0] = 'caller';
  assert.equal(LsmWal.recover(fixture.records).buffer[0].entries[0], 'new');
  assert.throws(() => LsmWal.encode({ ...state, generation: '1' }));
  assert.throws(() => LsmWal.encode(null));
  assert.throws(() => LsmWal.recover(null));
  assert.throws(() => LsmWal.recover(Array(1)));
  assert.throws(() => LsmWal.recover(['["native-lsm1","0","0","0",[],[]]'], { maxRuns: 1.5 }));
});
