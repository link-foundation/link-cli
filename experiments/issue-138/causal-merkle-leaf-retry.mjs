// Eight finite attempts, bounded heap via the invocation in the receipt.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { CausalReplica } from '../../js/client/src/engine-causal-replica.js';
import { CausalVersion } from '../../js/client/src/engine-causal-version.js';
const { CausalMerkleSnapshot, pullCausal } = await import(process.env.CAUSAL_MERKLE_MODULE ?? '../../js/client/src/engine-causal-merkle.js');
const fixtures = JSON.parse(readFileSync(new URL('../../docs/protocol/causal-merkle-v1.json', import.meta.url)));
const source = await CausalMerkleSnapshot.create(fixtures.snapshots[5].versions.map(v => CausalVersion.decode(JSON.stringify(v))));
const store = { generation: 0n, entries: [], calls: 0,
  async read() { return [{ generation: this.generation.toString(), entries: [...this.entries] }]; },
  async compareExchange(keys, expected, updates) {
    this.calls++; assert.equal(expected[0].generation, this.generation.toString());
    this.generation++; this.entries = [...updates[0]]; return true;
  } };
const destination = new CausalReplica(store, 'replica', '0000000000000000000000000000000e');
await destination.write('00000000000000000000000000000001', '1', [], { source: '0', target: '0', name: 'local' });
let progress;
for (let attempt = 0; attempt < 8; attempt++) {
  progress = await pullCausal(source, destination, { maxLeafRequests: 1, continuation: progress?.continuation });
  console.log(JSON.stringify({ attempt, leafRequests: progress.leafRequests, changedLeaves: progress.changedLeaves,
    complete: progress.complete, visibleVersions: (await destination.snapshot()).versions.length }));
  if (progress.complete) break;
}
assert.equal(progress.complete, true, 'leaf-only retries must advance past the concurrent first address');
assert.equal((await destination.snapshot()).versions.length, fixtures.snapshots[5].versions.length + 1);
