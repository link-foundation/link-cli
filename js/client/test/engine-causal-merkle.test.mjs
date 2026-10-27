import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { CausalVersion } from '../src/engine-causal-version.js';
import { CausalMerkleSnapshot, CausalPullCursor, pullCausal } from '../src/engine-causal-merkle.js';
import { CausalReplica } from '../src/engine-causal-replica.js';

const fixtures = JSON.parse(readFileSync(new URL('../../../docs/protocol/causal-merkle-v1.json', import.meta.url)));
const value = item => CausalVersion.decode(JSON.stringify(item));
const values = items => items.map(value);
const plainLeaf = item => ({ root: item.root, address: item.address, versions: values(item.versions), proof: item.proof });
const plainNode = item => ({ root: item.root, prefix: item.prefix, children: item.children, proof: item.proof });
const token = n => n.toString(16).padStart(32, '0');
class MemoryStore {
  generation = 0n; entries = []; calls = 0; fault;
  async read() { return [{ generation: this.generation.toString(), entries: [...this.entries] }]; }
  async compareExchange(keys, expected, updates) {
    this.calls++;
    if (this.fault === 'before') { this.fault = undefined; throw new Error('before'); }
    if (expected[0].generation !== this.generation.toString()) return false;
    this.generation++; this.entries = [...updates[0]];
    if (this.fault === 'after') { this.fault = undefined; throw new Error('after'); }
    return true;
  }
}
const owner = (store = new MemoryStore()) => new CausalReplica(store, 'replica', '0000000000000000000000000000000e');
const sorted = versions => versions.map(CausalVersion.encode).sort();

test('independent canonical roots, node proofs and leaf proofs agree', async () => {
  const snapshots = [];
  for (const item of fixtures.snapshots) {
    const snapshot = await CausalMerkleSnapshot.create(values(item.versions));
    assert.equal(snapshot.root, item.root); snapshots.push(snapshot);
    assert.equal((await CausalMerkleSnapshot.verifyNode(item.root, snapshot.node(''))).length, 16);
  }
  for (const item of fixtures.leaves) {
    assert.deepEqual(snapshots[item.snapshot].leaf(item.address), plainLeaf(item));
    assert.deepEqual(await CausalMerkleSnapshot.verifyLeaf(item.root, plainLeaf(item)), values(item.versions));
    assert.equal(await CausalMerkleSnapshot.encodeLeaf(plainLeaf(item)), item.wire);
    assert.deepEqual(await CausalMerkleSnapshot.decodeLeaf(item.root, item.wire), plainLeaf(item));
  }
  for (const item of fixtures.nodes) {
    assert.deepEqual(snapshots[item.snapshot].node(item.prefix), plainNode(item));
    assert.deepEqual(await CausalMerkleSnapshot.verifyNode(item.root, plainNode(item)), item.children);
    assert.equal(await CausalMerkleSnapshot.encodeNode(plainNode(item)), item.wire);
    assert.deepEqual(await CausalMerkleSnapshot.decodeNode(item.root, item.wire), plainNode(item));
  }
});

test('snapshots own input before hashing and return independently owned messages', async () => {
  const input = values(fixtures.snapshots[3].versions);
  const pending = CausalMerkleSnapshot.create(input);
  input[0].payload.name = 'changed'; input.length = 0;
  const snapshot = await pending;
  assert.equal(snapshot.root, fixtures.snapshots[3].root);
  const leaf = snapshot.leaf('1'), node = snapshot.node('');
  leaf.versions[0].clock[0].counter = '99'; leaf.proof[0][0] = '0'.repeat(64);
  node.children[0] = '0'.repeat(64); node.proof.push([]);
  assert.deepEqual(await CausalMerkleSnapshot.verifyLeaf(snapshot.root, snapshot.leaf('1')), values(fixtures.snapshots[3].versions));
  await CausalMerkleSnapshot.verifyNode(snapshot.root, snapshot.node(''));
  const sending = snapshot.leaf('1'), originalWire = await CausalMerkleSnapshot.encodeLeaf(sending);
  const encoded = CausalMerkleSnapshot.encodeLeaf(sending);
  sending.versions.length = 0; sending.proof.length = 0;
  assert.equal(await encoded, originalWire);
  const sendingNode = snapshot.node(''), originalNodeWire = await CausalMerkleSnapshot.encodeNode(sendingNode);
  const encodedNode = CausalMerkleSnapshot.encodeNode(sendingNode);
  sendingNode.children.length = 0; assert.equal(await encodedNode, originalNodeWire);
  assert.throws(() => { snapshot.root = '0'.repeat(64); });
});

