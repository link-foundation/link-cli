// Fresh loader: selecting old/default stores must not allocate the new strategy.
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
let selectedLoads = 0; let requests = 0;
const hook = registerHooks({ resolve(specifier, context, nextResolve) {
  if (/\/reclaiming-(store|codec|collector)\.js$/.test(specifier)) {
    selectedLoads++; throw Error('selected reclamation module load');
  }
  return nextResolve(specifier, context);
} });
try {
  const { EtcdJournalStore, PartitionedJournalStore, ReclaimingPartitionedJournalStore } = await import('../../js/client/src/index.js');
  const provider = new EtcdJournalStore(['http://localhost:1'], { fetch: async (_url, request) => {
    requests++; const body = JSON.parse(request.body);
    return new Response(JSON.stringify(body.compare ? { header: { revision: '2' }, succeeded: true }
      : { succeeded: true, responses: body.success.map(() => ({ response_range: {} })) }));
  } });
  const initial = await provider.read(['a']); assert.equal(await provider.compareExchange(['a'], initial, [['ordinary']]), true);
  const legacy = new PartitionedJournalStore(provider, { east: provider }, () => 'east');
  assert.deepEqual(await legacy.read(['a']), [{ generation: '0', entries: [] }]);
  assert.equal(selectedLoads, 0); assert.equal(requests, 3);
  await assert.rejects(ReclaimingPartitionedJournalStore.open(provider, { east: provider }, () => 'east'), /selected reclamation module load/);
  assert.equal(selectedLoads, 1); assert.equal(requests, 3);
} finally { hook.deregister(); }
console.log('PASS lazy reclamation: ordinary coordinator and blob1 reads load no selected helpers; explicit selection loads before I/O.');
