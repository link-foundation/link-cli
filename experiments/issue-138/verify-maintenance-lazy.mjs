// A fresh module loader proves ordinary coordinator calls do not load codecs.
import assert from 'node:assert/strict';
import { registerHooks } from 'node:module';
let selectedLoads = 0; let requests = 0;
const hook = registerHooks({
  resolve(specifier, context, nextResolve) {
    if (specifier.endsWith('/cluster-maintenance.js')) {
      selectedLoads++;
      throw Error('selected maintenance module load');
    }
    return nextResolve(specifier, context);
  },
});
try {
  const { EtcdJournalStore } = await import('../../js/client/src/index.js');
  const store = new EtcdJournalStore(['http://localhost:1'], { fetch: async (_url, request) => {
    requests++; const body = JSON.parse(request.body);
    return new Response(JSON.stringify(body.compare
      ? { header: { revision: '2' }, succeeded: true }
      : { succeeded: true, responses: [{ response_range: {} }] }));
  } });
  const initial = await store.read(['a']);
  assert.deepEqual(initial, [{ generation: '0', entries: [] }]);
  assert.equal(await store.compareExchange(['a'], initial, [['ordinary journal']]), true);
  assert.equal(selectedLoads, 0); assert.equal(requests, 2);
  await assert.rejects(store.scanNamespace(), /selected maintenance module load/);
  assert.equal(selectedLoads, 1); assert.equal(requests, 2);
} finally { hook.deregister(); }
console.log('PASS lazy maintenance: root import and ordinary read/CAS do not load selected codecs; positive selection control loads them before I/O.');
