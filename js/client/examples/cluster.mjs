// Node: CLINK_WASM=../../pkg node examples/cluster.mjs http://localhost:2379 /demo/
// Use an already initialized wasm module when embedding in a browser.
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { ClusterEngine, EtcdJournalStore, PartitionedJournalStore } from '../src/index.js';
const directory = resolve(process.env.CLINK_WASM ?? '../pkg');
const binding = await import(pathToFileURL(resolve(directory, 'clink_wasm.js')).href);
binding.initSync({ module: await readFile(resolve(directory, 'clink_wasm_bg.wasm')) });
const cache = process.argv.includes('--cache'); const args = process.argv.slice(2).filter((arg) => arg !== '--cache');
if (cache && args.length !== 4) throw new TypeError('--cache requires east and west data groups');
const store = new EtcdJournalStore((args[0] ?? 'http://localhost:2379').split(','), { prefix: args[1] ?? '/demo/' });
const journals = args[2] && args[3] ? new PartitionedJournalStore(store, {
  east: new EtcdJournalStore(args[2].split(','), { prefix: args[1] ?? '/demo/' }),
  west: new EtcdJournalStore(args[3].split(','), { prefix: args[1] ?? '/demo/' }),
}, (key) => key === 'a' ? 'east' : 'west', { cacheBytes: cache ? 65536 : 0 }) : store;
const engine = new ClusterEngine(binding, journals, { requireDurableFlush: true });
const transaction = await engine.begin(['a', 'b']);
try {
for (const key of ['a', 'b']) {
  const shard = transaction.shard(key);
  if (await shard.count() === 0n) await shard.create(0, 0);
  await shard.setName(1, 'JS coordinated 雪');
}
await transaction.commit();
for (let i = 0; i < 2; i++) {
  const read = await engine.begin(['a', 'b']);
  try { for (const key of ['a', 'b']) if (await read.shard(key).count() === 0n) throw Error('Missing committed link.'); }
  finally { await read.close(); }
}
console.log('Both shard journals committed atomically.');

} finally { await transaction.close(); }
