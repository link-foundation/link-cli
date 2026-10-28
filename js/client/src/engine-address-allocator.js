// Selected permanent range reservations over a linearizable atomic journal key.
import { LinksError } from './errors.js';

const maximum = 18446744073709551615n;
const invalid = () => { throw new TypeError('invalid address reservation state or request'); };
const number = value => {
  if (typeof value !== 'string' || value.length > 20 || !/^(0|[1-9][0-9]*)$/.test(value) || BigInt(value) > maximum) invalid();
  return BigInt(value);
};
const identity = value => {
  if (typeof value !== 'string' || !/^[0-9a-f]{32}$/.test(value) || value === '0'.repeat(32)) invalid();
};
const grant = (author, token, count, first, last) => ({ author, token, count, first, last });

/** One coordinator key defines one immutable fresh-address domain. Grants and
 * retry receipts are permanent; abandoned addresses are never recycled. This
 * does not fence author lineages or validate writes made outside this service. */
export class AddressAllocator {
  #store; #key; #first; #last; #header; #maxEntries; #maxBytes;
  constructor(store, key, first, last, { maxEntries = 4096, maxBytes = 1048576, requireDurableFlush = false } = {}) {
    if (typeof store?.read !== 'function' || typeof store?.compareExchange !== 'function' ||
        typeof key !== 'string' || !/^[A-Za-z0-9_.-]{1,128}$/.test(key)) invalid();
    const start = number(first), end = number(last);
    if (start === 0n || start > end || !Number.isInteger(maxEntries) || maxEntries < 2 || maxEntries > 4096 ||
        !Number.isInteger(maxBytes) || maxBytes < 1 || maxBytes > 1048576 ||
        typeof requireDurableFlush !== 'boolean' || (requireDurableFlush && store.supportsDurableFlush !== true)) invalid();
    this.#store = store; this.#key = key; this.#first = start; this.#last = end;
    this.#header = JSON.stringify(['address-reservations1', first, last]);
    this.#maxEntries = maxEntries; this.#maxBytes = maxBytes;
  }
  #admit(entries) {
    if (!Array.isArray(entries) || entries.length > this.#maxEntries) invalid();
    let bytes = 0;
    for (const entry of entries) {
      // All accepted fields are ASCII with no escapes. Bound before JSON parsing.
      if (typeof entry !== 'string' || entry.length > 256 || (bytes += entry.length) > this.#maxBytes) invalid();
    }
  }
  async #load() {
    const read = await this.#store.read([this.#key]);
    if (!Array.isArray(read) || read.length !== 1 || read[0] === null || typeof read[0] !== 'object') invalid();
    const generation = read[0].generation; number(generation);
    this.#admit(read[0].entries);
    const expected = { generation, entries: [...read[0].entries] };
    let next = this.#first;
    const reservations = new Map();
    if (generation === '0') {
      if (expected.entries.length !== 0) invalid();
    } else {
      if (expected.entries[0] !== this.#header) invalid();
      for (const text of expected.entries.slice(1)) {
        const entry = JSON.parse(text);
        if (!Array.isArray(entry) || entry.length !== 6 || entry[0] !== 'reserve-addresses1' || JSON.stringify(entry) !== text) invalid();
        const [, author, token, count, first, last] = entry;
        identity(author); identity(token);
        const width = number(count), start = number(first), end = number(last);
        if (width === 0n || start !== next || end > this.#last || end < start || end - start + 1n !== width || reservations.has(token)) invalid();
        reservations.set(token, grant(author, token, count, first, last)); next = end + 1n;
      }
    }
    return { expected, reservations, next };
  }
  async snapshot() {
    const state = await this.#load();
    return { generation: state.expected.generation, next: state.next > this.#last ? null : state.next.toString(),
      reservations: [...state.reservations.values()] };
  }
  async reserve(author, token, count) {
    identity(author); identity(token);
    const width = number(count); if (width === 0n) invalid();
    const state = await this.#load(), retained = state.reservations.get(token);
    if (retained) {
      if (retained.author !== author || retained.count !== count) invalid();
      return retained;
    }
    const end = state.next + width - 1n;
    if (end > this.#last || state.expected.generation === maximum.toString()) invalid();
    const result = grant(author, token, count, state.next.toString(), end.toString());
    const entries = [...state.expected.entries];
    if (entries.length === 0) entries.push(this.#header);
    entries.push(JSON.stringify(['reserve-addresses1', author, token, count, result.first, result.last]));
    this.#admit(entries);
    let committed;
    try { committed = await this.#store.compareExchange([this.#key], [state.expected], [entries]); }
    catch (cause) { throw new LinksError('indeterminate address reservation; retry the original author/token/count', { cause }); }
    if (committed === false) throw new LinksError('conflict: address reservations changed; retry the original request', { outcome: 'failed' });
    if (committed !== true) throw new LinksError('indeterminate address reservation publication response; retry the original author/token/count');
    return result;
  }
}
