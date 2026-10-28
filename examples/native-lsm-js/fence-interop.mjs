// SQL-free physical author ownership across clean runtime handoffs.
import assert from 'node:assert/strict';
import { readdirSync } from 'node:fs';
import { FileLsmJournalStore } from '../../js/client/src/engine-file-lsm-store.js';
import { AuthorFence } from '../../js/client/src/engine-author-fence.js';
const [action, directory, mirrors] = process.argv.slice(2);
assert.ok(['write','read','mutate','read-advanced'].includes(action));
assert.ok(['plain','mirrored'].includes(mirrors));
if (action === 'write') assert.deepEqual(readdirSync(directory), []);
const A = '00000000000000000000000000000001', mirrored = mirrors === 'mirrored';
const claim = epoch => ({author:A, token:(epoch*10).toString(16).padStart(32,'0'), epoch:epoch.toString()});
const keys = ['a','b'], allKeys = ['fence', ...keys];
const rows = phase => phase === 1 ? [['seed-a\0雪'],['seed-b\uFEFF😀']] : [['takeover-a'],['takeover-b']];
const physical = new FileLsmJournalStore(directory, {maxKeys:32,maxBytes:65536,bufferKeys:16,bufferBytes:32768,maxRuns:8,
  maxPages:128,mirroredRoot:mirrored,mirroredBanks:mirrored,mirroredTables:mirrored});
const fence = new AuthorFence(physical, 'fence', A, {requireDurableFlush:true});
async function check(phase) {
  const state = await fence.snapshot();
  assert.equal(state.epoch, phase.toString()); assert.deepEqual(state.claims, Array.from({length:phase}, (_,i) => claim(i+1)));
  assert.deepEqual(await fence.claim((phase-1).toString(), claim(phase).token), claim(phase));
  const before = physical.read(allKeys);
  if (phase === 2) {
    await assert.rejects(fence.claim('0', claim(1).token), /fenced/i); const stale = fence.bind(claim(1));
    await assert.rejects(stale.read(keys), /fenced/i);
    await assert.rejects(stale.compareExchange(keys, before.slice(1), rows(1)), /fenced/i);
  }
  const guarded = fence.bind(claim(phase)), values = await guarded.read(keys);
  assert.deepEqual(values.map(v => v.entries), rows(phase));
  assert.equal(await guarded.compareExchange(keys, values, [null,null]), true);
  assert.deepEqual(physical.read(allKeys), before);
}
try {
  if (action === 'write') {
    assert.deepEqual(await fence.claim('0', claim(1).token), claim(1)); const guarded = fence.bind(claim(1));
    assert.equal(await guarded.compareExchange(keys, keys.map(() => ({generation:'0',entries:[]})), rows(1)), true);
    physical.flush(); physical.compact();
  }
  await check(action === 'read-advanced' ? 2 : 1);
  if (action === 'mutate') {
    assert.deepEqual(await fence.claim('1', claim(2).token), claim(2)); const generation = (await fence.snapshot()).generation;
    const current = fence.bind(claim(2)); assert.equal(await current.compareExchange(keys, await current.read(keys), rows(2)), true);
    assert.equal((await fence.snapshot()).generation, generation); await check(2); physical.flush(); physical.compact();
  }
} finally { physical.close(); }
console.log(`PASS JS author fencing native LSM ${action} ${mirrors}`);
