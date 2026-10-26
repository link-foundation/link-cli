import { acknowledged, address, catalogOf, encoder, epochKey, epochOf, fail, generation, keysOf, mutation, optionsOf, pointerOf, records, scan, validName, versions } from './reclaiming-codec.js';
import { collect } from './reclaiming-collector.js';
const capable = (store) => ['read', 'compareExchange', 'scanNamespace', 'compareDelete'].every((key) => typeof store?.[key] === 'function');

export async function openStore(catalog, partitions, route, options) {
  options = optionsOf(options);
  if (!capable(catalog) || !partitions || typeof route !== 'function') throw new TypeError('provide maintenance-capable catalog, partitions and route');
  partitions = new Map(Object.entries(partitions).sort(([a], [b]) => a < b ? -1 : 1));
  if (partitions.size === 0 || [...partitions].some(([name, provider]) => !validName(name) || !capable(provider))) throw new TypeError('provide named maintenance-capable data stores');
  return mutation(async () => {
    let initial = await scan(catalog, options, false);
    if (initial.length === 0) {
      for (const provider of partitions.values()) if ((await scan(provider, options, true)).length !== 0) fail('initial reclamation namespaces must be empty');
      acknowledged(await catalog.compareExchange([epochKey], [{ generation: '0', entries: [] }], [['gc1', '0']]));
      initial = await scan(catalog, options, false);
    }
    catalogOf(initial, partitions);
    return new Store(catalog, partitions, route, options);
  });
}
class Store {
  cache; used = 0;
  constructor(catalog, partitions, route, options) {
    this.catalog = catalog; this.partitions = partitions; this.route = route; this.options = options;
    this.cache = options.cacheBytes > 0 ? new Map() : null;
  }
  get supportsDurableFlush() { return this.catalog.supportsDurableFlush === true && [...this.partitions.values()].every((provider) => provider.supportsDurableFlush === true); }
  async snapshot(keys) {
    const items = versions(await this.catalog.read([...keys, epochKey]), keys.length + 1);
    const root = items.pop(); const epoch = epochOf(root);
    const pointers = items.map((item) => pointerOf(item, epoch, this.partitions));
    return { items, pointers, root, epoch };
  }
  remember(identity, items) {
    if (!this.cache) return;
    let size = 1;
    for (const item of items) { const weight = encoder.encode(item).length + 1; if (weight > this.options.cacheBytes - size) return; size += weight; }
    const old = this.cache.get(identity);
    if (old) { this.used -= old.size; this.cache.delete(identity); }
    while (size > this.options.cacheBytes - this.used) {
      const first = this.cache.keys().next().value; this.used -= this.cache.get(first).size; this.cache.delete(first);
    }
    this.cache.set(identity, { size, entries: [...items] }); this.used += size;
  }
  async group(snapshot) {
    const result = [];
    for (let index = 0; index < snapshot.items.length; index++) {
      const item = snapshot.items[index]; const pointer = snapshot.pointers[index];
      if (pointer === null) { result.push({ generation: '0', entries: [] }); continue; }
      const identity = `${pointer.partition}/${pointer.key}/${pointer.generation}`;
      const cached = this.cache?.get(identity);
      if (cached) {
        this.cache.delete(identity); this.cache.set(identity, cached);
        result.push({ generation: item.generation, entries: [...cached.entries] }); continue;
      }
      const [object] = versions(await this.partitions.get(pointer.partition).read([pointer.key]), 1);
      if (object.generation === '0') return { missing: index };
      if (object.generation !== pointer.generation || address(pointer.epoch, object.entries) !== pointer.key) fail('modified immutable reclamation object');
      this.remember(identity, object.entries);
      result.push({ generation: item.generation, entries: object.entries });
    }
    return { result };
  }
  async read(keys) {
    keys = keysOf(keys);
    const first = await this.snapshot(keys); let result = await this.group(first);
    if (result.missing === undefined) return result.result;
    const fresh = await this.snapshot(keys);
    const a = first.items[result.missing]; const b = fresh.items[result.missing];
    if (a.generation === b.generation || (a.entries.length === b.entries.length && a.entries.every((item, index) => item === b.entries[index]))) fail('missing or modified referenced reclamation object');
    // Discard the ENTIRE first result, including cache hits; never splice groups.
    result = await this.group(fresh);
    if (result.missing !== undefined) fail('referenced reclamation object retired during bounded retry');
    return result.result;
  }
  async compareExchange(keys, expected, updates) {
    keys = keysOf(keys);
    if (!Array.isArray(expected) || !Array.isArray(updates) || expected.length !== keys.length || updates.length !== keys.length || Array.from(expected).some((item) => !generation(item?.generation))) throw new TypeError('expected generations and updates must match shards');
    const guards = expected.map((item) => ({ generation: item.generation, entries: [] }));
    const staged = Array.from(updates, (update, index) => {
      if (update === null) return null;
      const data = records(update); const partition = this.route(keys[index]);
      if (!this.partitions.has(partition)) throw new TypeError('route must select a configured partition');
      return { data, partition };
    });
    return mutation(async () => {
      const current = await this.snapshot(keys);
      if (current.items.some((item, index) => item.generation !== guards[index].generation)) return false;
      const pointers = [];
      for (const item of staged) {
        if (item === null) { pointers.push(null); continue; }
        const key = address(current.epoch, item.data); const provider = this.partitions.get(item.partition);
        let [object] = versions(await provider.read([key]), 1);
        if (object.generation === '0') {
          acknowledged(await provider.compareExchange([key], [object], [item.data]));
          [object] = versions(await provider.read([key]), 1);
        }
        if (object.generation === '0' || object.entries.length !== item.data.length || object.entries.some((record, index) => record !== item.data[index])) fail('immutable object collision or staging conflict');
        pointers.push(['blob2', item.partition, key, object.generation]);
      }
      return acknowledged(await this.catalog.compareExchange([...keys, epochKey], [...guards, current.root], [...pointers, null]));
    });
  }
  collectGarbage() { return collect(this); }
}
