// Prepared before the production inspection API; finite empty image control.
import assert from 'node:assert/strict';
import test from 'node:test';
import { CheckpointPagedCommitLog } from '../../js/client/src/engine-checkpoint-pages.js';
import { Device } from '../../js/client/test/support/sorted-table.mjs';
test('checkpoint inspection leaves empty exclusive devices uninitialized', () => {
  const all = [new Device(), new Device(), new Device()];
  const before = structuredClone(all.map(device => device.live));
  for (const device of all) device.newDatabaseId = () => { throw Error('unexpected identity allocation'); };
  assert.deepEqual(CheckpointPagedCommitLog.inspect(all[0], all.slice(1), { maxPages:128 }), []);
  assert.deepEqual(all.map(device => device.live), before);
  assert.ok(all.every(device => device.calls.length === 0));
});
