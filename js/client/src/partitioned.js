// Immutable journal objects in independent stores, atomic metadata in a catalog.
// Object namespaces are append-only: never delete/rewrite a referenced object.
import { LinksError } from './errors.js';

const encoder = new TextEncoder();
const validName = (value) => typeof value === 'string' && /^[A-Za-z0-9_.-]{1,128}$/.test(value);
const generation = (value) => typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value) && BigInt(value) <= 18446744073709551615n;
function entries(value) {
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== 'string')) throw new TypeError('journal must contain strings');
  return [...value];
}
// FNV is an address index, not an authentication primitive. Creation compares
// the complete contents; a collision is rejected, never overwritten/reused.
function address(records) {
  let hash = 0xcbf29ce484222325n;
  const add = (bytes) => { for (const byte of bytes) hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * 0x100000001b3n); };
  add(encoder.encode('blob1|'));
  for (const record of records) { const bytes = encoder.encode(record); add(encoder.encode(`${bytes.length}:`)); add(bytes); }
  return `blob1-${hash.toString(16).padStart(16, '0')}`;
}
function keysOf(keys) {
  if (!Array.isArray(keys) || keys.length < 1 || keys.length > 32 || new Set(keys).size !== keys.length || keys.some((key) => !validName(key))) throw new TypeError('select 1..32 distinct shard names');
  return [...keys];
}
function atomic(store) { return typeof store?.read === 'function' && typeof store?.compareExchange === 'function'; }
async function readOne(store, key) {
  const result = await store.read([key]);
  if (!Array.isArray(result) || result.length !== 1 || !generation(result[0]?.generation)) throw new LinksError('invalid immutable object snapshot', { outcome: 'not-sent' });
  return { generation: result[0].generation, entries: entries(result[0].entries) };
}

/** The catalog publishes only pointers. Data stores may be separate consensus
 * groups. All selected catalog generations are still checked atomically.
 * Stores/prefixes are caller-owned; object keys must remain immutable forever.
 */
export class PartitionedJournalStore {
  #catalog; #partitions; #route;
  constructor(catalog, partitions, route) {
    if (!atomic(catalog) || !partitions || typeof route !== 'function') throw new TypeError('provide a catalog, partitions and a routing function');
    this.#partitions = new Map(Object.entries(partitions));
    if (this.#partitions.size === 0 || [...this.#partitions].some(([key, store]) => !validName(key) || !atomic(store))) throw new TypeError('provide named atomic data stores');
    this.#catalog = catalog; this.#route = route;
  }
  get supportsDurableFlush() { return this.#catalog.supportsDurableFlush === true && [...this.#partitions.values()].every((store) => store.supportsDurableFlush === true); }
  async read(keys) {
    keys = keysOf(keys);
    const snapshot = await this.#catalog.read(keys);
    if (!Array.isArray(snapshot) || snapshot.length !== keys.length) throw new LinksError('incomplete catalog snapshot', { outcome: 'not-sent' });
    const pointers = snapshot.map((item) => ({ generation: item?.generation, entries: entries(item?.entries) }));
    const result = [];
    for (const pointer of pointers) {
      if (!generation(pointer?.generation)) throw new LinksError('invalid catalog generation', { outcome: 'not-sent' });
      const parts = entries(pointer.entries);
      if (pointer.generation === '0' && parts.length === 0) { result.push({ generation: '0', entries: [] }); continue; }
      if (pointer.generation === '0' || parts.length !== 4 || parts[0] !== 'blob1' || !this.#partitions.has(parts[1]) || !/^blob1-[0-9a-f]{16}$/.test(parts[2]) || !generation(parts[3]) || parts[3] === '0') throw new LinksError('invalid catalog pointer', { outcome: 'not-sent' });
      const object = await readOne(this.#partitions.get(parts[1]), parts[2]);
      if (object.generation !== parts[3] || address(object.entries) !== parts[2]) throw new LinksError('missing or modified immutable object', { outcome: 'not-sent' });
      result.push({ generation: pointer.generation, entries: object.entries });
    }
    return result;
  }
  async compareExchange(keys, expected, updates) {
    keys = keysOf(keys);
    if (!Array.isArray(expected) || !Array.isArray(updates) || expected.length !== keys.length || updates.length !== keys.length || expected.some((item) => !generation(item?.generation))) throw new TypeError('expected generations and updates must match shards');
    const versions = expected.map((item) => ({ generation: item.generation, entries: [] }));
    // Validate and copy every update and route before staging any data.
    const staged = updates.map((update, i) => {
      if (update === null) return null;
      const data = entries(update); const partition = this.#route(keys[i]);
      if (!this.#partitions.has(partition)) throw new TypeError('route must select a configured partition');
      return { data, partition, key: address(data) };
    });
    const pointers = [];
    for (const item of staged) {
      if (item === null) { pointers.push(null); continue; }
      const store = this.#partitions.get(item.partition);
      let object = await readOne(store, item.key);
      if (object.generation === '0') {
        // An uncertain data write leaves only an orphan, never a published group.
        // No mutation is automatically retried, including an unknown response.
        await store.compareExchange([item.key], [object], [item.data]);
        object = await readOne(store, item.key);
      }
      if (object.generation === '0' || object.entries.length !== item.data.length || object.entries.some((entry, i) => entry !== item.data[i])) throw new LinksError('immutable object collision or inconsistent data store', { outcome: 'not-sent' });
      pointers.push(['blob1', item.partition, item.key, object.generation]);
    }
    return this.#catalog.compareExchange(keys, versions, pointers);
  }
}
