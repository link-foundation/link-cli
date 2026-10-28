// Optional explicit author takeover. Every guarded data CAS includes the fence.
import { LinksError } from './errors.js';
const max = 18446744073709551615n;
const invalid = () => { throw new TypeError('invalid author fence state or request'); };
const identity = value => {
  if (typeof value !== 'string' || !/^[0-9a-f]{32}$/.test(value) || value === '0'.repeat(32)) invalid();
};
const number = value => {
  if (typeof value !== 'string' || value.length > 20 || !/^(0|[1-9][0-9]*)$/.test(value) || BigInt(value) > max) invalid();
  return BigInt(value);
};
const key = value => typeof value === 'string' && /^[A-Za-z0-9_.-]{1,128}$/.test(value);
const fenced = () => { throw new LinksError('author owner is fenced by a later takeover', { outcome: 'failed' }); };
const committed = value => {
  if (value !== true && value !== false) throw new LinksError('indeterminate author fence publication reply');
  return value;
};
export class AuthorFence {
  #store; #key; #author; #header; #maxEntries; #maxBytes;
  constructor(store, fenceKey, author, { maxEntries = 4096, maxBytes = 1048576, requireDurableFlush = false } = {}) {
    identity(author);
    if (typeof store?.read !== 'function' || typeof store?.compareExchange !== 'function' || !key(fenceKey) ||
        !Number.isInteger(maxEntries) || maxEntries < 2 || maxEntries > 4096 ||
        !Number.isInteger(maxBytes) || maxBytes < 1 || maxBytes > 1048576 || typeof requireDurableFlush !== 'boolean' ||
        (requireDurableFlush && store.supportsDurableFlush !== true)) invalid();
    this.#store = store; this.#key = fenceKey; this.#author = author;
    this.#header = JSON.stringify(['author-fence1', author]); this.#maxEntries = maxEntries; this.#maxBytes = maxBytes;
  }
  #admit(entries) {
    if (!Array.isArray(entries) || entries.length > this.#maxEntries) invalid();
    let bytes = 0;
    // Canonical fields are ASCII; reject other code units before JSON parsing,
    // so each admitted character is exactly one UTF-8 byte.
    for (const entry of entries) if (typeof entry !== 'string' || entry.length > 256 || /[^\x00-\x7f]/.test(entry) || (bytes += entry.length) > this.#maxBytes) invalid();
  }
  #parse(value) {
    if (value === null || typeof value !== 'object') invalid();
    number(value.generation); this.#admit(value.entries);
    const expected = { generation: value.generation, entries: [...value.entries] }, claims = [], tokens = new Set();
    if (expected.generation === '0') {
      if (expected.entries.length !== 0) invalid();
    } else {
      if (expected.entries[0] !== this.#header) invalid();
      for (const text of expected.entries.slice(1)) {
        const entry = JSON.parse(text);
        if (!Array.isArray(entry) || entry.length !== 3 || entry[0] !== 'claim-author1' || JSON.stringify(entry) !== text) invalid();
        identity(entry[1]);
        if (entry[2] !== (claims.length + 1).toString() || tokens.has(entry[1])) invalid();
        tokens.add(entry[1]); claims.push({ author: this.#author, token: entry[1], epoch: entry[2] });
      }
    }
    return { expected, claims };
  }
  async #read(keys = []) {
    const values = await this.#store.read([this.#key, ...keys]);
    if (!Array.isArray(values) || values.length !== keys.length + 1) invalid();
    return { ...this.#parse(values[0]), values: values.slice(1) };
  }
  async snapshot() {
    const state = await this.#read();
    return { generation: state.expected.generation, epoch: state.claims.length.toString(), claims: state.claims };
  }
  async claim(expectedEpoch, token) {
    number(expectedEpoch); identity(token);
    const state = await this.#read(), retained = state.claims.find(value => value.token === token);
    if (retained) {
      if (retained.epoch !== state.claims.length.toString() || BigInt(retained.epoch) !== BigInt(expectedEpoch) + 1n) fenced();
      return retained;
    }
    if (expectedEpoch !== state.claims.length.toString()) throw new LinksError('conflict: author epoch changed', { outcome: 'failed' });
    if (state.expected.generation === max.toString()) invalid();
    const result = { author: this.#author, token, epoch: (BigInt(expectedEpoch) + 1n).toString() };
    const entries = [...state.expected.entries]; if (entries.length === 0) entries.push(this.#header);
    entries.push(JSON.stringify(['claim-author1', token, result.epoch])); this.#admit(entries);
    let response;
    try { response = await this.#store.compareExchange([this.#key], [state.expected], [entries]); }
    catch (cause) { throw new LinksError('indeterminate author takeover; retry the original predecessor/token', { cause }); }
    if (!committed(response)) throw new LinksError('conflict: author epoch changed', { outcome: 'failed' });
    return result;
  }
  bind(claim) {
    if (claim === null || typeof claim !== 'object' || claim.author !== this.#author) invalid();
    identity(claim.token); if (number(claim.epoch) === 0n) invalid();
    const owned = { author: claim.author, token: claim.token, epoch: claim.epoch };
    const keysFor = keys => {
      if (!Array.isArray(keys) || keys.length < 1 || keys.length > 31 || new Set(keys).size !== keys.length ||
          keys.some(value => !key(value) || value === this.#key)) invalid();
      return [...keys];
    };
    const current = async keys => {
      const state = await this.#read(keys), latest = state.claims.at(-1);
      if (latest?.token !== owned.token || latest.epoch !== owned.epoch) fenced();
      return state;
    };
    const store = this.#store, fenceKey = this.#key;
    return {
      get supportsDurableFlush() { return store.supportsDurableFlush === true; },
      intoStore() { return store; },
      async read(keys) { return (await current(keysFor(keys))).values; },
      async compareExchange(keys, expected, updates) {
        const selected = keysFor(keys);
        if (!Array.isArray(expected) || expected.length !== selected.length || !Array.isArray(updates) || updates.length !== selected.length) invalid();
        // Capture mutable caller input before awaiting the ownership read.
        const versions = structuredClone(expected), writes = structuredClone(updates);
        const state = await current([]);
        return committed(await store.compareExchange([fenceKey, ...selected], [state.expected, ...versions], [null, ...writes]));
      },
    };
  }
}
