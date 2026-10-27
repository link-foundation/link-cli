// Selected local batches. Both journals publish through one coherent store CAS.
import { CausalReplica } from './engine-causal-replica.js';
import { CausalVersion } from './engine-causal-version.js';
import { LinksError } from './errors.js';

const invalid = () => { throw new TypeError('invalid causal batch state'); };
const maximum = 18446744073709551615n;
const tokenMaximum = 2n ** 128n - 1n;
const number = value => {
  if (typeof value !== 'string' || value.length > 20 || !/^(0|[1-9][0-9]*)$/.test(value) ||
      BigInt(value) > maximum) invalid();
  return BigInt(value);
};
const identity = value => {
  if (typeof value !== 'string' || !/^[0-9a-f]{32}$/.test(value) || value === '0'.repeat(32)) invalid();
  return BigInt(`0x${value}`);
};
const child = (token, index) => (token + BigInt(index)).toString(16).padStart(32, '0');
const raw = value => JSON.parse(CausalVersion.encode(value));
const own = value => CausalVersion.decode(CausalVersion.encode(value));
const normalized = edits => {
  if (!Array.isArray(edits) || edits.length > 64) invalid();
  let previous = 0n;
  return Array.from(edits, edit => {
    if (edit === null || typeof edit !== 'object' || number(edit.address) <= previous ||
        !Array.isArray(edit.observed) || edit.observed.length > 64) invalid();
    previous = BigInt(edit.address);
    const observed = Array.from(edit.observed, own), context = CausalVersion.merge(observed, []);
    const mutation = own(CausalVersion.advance(edit.address, '00000000000000000000000000000001', '0', [], edit.payload));
    return { observed, payload: mutation.payload, encoded: [edit.address, context.map(raw), raw(mutation)[4]] };
  });
};
const range = (token, count) => {
  if (token + BigInt(Math.max(count - 1, 0)) > tokenMaximum) invalid();
};

class StagingStore {
  supportsDurableFlush = true;
  constructor(key, value) {
    this.key = key;
    this.value = { generation: value.generation === '0' ? '0' : '1', entries: [...value.entries] };
  }
  async read() { return [{ generation: this.value.generation, entries: [...this.value.entries] }]; }
  async compareExchange(keys, expected, updates) {
    // This provider is private to one synchronous chain of awaited owner writes.
    this.value = { generation: (BigInt(expected[0].generation) + 1n).toString(), entries: [...updates[0]] };
    return true;
  }
}

/** Caller-owned store; reserve two distinct keys for one author lineage.
 * Complete owner and receipt journals are bounded and replayed on every call.
 * Remote per-row exchange does not preserve local batch atomicity. */