test('root-bound verification rejects mixed, malformed and changed responses', async () => {
  const leaf = plainLeaf(fixtures.leaves[1]), node = plainNode(fixtures.nodes[1]);
  const changes = [
    response => { response.root = '0'.repeat(64); },
    response => { response.address = '0'; },
    response => { response.address = '01'; },
    response => { response.address = '18446744073709551616'; },
    response => { response.address = '2'; },
    response => { response.versions.reverse(); },
    response => { response.versions.push(response.versions[0]); },
    response => { response.versions[0].payload.name = 'forged'; },
    response => { response.proof.pop(); },
    response => { response.proof[0].pop(); },
    response => { response.proof[0][0] = '0'.repeat(64); },
    response => { response.proof[0][0] = 'A'.repeat(64); },
  ];
  for (const change of changes) {
    const response = structuredClone(leaf); change(response);
    await assert.rejects(CausalMerkleSnapshot.verifyLeaf(leaf.root, response));
  }
  for (const change of [
    response => { response.root = leaf.root; },
    response => { response.prefix = 'F'; },
    response => { response.prefix = '0'.repeat(16); },
    response => { response.prefix = '1' + response.prefix.slice(1); },
    response => { response.children.pop(); },
    response => { response.children[0] = '0'.repeat(64); },
    response => { response.children[0] = 'bad'; },
    response => { response.proof.pop(); },
  ]) {
    const response = structuredClone(node); change(response);
    await assert.rejects(CausalMerkleSnapshot.verifyNode(node.root, response));
  }
  await assert.rejects(CausalMerkleSnapshot.verifyLeaf('bad', leaf));
  await assert.rejects(CausalMerkleSnapshot.verifyNode('bad', node));
  await assert.rejects(CausalMerkleSnapshot.verifyLeaf(fixtures.snapshots[0].root, leaf));
  for (const wire of ['', 'null', '[]', '{}', '["other"]', fixtures.leaves[1].wire + ' ',
    fixtures.leaves[1].wire.replace('causal-leaf1', 'causal-leaf2'),
    fixtures.leaves[1].wire.replace('"1"', '1'),
    fixtures.leaves[1].wire.replace('causal-leaf1', 'causal-\\u006ceaf1')]) {
    await assert.rejects(CausalMerkleSnapshot.decodeLeaf(leaf.root, wire));
  }
  for (const wire of ['', '[]', '{}', fixtures.nodes[1].wire + ' ', fixtures.nodes[1].wire.replace('causal-node1', 'causal-node2')])
    await assert.rejects(CausalMerkleSnapshot.decodeNode(node.root, wire));
  for (const replacement of [null, Array(65).fill(fixtures.leaves[1].versions[0])]) {
    const array = JSON.parse(fixtures.leaves[1].wire); array[3] = replacement;
    await assert.rejects(CausalMerkleSnapshot.decodeLeaf(leaf.root, JSON.stringify(array)));
  }
  for (const replacement of [null, []]) {
    const array = JSON.parse(fixtures.nodes[1].wire); array[3] = replacement;
    await assert.rejects(CausalMerkleSnapshot.decodeNode(node.root, JSON.stringify(array)));
    await assert.rejects(CausalMerkleSnapshot.encodeNode({ ...node, children: replacement }));
  }
  await assert.rejects(CausalMerkleSnapshot.encodeNode(null));
  await assert.rejects(CausalMerkleSnapshot.encodeLeaf(null));
  const parse = JSON.parse;
  try {
    JSON.parse = () => { assert.fail('inadmissible Merkle frame reached JSON.parse'); };
    for (const wire of [null, 'x'.repeat(1081345), '['.repeat(6) + ']'.repeat(6), '["\\\\"]', '["雪"]',
      '[{}]', ']', '[', '"', '["\n"]', '[' + ','.repeat(32769) + ']']) {
      await assert.rejects(CausalMerkleSnapshot.decodeLeaf(leaf.root, wire), error => !error.message.includes('reached JSON.parse'));
    }
    await assert.rejects(CausalMerkleSnapshot.decodeNode(node.root, 'x'.repeat(32769)), error => !error.message.includes('reached JSON.parse'));
  } finally { JSON.parse = parse; }
});

