import test from 'node:test';
import assert from 'node:assert/strict';
import { EtcdJournalStore } from '../src/cluster.js';
test('maintenance options fail before I/O', { timeout: 5000 }, async () => {
  const store = new EtcdJournalStore(['http://127.0.0.1:1'], { prefix: '/test/', fetch: () => { throw Error('unexpected I/O'); } });
  await assert.rejects(store.scanNamespace({ maxKeys: 0 }), TypeError);
  await assert.rejects(store.compareDelete('a', '0'), TypeError);
});
