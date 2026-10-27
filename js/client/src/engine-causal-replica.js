// Selected local causal owner. Only a coherent local atomic store publishes.
import { CausalVersion } from './engine-causal-version.js';
import { LinksError } from './errors.js';

const invalid = () => { throw new TypeError('invalid causal replica state'); };
const id = value => {
  if (typeof value !== 'string' || !/^[0-9a-f]{32}$/.test(value) || value === '0'.repeat(32)) invalid();
};
const number = value => {
  if (typeof value !== 'string' || value.length > 20 || !/^(0|[1-9][0-9]*)$/.test(value) ||
      BigInt(value) > 18446744073709551615n) invalid();
  return BigInt(value);
};
const wire = value => CausalVersion.encode(value);
const own = value => CausalVersion.decode(wire(value));
const raw = value => JSON.parse(wire(value));
const values = items => {
  if (!Array.isArray(items) || items.length > 64) invalid();
  return Array.from(items, own);
};
const compare = (a, b) => BigInt(a.address) < BigInt(b.address) ? -1 : BigInt(a.address) > BigInt(b.address) ? 1 :
  a.dot.author < b.dot.author ? -1 : a.dot.author > b.dot.author ? 1 :
  BigInt(a.dot.counter) < BigInt(b.dot.counter) ? -1 : BigInt(a.dot.counter) > BigInt(b.dot.counter) ? 1 : 0;
const evidence = items => {
  const admitted = values(items); CausalVersion.merge(admitted, []);
  return [...new Map(admitted.map(value => [`${value.dot.author}:${value.dot.counter}`, value])).values()].sort(compare);
};
const descriptor = (address, context, payload) => JSON.stringify([address, context.map(raw),
  raw(CausalVersion.advance(address, '00000000000000000000000000000001', '0', [], payload))[4]]);

class State {
  constructor(author) { this.author = author; this.counter = 0n; this.history = new Map(); this.rows = new Map(); this.tokens = new Map(); }
  remember(items, local = false, update = true) {
    const groups = new Map();
    for (const value of items) {
      const text = wire(value), dot = `${value.dot.author}:${value.dot.counter}`;
      const prior = this.history.get(dot);
      if (prior && prior.text !== text) invalid();
      if (!local && value.dot.author === this.author && !prior) invalid();
      if (value.clock.some(item => item.author === this.author && BigInt(item.counter) > this.counter)) invalid();
      for (const previous of this.history.values()) {
        if (previous.value.address === value.address && JSON.stringify(previous.value.clock) === JSON.stringify(value.clock) &&
            previous.text !== text) invalid();
      }
      this.history.set(dot, { text, value });
      if (!groups.has(value.address)) groups.set(value.address, []);
      groups.get(value.address).push(value);
    }
    if (update) for (const [address, group] of groups)
      this.rows.set(address, CausalVersion.merge(this.rows.get(address) ?? [], group));
  }
  apply(entry) {
    if (!Array.isArray(entry)) invalid();
    if (entry[0] === 'causal-receive1') {
      if (entry.length !== 2 || !Array.isArray(entry[1]) || entry[1].length === 0) invalid();
      const received = values(entry[1].map(value => CausalVersion.decode(JSON.stringify(value))));
      if (JSON.stringify(received.map(raw)) !== JSON.stringify([...received].sort(compare).map(raw)) ||
          new Set(received.map(value => `${value.dot.author}:${value.dot.counter}`)).size !== received.length) invalid();
      this.remember(received); return;
    }
    if (entry[0] !== 'causal-write1' || entry.length !== 4) invalid();
    id(entry[1]); if (this.tokens.has(entry[1]) || !Array.isArray(entry[2])) invalid();
    const seen = evidence(entry[2].map(value => CausalVersion.decode(JSON.stringify(value))));
    if (JSON.stringify(seen.map(raw)) !== JSON.stringify(entry[2])) invalid();
    const context = CausalVersion.merge(seen, []);
    const mutation = CausalVersion.decode(JSON.stringify(entry[3]));
    this.remember(seen, false, false);
    const expected = CausalVersion.advance(mutation.address, this.author, this.counter.toString(), context, mutation.payload);
    if (wire(expected) !== wire(mutation)) invalid();
    this.counter = BigInt(mutation.dot.counter); this.remember([mutation], true);
    this.tokens.set(entry[1], { value: mutation, descriptor: descriptor(mutation.address, context, mutation.payload) });
  }
  versions() { return [...this.rows.values()].flat().sort(compare).map(own); }
}

/** The caller owns the atomic store and must reserve this key exclusively for
 * one replica lineage. Separate active copies must use different authors.
 * Every operation rereads coherent local state; no mutation retries itself. */