test('finite admission, canonical leaves, tombstones and identity checks precede hashing', async () => {
  assert.throws(() => new CausalMerkleSnapshot());
  await assert.rejects(CausalMerkleSnapshot.create([], null));
  await assert.rejects(CausalMerkleSnapshot.verifyNode(fixtures.snapshots[0].root, null));
  await assert.rejects(CausalMerkleSnapshot.verifyLeaf(fixtures.snapshots[0].root, []));
  const original = value(fixtures.snapshots[1].versions[0]);
  const old = CausalVersion.advance('1', original.dot.author, '0', [], original.payload);
  const newer = CausalVersion.advance('1', original.dot.author, '1', [old], null);
  const snapshot = await CausalMerkleSnapshot.create([old, newer, old]);
  assert.deepEqual(snapshot.leaf('1').versions, [newer]);
  await CausalMerkleSnapshot.verifyLeaf(snapshot.root, snapshot.leaf('1'));
  const contradiction = structuredClone(original); contradiction.payload.name = 'different';
  await assert.rejects(CausalMerkleSnapshot.create([original, contradiction]));
  const reused = structuredClone(original); reused.address = '2';
  await assert.rejects(CausalMerkleSnapshot.create([original, reused]));
  for (const options of [{ maxVersions: 0 }, { maxVersions: 4097 }, { maxVersions: 1.5 },
    { maxBytes: 0 }, { maxBytes: 8388609 }, { maxBytes: 1.5 }]) {
    await assert.rejects(CausalMerkleSnapshot.create([], options));
  }
  await assert.rejects(CausalMerkleSnapshot.create([original, original], { maxVersions: 1 }));
  await assert.rejects(CausalMerkleSnapshot.create([original], { maxBytes: 1 }));
  await assert.rejects(CausalMerkleSnapshot.create(Array(65).fill(original)));
  await assert.rejects(CausalMerkleSnapshot.create(null));
  assert.throws(() => snapshot.leaf('0'));
  assert.throws(() => snapshot.node('x'));
  assert.throws(() => snapshot.node('0'.repeat(16)));
  const response = snapshot.leaf('1'); response.versions.unshift(old);
  await assert.rejects(CausalMerkleSnapshot.verifyLeaf(snapshot.root, response));
});

test('bounded pull matches direct causal union for all 64 independent snapshot pairs', async () => {
  for (const sourceCase of fixtures.snapshots) for (const destinationCase of fixtures.snapshots) {
    const source = await CausalMerkleSnapshot.create(values(sourceCase.versions));
    const store = new MemoryStore(), destination = owner(store);
    await destination.receive(values(destinationCase.versions));
    const progress = await pullCausal(source, destination);
    assert.equal(progress.root, source.root); assert.equal(progress.complete, true);
    const groups = new Map();
    for (const value of values([...sourceCase.versions, ...destinationCase.versions])) {
      if (!groups.has(value.address)) groups.set(value.address, []); groups.get(value.address).push(value);
    }
    const expected = [...groups.values()].flatMap(group => CausalVersion.merge(group, []));
    assert.deepEqual(sorted((await destination.snapshot()).versions), sorted(expected));
    const published = store.calls;
    const retry = await pullCausal(source, destination);
    assert.equal(retry.changedLeaves, 0); assert.equal(retry.complete, true); assert.equal(store.calls, published);
    if (sourceCase.root === destinationCase.root || sourceCase.versions.length === 0) {
      assert.equal(progress.nodeRequests, 0); assert.equal(progress.leafRequests, 0);
    }
  }
});

