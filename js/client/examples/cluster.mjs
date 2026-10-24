// Node: CLINK_WASM=../../pkg node examples/cluster.mjs http://localhost:2379 /demo/
// Use an already initialized wasm module when embedding in a browser.
import { readFile } from 'node:fs/promises';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { ClusterEngine, EtcdJournalStore } from '../src/index.js';
const directory = resolve(process.env.CLINK_WASM ?? '../pkg');
const binding = await import(pathToFileURL(resolve(directory, 'clink_wasm.js')).href);
binding.initSync({ module: await readFile(resolve(directory, 'clink_wasm_bg.wasm')) });
const store = new EtcdJournalStore((process.argv[2] ?? 'http://localhost:2379').split(','), { prefix: process.argv[3] ?? '/demo/' });
const engine = new ClusterEngine(binding, store, { requireDurableFlush: true });
const transaction = await engine.begin(['a', 'b']);
try {
for (const key of ['a', 'b']) {
  const shard = transaction.shard(key);
  if (await shard.count() === 0n) await shard.create(0, 0);
  await shard.setName(1, 'JS coordinated 雪');
}
await transaction.commit();
console.log('Both shard journals committed atomically.');

} finally { await transaction.close(); }