export class CausalReplica {
  #store; #key; #author; #maxEntries; #maxBytes;
  constructor(store, key, author, { maxEntries = 4096, maxBytes = 8388608, requireDurableFlush = false } = {}) {
    if (typeof store?.read !== 'function' || typeof store?.compareExchange !== 'function' ||
        typeof key !== 'string' || !/^[A-Za-z0-9_.-]{1,128}$/.test(key)) invalid();
    id(author);
    if (!Number.isInteger(maxEntries) || maxEntries < 2 || maxEntries > 4096 ||
        !Number.isInteger(maxBytes) || maxBytes < 1 || maxBytes > 8388608 ||
        typeof requireDurableFlush !== 'boolean' || (requireDurableFlush && store.supportsDurableFlush !== true)) invalid();
    this.#store = store; this.#key = key; this.#author = author; this.#maxEntries = maxEntries; this.#maxBytes = maxBytes;
  }
  #admit(entries) {
    if (!Array.isArray(entries) || entries.length > this.#maxEntries) invalid();
    let bytes = 0;
    for (const entry of entries) {
      if (typeof entry !== 'string' || entry.length > 1048576 || (bytes += entry.length) > this.#maxBytes) invalid();
      let depth = 0, separators = 0, quoted = false;
      // Canonical fields contain no escapes. Bound allocation shape before JSON.
      for (const char of entry) {
        if (char < ' ' || char > '~' || char === '\\') invalid();
        if (char === '"') { quoted = !quoted; continue; }
        if (quoted) continue;
        if (char === '{' || char === '}') invalid();
        if (char === '[') { if (++depth > 5) invalid(); }
        else if (char === ']') { if (--depth < 0) invalid(); }
        else if (char === ',' && ++separators > 32768) invalid();
      }
      if (quoted || depth !== 0) invalid();
    }
  }
  async #load() {
    const result = await this.#store.read([this.#key]);
    if (!Array.isArray(result) || result.length !== 1 || result[0] === null || typeof result[0] !== 'object') invalid();
    const { generation, entries } = result[0]; number(generation); this.#admit(entries);
    const expected = { generation, entries: [...entries] }, state = new State(this.#author);
    if (generation === '0') { if (entries.length !== 0) invalid(); return { expected, state }; }
    if (entries.length === 0 || entries[0] !== JSON.stringify(['causal-replica1', this.#author])) invalid();
    for (const entry of entries.slice(1)) {
      const parsed = JSON.parse(entry); if (JSON.stringify(parsed) !== entry) invalid(); state.apply(parsed);
    }
    return { expected, state };
  }
  async #publish(expected, entry) {
    const entries = expected.entries.length === 0 ? [JSON.stringify(['causal-replica1', this.#author])] : [...expected.entries];
    entries.push(JSON.stringify(entry)); this.#admit(entries);
    let result;
    try { result = await this.#store.compareExchange([this.#key], [expected], [entries]); }
    catch (cause) { throw new LinksError('indeterminate causal publication; retry the same token after a fresh read', { cause }); }
    if (result === false) throw new LinksError('conflict: causal state changed; reread and retry the same token', { outcome: 'failed' });
    if (result !== true) throw new LinksError('indeterminate causal publication response; reread before retrying');
  }
  async snapshot() {
    const { state } = await this.#load();
    return { author: this.#author, lastCounter: state.counter.toString(), versions: state.versions() };
  }
  async read(address) {
    if (number(address) === 0n) invalid();
    return (await this.#load()).state.rows.get(address)?.map(own) ?? [];
  }
  async write(token, address, context, payload) {
    id(token); const admitted = evidence(context), seen = CausalVersion.merge(admitted, []), input = descriptor(address, seen, payload);
    const { expected, state } = await this.#load(), previous = state.tokens.get(token);
    state.remember(admitted, false, false);
    if (previous) { if (previous.descriptor !== input) invalid(); return own(previous.value); }
    const mutation = CausalVersion.advance(address, this.#author, state.counter.toString(), seen, payload);
    const entry = ['causal-write1', token, admitted.map(raw), raw(mutation)];
    state.apply(entry); await this.#publish(expected, entry); return own(mutation);
  }
  async receive(incoming) {
    const admitted = values(incoming), { expected, state } = await this.#load();
    const newValues = new Map(), known = new Set(state.history.keys());
    // Validate the entire batch, including historical contradictions, before CAS.
    state.remember(admitted);
    for (const value of admitted) {
      const dot = `${value.dot.author}:${value.dot.counter}`;
      if (!known.has(dot)) newValues.set(dot, value);
    }
    if (newValues.size === 0) return false;
    const entry = ['causal-receive1', [...newValues.values()].sort(compare).map(raw)];
    await this.#publish(expected, entry); return true;
  }
}
