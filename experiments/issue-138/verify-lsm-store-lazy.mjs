// Isolated loader: absence by default plus an explicit-selection positive control.
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
let loads = 0;
const hook = registerHooks({ resolve(specifier, context, nextResolve) {
  if (specifier.endsWith('/engine-lsm-store.js')) { loads++; throw Error('selected physical LSM store'); }
  assert.doesNotMatch(specifier, /sqlite/i);
  return nextResolve(specifier, context);
} });
try {
  const { MemoryCommitLog } = await import('../../js/client/src/index.js');
  const log = new MemoryCommitLog(); log.append('ordinary'); log.flush(); assert.equal(loads, 0);
  await assert.rejects(import('../../js/client/src/engine-lsm-store.js'), /selected physical LSM store/);
  assert.equal(loads, 1);
} finally { hook.deregister(); }
console.log('PASS: ordinary root/memory paths do not load the physical LSM or SQLite; explicit selection reaches the store.');
