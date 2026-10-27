// Run with Node, Bun, or Deno (--allow-read --allow-write --allow-env).
// One finite SQL-free directory; this example removes only its own temporary data.
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FileLsmJournalStore } from '../../js/client/src/engine-file-lsm-store.js';
import { CausalBatch } from '../../js/client/src/engine-causal-batch.js';
import { CausalReplica } from '../../js/client/src/engine-causal-replica.js';
import { CausalVersion } from '../../js/client/src/engine-causal-version.js';
import { CausalProjection } from '../../js/client/src/engine-causal-projection.js';

const directory = mkdtempSync(join(tmpdir(), 'causal-projection-cache-'));
const alice = '00000000000000000000000000000001';
const bob = '00000000000000000000000000000002';
const token = value => BigInt(value).toString(16).padStart(32, '0');
let store;
try {
  store = new FileLsmJournalStore(directory);
  const batch = new CausalBatch(store, 'owner', 'receipts', alice, { requireDurableFlush: true });
  const owner = new CausalReplica(store, 'owner', alice, { requireDurableFlush: true });
  await batch.write(token(100), '0', [{ address: '1', observed: [],
    payload: { source: '0', target: '0', name: 'cached row' } }]);
  // A verified remote version arrives independently, concurrent with Alice's row.
  await owner.receive([CausalVersion.advance('1', bob, '0', [], null)]);
  const snapshot = await batch.snapshot();
  assert.throws(() => new CausalProjection(snapshot.versions, 'reject'), /conflict/i);
  const cache = new CausalProjection(snapshot.versions, 'max-dot');
  assert.equal(cache.versions().length, 2); // Losing live value is still retained.
  assert.equal(cache.selected()[0].payload, null);
  assert.deepEqual(cache.rows(), []); // Choose the deletion before visibility.
  await batch.write(token(200), snapshot.generation, [{ address: '1',
    observed: cache.versions(), payload: { source: '0', target: '0', name: 'resolved' } }]);
  const refreshed = new CausalProjection((await batch.snapshot()).versions, 'reject');
  assert.equal(refreshed.rows()[0].name, 'resolved');
  assert.deepEqual(cache.rows(), []); // An owned old view remains a snapshot.
  store.flush();
  console.log('PASS owned causal cache: concurrent deletion, explicit resolution, refreshed rows');
} finally {
  try { store?.close(); }
  finally { rmSync(directory, { recursive: true, force: true }); }
}