test('bounded attempts resume from fresh state and retain completed work after transport failure', async () => {
  const source = await CausalMerkleSnapshot.create(values(fixtures.snapshots[5].versions));
  const store = new MemoryStore(), destination = owner(store);
  const short = await pullCausal(source, destination, { maxNodeRequests: 1 });
  assert.equal(short.complete, false); assert.equal(short.nodeRequests, 1); assert.equal(short.leafRequests, 0);
  assert.equal(store.calls, 0);
  const tiny = await pullCausal(source, destination, { maxVersionBytes: 1 });
  assert.equal(tiny.complete, false); assert.equal(tiny.versionBytes, 0); assert.equal(store.calls, 0);
  const first = await pullCausal(source, destination, { maxLeafRequests: 1 });
  assert.equal(first.complete, false); assert.equal(first.changedLeaves, 1);
  let reads = 0;
  await assert.rejects(pullCausal({ root: source.root, node: prefix => source.node(prefix), leaf: key => {
    if (++reads === 2) throw new Error('transport interruption'); return source.leaf(key);
  } }, destination), /transport interruption/);
  assert.equal((await destination.snapshot()).versions.length, 2);
  assert.equal((await pullCausal(source, destination)).complete, true);
  assert.deepEqual(sorted((await destination.snapshot()).versions), sorted(values(fixtures.snapshots[5].versions)));
  for (const options of [{ maxNodeRequests: 0 }, { maxNodeRequests: 65537 }, { maxLeafRequests: 0 },
    { maxLeafRequests: 4097 }, { maxVersionBytes: 0 }, { maxVersionBytes: 8388609 }, null]) {
    await assert.rejects(pullCausal(source, destination, options));
  }
  await assert.rejects(pullCausal(null, destination));
  await assert.rejects(pullCausal({ root: source.root }, destination));
  await assert.rejects(pullCausal(source, {}));
});

test('concurrent destination writes and lost publication replies preserve causal union', async () => {
  const source = await CausalMerkleSnapshot.create(values(fixtures.snapshots[1].versions));
  const store = new MemoryStore(), destination = owner(store);
  const concurrent = { source: '0', target: '0', name: 'concurrent' };
  let changed = false;
  await pullCausal({ root: source.root, node: async prefix => {
    if (!changed) { changed = true; await destination.write(token(1), '1', [], concurrent); }
    return source.node(prefix);
  }, leaf: key => source.leaf(key) }, destination);
  assert.equal((await destination.read('1')).length, 2);
  assert.ok((await destination.read('1')).some(value => value.payload?.name === 'concurrent'));
  for (const phase of ['before', 'after']) {
    const failedStore = new MemoryStore(), failedOwner = owner(failedStore); failedStore.fault = phase;
    await assert.rejects(pullCausal(source, failedOwner), /indeterminate/);
    const reopened = owner(failedStore), count = failedStore.calls;
    assert.equal((await pullCausal(source, reopened)).complete, true);
    assert.equal(failedStore.calls, count + (phase === 'before' ? 1 : 0));
    await reopened.write(token(1), '2', [], null);
    assert.equal((await reopened.snapshot()).lastCounter, '1');
  }
  const malformedStore = new MemoryStore(), malformedOwner = owner(malformedStore);
  await assert.rejects(pullCausal(source, { snapshot: () => malformedOwner.snapshot(), receive: async versions => {
    await malformedOwner.receive(versions); return 'malformed';
  } }));
  const calls = malformedStore.calls;
  await pullCausal(source, malformedOwner); assert.equal(malformedStore.calls, calls);
});

