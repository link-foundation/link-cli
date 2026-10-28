import { acknowledged, catalogOf, epochKey, epochOf, fail, keyEpoch, maximum, mutation, scan, versions } from './reclaiming-codec.js';

export async function collect(store) {
  return mutation(async () => {
    const [before] = versions(await store.catalog.read([epochKey]), 1);
    const epoch = epochOf(before);
    if (epoch === maximum) fail('reclamation epoch exhausted');
    const fence = epoch + 1n;
    // One mutation attempt. An unknown fence must never be replayed automatically.
    if (!acknowledged(await store.catalog.compareExchange([epochKey], [before], [['gc1', String(fence)]]))) return null;
    const catalog = catalogOf(await scan(store.catalog, store.options, false), store.partitions);
    if (catalog.epoch < fence) fail('catalog snapshot predates reclamation fence');
    // Providers can alias one physical namespace. Protect live keys across names.
    const protectedKeys = new Set([...catalog.protectedObjects.keys()].map((identity) => identity.slice(identity.indexOf('/') + 1)));
    const snapshots = [];
    // Complete and validate EVERY namespace before the first destructive action.
    for (const [partition, provider] of store.partitions) {
      const items = await scan(provider, store.options, true);
      const observed = new Map();
      for (const item of items) { keyEpoch(item.key); observed.set(item.key, item.version.generation); }
      for (const [identity, generation] of catalog.protectedObjects) {
        const prefix = partition + '/';
        if (identity.startsWith(prefix) && observed.get(identity.slice(prefix.length)) !== generation) fail('missing or changed protected object');
      }
      snapshots.push({ partition, provider, items });
    }
    let examined = 0; let deleted = 0;
    for (const { partition, provider, items } of snapshots) {
      for (const item of items) {
        examined++;
        if (keyEpoch(item.key) >= fence || protectedKeys.has(item.key)) continue;
        // Exact observed generation prevents deletion of an ABA replacement.
        if (acknowledged(await provider.compareDelete(item.key, item.version.generation))) deleted++;
      }
    }
    return { epoch: String(fence), examined, deleted };
  });
}
