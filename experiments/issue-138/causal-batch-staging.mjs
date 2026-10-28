// Finite trusted-input composition experiment. Not a production provider or
// canonical receipt protocol. It omits admission and public API contracts;
// use js/client/src/engine-causal-batch.js for the selected production component.
import { CausalReplica } from '../../js/client/src/engine-causal-replica.js';
import { CausalVersion } from '../../js/client/src/engine-causal-version.js';

const author = '00000000000000000000000000000001';
const keys = ['replica', 'batch-receipts'];
const maximum = 18446744073709551615n;
const tokenMaximum = 2n ** 128n - 1n;
const clone = value => structuredClone(value);
const fail = message => { throw new Error(message); };

class StagingStore {
  supportsDurableFlush = true;
  constructor(value) {
    this.value = clone(value);
    // The local generation is private. Only the original two-key snapshot is
    // ever used to condition the real publication; no external counter is reused.
    if (this.value.generation !== '0') this.value.generation = '1';
  }
  async read(requested) {
    if (JSON.stringify(requested) !== JSON.stringify([keys[0]])) fail('unexpected staging key');
    return [clone(this.value)];
  }
  async compareExchange(requested, expected, updates) {
    if (JSON.stringify(requested) !== JSON.stringify([keys[0]])) fail('unexpected staging key');
    if (expected[0].generation !== this.value.generation) return false;
    this.value = { generation: (BigInt(this.value.generation) + 1n).toString(), entries: [...updates[0]] };
    return true;
  }
}

export async function publishBatch(store, token, generation, edits) {
  if (!/^[0-9a-f]{32}$/.test(token) || BigInt(`0x${token}`) === 0n ||
      !Array.isArray(edits) || edits.length > 3) fail('finite experiment admission');
  const baseToken = BigInt(`0x${token}`);
  if (baseToken + BigInt(Math.max(edits.length - 1, 0)) > tokenMaximum) fail('token range exhausted');
  for (let index = 1; index < edits.length; index++)
    if (BigInt(edits[index - 1].address) >= BigInt(edits[index].address)) fail('unordered edit addresses');
  edits = clone(edits);
  const input = JSON.stringify([generation, edits]);
  const expected = await store.read(keys);
  const staging = new StagingStore(expected[0]);
  const owner = new CausalReplica(staging, keys[0], author);
  const state = await owner.snapshot();
  const receiptEntries = [...expected[1].entries];
  const receipts = receiptEntries.slice(1).map(JSON.parse);
  const receipt = receipts.find(value => value[1] === token);
  if (receipt && receipt[2] !== input) fail('changed retry request');
  if (!receipt && expected[0].generation !== generation) fail('conflict: stale base');
  const beforeCounter = receipt ? BigInt(receipt[3]) : BigInt(state.lastCounter);
  if (beforeCounter + BigInt(edits.length) > maximum) fail('counter exhausted');
  const beforeEntries = JSON.stringify(staging.value.entries);
  const result = [];
  for (let index = 0; index < edits.length; index++) {
    const edit = edits[index];
    const childToken = (baseToken + BigInt(index)).toString(16).padStart(32, '0');
    const value = await owner.write(childToken, edit.address, edit.observed, edit.payload);
    if (BigInt(value.dot.counter) !== beforeCounter + BigInt(index) + 1n) fail('child-token collision');
    result.push(value);
  }
  const encoded = result.map(CausalVersion.encode);
  if (receipt) {
    if (beforeEntries !== JSON.stringify(staging.value.entries) || JSON.stringify(encoded) !== JSON.stringify(receipt[4]))
      fail('receipt has no matching owner publication');
    return clone(result);
  }
  if (staging.value.entries.length === 0)
    staging.value.entries.push(JSON.stringify(['causal-replica1', author]));
  if (receiptEntries.length === 0) receiptEntries.push(JSON.stringify(['batch-draft1', keys[0], author]));
  receiptEntries.push(JSON.stringify(['batch-draft1', token, input, beforeCounter.toString(), encoded]));
  let committed;
  try { committed = await store.compareExchange(keys, expected, [staging.value.entries, receiptEntries]); }
  catch (cause) { throw new Error('indeterminate batch publication', { cause }); }
  if (committed === false) fail('conflict: batch state changed');
  if (committed !== true) fail('indeterminate batch publication');
  return clone(result);
}