test('transfer verifies requested root, path and complete leaf before local publication', async () => {
  const source = await CausalMerkleSnapshot.create(values(fixtures.snapshots[1].versions));
  const other = await CausalMerkleSnapshot.create(values(fixtures.snapshots[4].versions));
  for (const remote of [
    { root: source.root, node: () => other.node(''), leaf: key => source.leaf(key) },
    { root: source.root, node: () => source.node('0'), leaf: key => source.leaf(key) },
    { root: source.root, node: prefix => source.node(prefix), leaf: () => source.leaf('2') },
    { root: source.root, node: prefix => source.node(prefix), leaf: key => other.leaf(key) },
    { root: source.root, node: prefix => source.node(prefix), leaf: key => {
      const response = source.leaf(key); response.versions[0].payload.name = 'forged'; return response;
    } },
  ]) {
    const store = new MemoryStore(); await assert.rejects(pullCausal(remote, owner(store))); assert.equal(store.calls, 0);
  }
});

test('bounded continuation reaches every address despite a concurrent first leaf', async () => {
  const source = await CausalMerkleSnapshot.create(values(fixtures.snapshots[5].versions));
  const store = new MemoryStore(), destination = owner(store);
  const concurrent = await destination.write(token(1), '1', [], { source: '0', target: '0', name: 'local' });
  const paused = await pullCausal(source, destination, { maxNodeRequests: 1 });
  const cursor = paused.continuation;
  assert.ok(cursor instanceof CausalPullCursor); assert.equal(cursor.root, source.root);
  assert.throws(() => new CausalPullCursor()); assert.throws(() => { cursor.root = '0'.repeat(64); });
  assert.throws(() => { cursor.resume = () => []; });
  assert.throws(() => { CausalPullCursor.prototype.resume = () => []; });
  assert.throws(() => Object.defineProperty(cursor, 'root', { value: '0'.repeat(64) }));
  const paths = cursor.resume(source.root, '0000000000000000000000000000000e');
  paths.length = 0;
  assert.throws(() => cursor.resume('0'.repeat(64), '0000000000000000000000000000000e'));
  for (const invalidCursor of [{ root: source.root }, Object.create(CausalPullCursor.prototype)])
    await assert.rejects(pullCausal(source, destination, { continuation: invalidCursor }));
  const other = await CausalMerkleSnapshot.create([]); let read = false;
  await assert.rejects(pullCausal(other, { snapshot() { read = true; throw Error('read'); }, receive() {} }, { continuation: cursor }));
  assert.equal(read, false, 'changed root is rejected before a destination read');
  const foreign = new CausalReplica(new MemoryStore(), 'replica', '0000000000000000000000000000000f');
  await assert.rejects(pullCausal(source, foreign, { continuation: cursor }));
  let progress = paused;
  for (let attempt = 0; attempt < 128; attempt++) {
    progress = await pullCausal(source, destination, {
      maxNodeRequests: 1, maxLeafRequests: 1, continuation: progress?.continuation,
    });
    assert.ok(progress.nodeRequests <= 1); assert.ok(progress.leafRequests <= 1);
    if (progress.complete) break;
  }
  assert.equal(progress.complete, true, 'finite retries must advance the pending traversal');
  assert.equal(progress.continuation, null);
  assert.equal((await pullCausal(source, destination, { continuation: cursor })).complete, true);
  const identical = owner(); await identical.receive(values(fixtures.snapshots[5].versions));
  assert.equal((await pullCausal(source, identical, { continuation: cursor })).continuation, null);
  const leafLimited = owner();
  await leafLimited.write(token(1), '1', [], { source: '0', target: '0', name: 'local' });
  let leafProgress;
  for (let attempt = 0; attempt < 8; attempt++) {
    leafProgress = await pullCausal(source, leafLimited, { maxLeafRequests: 1, continuation: leafProgress?.continuation });
    assert.ok(leafProgress.leafRequests <= 1); if (leafProgress.complete) break;
  }
  assert.equal(leafProgress.complete, true);
  assert.deepEqual(sorted((await leafLimited.snapshot()).versions), sorted([...values(fixtures.snapshots[5].versions), concurrent]));
  assert.deepEqual(sorted((await destination.snapshot()).versions), sorted([...values(fixtures.snapshots[5].versions), concurrent]));
});