export class CausalBatch {
  #store; #keys; #author; #options;
  constructor(store, ownerKey, receiptKey, author, options = {}) {
    // Reuse the existing owner's validation and durability contract.
    new CausalReplica(store, ownerKey, author, options);
    if (typeof receiptKey !== 'string' || !/^[A-Za-z0-9_.-]{1,128}$/.test(receiptKey) || receiptKey === ownerKey) invalid();
    const { maxEntries = 4096, maxBytes = 8388608, requireDurableFlush = false } = options;
    this.#store = store; this.#keys = [ownerKey, receiptKey]; this.#author = author;
    this.#options = { maxEntries, maxBytes, requireDurableFlush };
  }
  #admit(entries) {
    if (!Array.isArray(entries) || entries.length > this.#options.maxEntries) invalid();
    let bytes = 0;
    for (const entry of entries) {
      if (typeof entry !== 'string' || entry.length > 1048576 || (bytes += entry.length) > this.#options.maxBytes) invalid();
      let depth = 0, separators = 0, quoted = false;
      for (const char of entry) {
        if (char < ' ' || char > '~' || char === '\\') invalid();
        if (char === '"') { quoted = !quoted; continue; }
        if (quoted) continue;
        if (char === '{' || char === '}') invalid();
        if (char === '[') { if (++depth > 7) invalid(); }
        else if (char === ']') { if (--depth < 0) invalid(); }
        else if (char === ',' && ++separators > 32768) invalid();
      }
      if (quoted || depth !== 0) invalid();
    }
  }
  async #load() {
    const read = await this.#store.read([...this.#keys]);
    if (!Array.isArray(read) || read.length !== 2) invalid();
    const expected = read.map(value => {
      if (value === null || typeof value !== 'object') invalid();
      number(value.generation); this.#admit(value.entries);
      return { generation: value.generation, entries: [...value.entries] };
    });
    const staging = new StagingStore(this.#keys[0], expected[0]);
    const owner = new CausalReplica(staging, this.#keys[0], this.#author, this.#options);
    const snapshot = await owner.snapshot(), receipts = new Map(), writes = new Map(), used = new Set();
    // Owner replay validates all raw evidence before any indexed receipt is trusted.
    for (const text of expected[0].entries.slice(1)) {
      const entry = JSON.parse(text);
      if (entry[0] === 'causal-write1') {
        const value = CausalVersion.decode(JSON.stringify(entry[3]));
        const context = CausalVersion.merge(entry[2].map(value => CausalVersion.decode(JSON.stringify(value))), []);
        writes.set(entry[1], { value, encoded: [value.address, context.map(raw), entry[3][4]] });
      }
    }
    const metadata = expected[1];
    if (metadata.generation === '0') { if (metadata.entries.length !== 0) invalid(); }
    else {
      if (expected[0].generation === '0' || metadata.entries.length === 0 ||
          metadata.entries[0] !== JSON.stringify(['causal-batches1', this.#keys[0], this.#author])) invalid();
      for (const text of metadata.entries.slice(1)) {
        const entry = JSON.parse(text);
        if (JSON.stringify(entry) !== text || !Array.isArray(entry) || entry.length !== 5 || entry[0] !== 'causal-batch1') invalid();
        const base = identity(entry[1]); number(entry[2]); const start = number(entry[3]);
        if (receipts.has(entry[1]) || start > BigInt(snapshot.lastCounter) || !Array.isArray(entry[4]) || entry[4].length > 64) invalid();
        const edits = entry[4].map(edit => {
          if (!Array.isArray(edit) || edit.length !== 3 || !Array.isArray(edit[1]) || edit[1].length > 64) invalid();
          return { address: edit[0], observed: edit[1].map(value => CausalVersion.decode(JSON.stringify(value))),
            payload: CausalVersion.decode(JSON.stringify(['causal-version1', edit[0], [this.#author, '1'], [[this.#author, '1']], edit[2]])).payload };
        });
        const admitted = normalized(edits); range(base, admitted.length);
        if (JSON.stringify(admitted.map(edit => edit.encoded)) !== JSON.stringify(entry[4])) invalid();
        const result = admitted.map((edit, index) => {
          const token = child(base, index), found = writes.get(token);
          if (used.has(token) || !found || BigInt(found.value.dot.counter) !== start + BigInt(index) + 1n ||
              JSON.stringify(found.encoded) !== JSON.stringify(edit.encoded)) invalid();
          used.add(token); return own(found.value);
        });
        receipts.set(entry[1], { request: JSON.stringify([entry[2], entry[4]]), result });
      }
    }
    return { expected, staging, owner, snapshot, receipts };
  }
  async snapshot() {
    const { expected, snapshot } = await this.#load();
    return { generation: expected[0].generation, ...snapshot };
  }
  async write(token, generation, edits) {
    const base = identity(token); number(generation);
    const admitted = normalized(edits); range(base, admitted.length);
    const encoded = admitted.map(edit => edit.encoded), request = JSON.stringify([generation, encoded]);
    const { expected, staging, owner, snapshot, receipts } = await this.#load();
    const receipt = receipts.get(token);
    if (receipt && receipt.request !== request) invalid();
    if (!receipt && expected[0].generation !== generation) throw new LinksError('conflict: stale causal batch base', { outcome: 'failed' });
    // Every staged write checks counter/capacity before the only real CAS.
    const result = [];
    for (let index = 0; index < admitted.length; index++) {
      const edit = admitted[index];
      const value = await owner.write(child(base, index), edit.encoded[0], edit.observed, edit.payload);
      const counter = receipt ? receipt.result[index].dot.counter : (BigInt(snapshot.lastCounter) + BigInt(index) + 1n).toString();
      if (value.dot.counter !== counter) throw new TypeError('causal batch child-token collision');
      result.push(value);
    }
    if (receipt) {
      return result;
    }
    if (staging.value.entries.length === 0) staging.value.entries.push(JSON.stringify(['causal-replica1', this.#author]));
    const entries = expected[1].entries.length === 0 ? [JSON.stringify(['causal-batches1', this.#keys[0], this.#author])] : [...expected[1].entries];
    entries.push(JSON.stringify(['causal-batch1', token, generation, snapshot.lastCounter, encoded])); this.#admit(entries);
    let committed;
    try { committed = await this.#store.compareExchange([...this.#keys], expected, [[...staging.value.entries], entries]); }
    catch (cause) { throw new LinksError('indeterminate causal batch publication; reread and retry the original request', { cause }); }
    if (committed === false) throw new LinksError('conflict: causal batch state changed', { outcome: 'failed' });
    if (committed !== true) throw new LinksError('indeterminate causal batch publication response');
    return result;
  }
}
