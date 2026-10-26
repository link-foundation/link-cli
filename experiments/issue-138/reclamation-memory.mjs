// Deterministic provider for reclamation tests; no durability claim.
const maximum = 18446744073709551615n;
const encoder = new TextEncoder();
const name = (key) => typeof key === 'string' && /^[A-Za-z0-9_.-]{1,128}$/.test(key);
function keysOf(keys) {
  if (!Array.isArray(keys) || keys.length < 1 || keys.length > 32 || new Set(keys).size !== keys.length || keys.some((key) => !name(key))) throw Error('invalid fake keys');
  return [...keys];
}
function generation(text, absent = true) {
  if (typeof text !== 'string' || !/^(0|[1-9][0-9]*)$/.test(text) || BigInt(text) > maximum || (!absent && text === '0')) throw Error('invalid fake generation');
  return text;
}
function records(items) {
  if (!Array.isArray(items) || items.some((item) => typeof item !== 'string')) throw Error('invalid fake records');
  return [...items];
}
function owned(value) { return value ? { generation: value.generation, entries: [...value.entries] } : { generation: '0', entries: [] }; }
export class MemoryMaintenanceStore {
  supportsDurableFlush = false;
  values = new Map(); revision = 0n; calls = []; before = new Map(); after = new Map();
  constructor(label) { this.label = label; }
  on(operation, callback, { after = false } = {}) {
    const table = after ? this.after : this.before;
    const queue = table.get(operation) ?? []; queue.push(callback); table.set(operation, queue);
  }
  async #run(operation, keys, action, extra = {}) {
    const call = { operation, keys: [...keys], ...extra }; this.calls.push(call);
    const before = this.before.get(operation)?.shift(); if (before) await before(this, call);
    const result = action();
    const after = this.after.get(operation)?.shift(); if (after) await after(this, call);
    return result;
  }
  #next() {
    if (this.revision === maximum) throw Error('fake generation exhausted');
    this.revision++; return String(this.revision);
  }
  async read(keys) {
    keys = keysOf(keys);
    return this.#run('read', keys, () => keys.map((key) => owned(this.values.get(key))));
  }
  async compareExchange(keys, expected, updates) {
    keys = keysOf(keys);
    if (expected.length !== keys.length || updates.length !== keys.length) throw Error('invalid fake arrays');
    expected = expected.map((item) => generation(item.generation));
    updates = updates.map((item) => item === null ? null : records(item));
    return this.#run('swap', keys, () => {
      if (keys.some((key, index) => (this.values.get(key)?.generation ?? '0') !== expected[index])) return false;
      if (updates.every((item) => item === null)) return true;
      const version = this.#next();
      keys.forEach((key, index) => { if (updates[index] !== null) this.values.set(key, { generation: version, entries: [...updates[index]] }); });
      return true;
    }, { expected, updates });
  }
  async scanNamespace({ maxKeys = 4096, maxBytes = 8388608, pageKeys = 128, keysOnly = false } = {}) {
    if ([maxKeys, maxBytes, pageKeys].some((value) => !Number.isSafeInteger(value) || value <= 0) || pageKeys > 256 || typeof keysOnly !== 'boolean') throw Error('invalid fake scan limits');
    return this.#run('scan', [], () => {
      let bytes = 0;
      const result = [...this.values].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([key, value]) => {
        bytes += encoder.encode(key).length + 8;
        if (!keysOnly) for (const item of value.entries) bytes += encoder.encode(item).length + 1;
        return { key, version: { generation: value.generation, entries: keysOnly ? [] : [...value.entries] } };
      });
      if (result.length > maxKeys || bytes > maxBytes) throw Error('complete fake scan limit');
      return result;
    }, { keysOnly });
  }
  async compareDelete(key, expected) {
    keysOf([key]); generation(expected, false);
    return this.#run('delete', [key], () => {
      if (this.values.get(key)?.generation !== expected) return false;
      this.#next(); this.values.delete(key); return true;
    }, { expected });
  }
}
