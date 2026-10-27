// Finite offline message exchange over separately owned physical directories.
import assert from 'node:assert/strict';
import { openSync, readSync, closeSync, writeFileSync } from 'node:fs';
import { FileLsmJournalStore } from '../../js/client/src/engine-file-lsm-store.js';
import { CausalReplica } from '../../js/client/src/engine-causal-replica.js';
import { CausalVersion } from '../../js/client/src/engine-causal-version.js';
import { CausalMerkleSnapshot, pullCausal } from '../../js/client/src/engine-causal-merkle.js';

const [action, directory, mirrors, messageFile] = process.argv.slice(2);
assert.ok(['export', 'seed', 'pull', 'read-pulled', 'following', 'read-following', 'cut-before', 'cut-after'].includes(action));
assert.ok(['plain', 'mirrored'].includes(mirrors));
const alice = '00000000000000000000000000000001', bob = '00000000000000000000000000000002';
const local = '0000000000000000000000000000000d', token = n => n.toString(16).padStart(32, '0');
const row = name => ({ source: '2', target: '3', name });
const initial = CausalVersion.advance('1', alice, '0', [], row('initial\ufeff雪😀\0'));
const deletion = CausalVersion.advance('1', bob, '0', [initial], null);
const selected = CausalVersion.advance('1', alice, '1', [deletion], row('selected\ufeff雪😀\0'));
const other = CausalVersion.advance('2', alice, '2', [], null);
const concurrent = CausalVersion.advance('1', local, '0', [], row('concurrent'));
const resolved = CausalVersion.advance('1', local, '1', [selected, concurrent], null);
const wires = values => values.map(CausalVersion.encode).sort();
const mirrored = mirrors === 'mirrored';
const physical = new FileLsmJournalStore(directory, { maxKeys: 32, maxBytes: 65536,
  bufferKeys: 16, bufferBytes: 32768, maxRuns: 8, maxPages: 128,
  mirroredRoot: mirrored, mirroredBanks: mirrored, mirroredTables: mirrored });
const owner = new CausalReplica({
  get supportsDurableFlush() { return physical.supportsDurableFlush; },
  read: keys => physical.read(keys), compareExchange: (...args) => {
    if (action === 'cut-before') process.exit(73);
    const result = physical.compareExchange(...args);
    if (action === 'cut-after' && result === true) process.exit(73); return result;
  },
}, 'replica', action === 'export' ? alice : local, { requireDurableFlush: true });
const check = async following => {
  const state = await owner.snapshot();
  assert.equal(state.lastCounter, following ? '2' : '1');
  assert.deepEqual(wires(state.versions), wires(following ? [resolved, other] : [selected, concurrent, other]));
  assert.equal(CausalVersion.encode(await owner.write(token(1), '1', [], row('concurrent'))), CausalVersion.encode(concurrent));
};
try {
  if (action === 'export') {
    const snapshot = await CausalMerkleSnapshot.create((await owner.snapshot()).versions);
    const lines = ['causal-bundle1', snapshot.root], paths = new Set();
    for (const value of [selected, other]) {
      const path = BigInt(value.address).toString(16).padStart(16, '0');
      for (let depth = 0; depth < 16; depth++) paths.add(path.slice(0, depth));
    }
    for (const prefix of [...paths].sort()) lines.push(await CausalMerkleSnapshot.encodeNode(snapshot.node(prefix)));
    for (const value of [selected, other]) lines.push(await CausalMerkleSnapshot.encodeLeaf(snapshot.leaf(value.address)));
    const wire = lines.join('\n') + '\n'; assert.ok(wire.length <= 524288); writeFileSync(messageFile, wire);
  } else if (action === 'seed') {
    assert.equal(CausalVersion.encode(await owner.write(token(1), '1', [], row('concurrent'))), CausalVersion.encode(concurrent));
  } else if (action === 'read-pulled' || action === 'read-following') {
    await check(action === 'read-following');
  } else if (action === 'following') {
    await check(false); const observed = await owner.read('1');
    assert.equal(CausalVersion.encode(await owner.write(token(2), '1', observed, null)), CausalVersion.encode(resolved));
    await check(true);
  } else {
    const buffer = Buffer.alloc(524289), fd = openSync(messageFile, 'r'); let size = 0;
    try { while (size < buffer.length) { const count = readSync(fd, buffer, size, buffer.length - size, null); if (!count) break; size += count; } }
    finally { closeSync(fd); }
    assert.ok(size <= 524288); const lines = buffer.subarray(0, size).toString('utf8').split('\n');
    assert.equal(lines.pop(), ''); assert.equal(lines.shift(), 'causal-bundle1'); const root = lines.shift();
    assert.ok(lines.length <= 40); const nodes = new Map(), leaves = new Map();
    for (const wire of lines) {
      if (wire.startsWith('["causal-node1",')) {
        const value = await CausalMerkleSnapshot.decodeNode(root, wire); assert.ok(!nodes.has(value.prefix)); nodes.set(value.prefix, wire);
      } else {
        const value = await CausalMerkleSnapshot.decodeLeaf(root, wire); assert.ok(!leaves.has(value.address)); leaves.set(value.address, wire);
      }
    }
    const source = { root, node: prefix => CausalMerkleSnapshot.decodeNode(root, nodes.get(prefix)),
      leaf: address => CausalMerkleSnapshot.decodeLeaf(root, leaves.get(address)) };
    let continuation = null, complete = false;
    for (let attempt = 0; attempt < 40; attempt++) {
      const progress = await pullCausal(source, owner, { maxNodeRequests: 4, maxLeafRequests: 1, continuation });
      assert.ok(progress.nodeRequests <= 4 && progress.leafRequests <= 1);
      if (progress.complete) { complete = true; break; } continuation = progress.continuation;
    }
    assert.equal(complete, true); await check(false);
    const retry = await pullCausal(source, owner); assert.equal(retry.changedLeaves, 0); assert.equal(retry.complete, true);
  }
} finally { physical.close(); }
console.log(`PASS JS Merkle native LSM ${action} ${mirrors}`);
