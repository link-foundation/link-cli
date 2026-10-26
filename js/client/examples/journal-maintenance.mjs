// node|bun|deno run --allow-net ... endpoints-comma-separated fresh-prefix/
import { EtcdJournalStore } from '../src/cluster.js';
const args = globalThis.Deno?.args ?? process.argv.slice(2);
if (args.length !== 2) throw Error('usage: endpoints-comma-separated fresh-prefix/');
const store = new EtcdJournalStore(args[0].split(','), { prefix: args[1] });
function check(condition) { if (!condition) throw Error('maintenance contract failed'); }
const options = { pageKeys: 1 };
if ((await store.scanNamespace(options)).length !== 0) throw Error('example requires an empty dedicated namespace');
check(await store.compareExchange(['a', 'b'], [{ generation: '0', entries: [] }, { generation: '0', entries: [] }], [['first 雪'], ['second']]));
const before = await store.scanNamespace(options);
check(before.length === 2 && before[0].key === 'a' && before[0].version.entries[0] === 'first 雪');
check(await store.compareDelete('a', before[0].version.generation));
check(!await store.compareDelete('a', before[0].version.generation));
check(await store.compareExchange(['a'], [{ generation: '0', entries: [] }], [['replacement']]));
check(!await store.compareDelete('a', before[0].version.generation));
const after = await store.scanNamespace(options);
check(BigInt(after[0].version.generation) > BigInt(before[0].version.generation) && after[0].version.entries[0] === 'replacement');
check(JSON.stringify(after[1]) === JSON.stringify(before[1]));
const keysOnly = await store.scanNamespace({ ...options, keysOnly: true });
check(keysOnly.every((entry) => entry.version.entries.length === 0) && keysOnly[0].version.generation === after[0].version.generation);
console.log('PASS JS: pinned pagination, conditional delete, absent conflict, ABA protection, keys-only scan');
