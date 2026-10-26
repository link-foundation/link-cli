// Loaded only by the explicitly opened reclamation strategy.
import { LinksError } from './errors.js';
export const epochKey = '__link_gc_epoch';
export const maximum = 18446744073709551615n;
export const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
export const validName = (key) => typeof key === 'string' && /^[A-Za-z0-9_.-]{1,128}$/.test(key);
export const generation = (text) => typeof text === 'string' && /^(0|[1-9][0-9]*)$/.test(text) && BigInt(text) <= maximum;
// Reply validation can follow acknowledged mutations in a composite operation.
export function fail(message) { throw new LinksError(message, { outcome: 'unknown' }); }
// A later unsent/refused request cannot erase earlier steps of this operation.
export async function mutation(work) {
  try { return await work(); }
  catch (cause) { throw new LinksError(String(cause), { outcome: 'unknown', cause }); }
}
export function records(items) {
  if (!Array.isArray(items) || Array.from(items).some((item) => typeof item !== 'string' || decoder.decode(encoder.encode(item)) !== item)) throw new TypeError('journal must contain well-formed Unicode strings');
  return [...items];
}
export function version(item, absent = true) {
  if (!generation(item?.generation) || (!absent && item.generation === '0')) fail('invalid journal generation');
  const entries = records(item.entries);
  if (item.generation === '0' && entries.length !== 0) fail('absent journal has records');
  return { generation: item.generation, entries };
}
export function versions(items, count) {
  if (!Array.isArray(items) || items.length !== count) fail('incomplete journal snapshot');
  return Array.from(items, (item) => version(item));
}
export function keysOf(keys) {
  if (!Array.isArray(keys) || keys.length < 1 || keys.length > 31 || new Set(keys).size !== keys.length || Array.from(keys).some((key) => !validName(key) || key === epochKey)) throw new TypeError('select 1..31 distinct nonreserved shard names');
  return [...keys];
}
export function optionsOf({ maxKeys = 4096, maxBytes = 8388608, pageKeys = 128, cacheBytes = 0 } = {}) {
  if ([maxKeys, maxBytes, pageKeys].some((item) => !Number.isSafeInteger(item) || item <= 0) || pageKeys > 256 || !Number.isSafeInteger(cacheBytes) || cacheBytes < 0) throw new TypeError('invalid reclamation limits');
  return { maxKeys, maxBytes, pageKeys, cacheBytes };
}
export function epochOf(item) {
  item = version(item, false);
  if (item.entries.length !== 2 || item.entries[0] !== 'gc1' || !generation(item.entries[1])) fail('invalid reclamation epoch');
  return BigInt(item.entries[1]);
}
export function keyEpoch(key) {
  if (typeof key !== 'string' || !/^blob2-[0-9a-f]{16}-[0-9a-f]{16}$/.test(key)) fail('invalid reclamation object key');
  return BigInt('0x' + key.slice(6, 22));
}
export function address(epoch, items) {
  let hash = 0xcbf29ce484222325n;
  const add = (bytes) => { for (const byte of bytes) hash = BigInt.asUintN(64, (hash ^ BigInt(byte)) * 0x100000001b3n); };
  add(encoder.encode('blob2|'));
  for (const item of items) { const bytes = encoder.encode(item); add(encoder.encode(`${bytes.length}:`)); add(bytes); }
  return `blob2-${epoch.toString(16).padStart(16, '0')}-${hash.toString(16).padStart(16, '0')}`;
}
export function pointerOf(item, epoch, partitions) {
  if (item.generation === '0' && item.entries.length === 0) return null;
  const parts = item.entries;
  if (item.generation === '0' || parts.length !== 4 || parts[0] !== 'blob2' || !partitions.has(parts[1]) || !generation(parts[3]) || parts[3] === '0') fail('invalid reclamation pointer');
  const objectEpoch = keyEpoch(parts[2]);
  if (objectEpoch > epoch) fail('pointer epoch exceeds catalog epoch');
  return { partition: parts[1], key: parts[2], generation: parts[3], epoch: objectEpoch };
}
export function acknowledged(value) {
  if (typeof value !== 'boolean') throw new LinksError('indeterminate mutation acknowledgement; read fresh state', { outcome: 'unknown' });
  return value;
}
export async function scan(store, options, keysOnly) {
  const result = await store.scanNamespace({ maxKeys: options.maxKeys, maxBytes: options.maxBytes, pageKeys: options.pageKeys, keysOnly });
  if (!Array.isArray(result) || result.length > options.maxKeys) fail('invalid complete namespace snapshot');
  let previous = ''; let bytes = 0;
  return Array.from(result, (item) => {
    if (!validName(item?.key) || item.key <= previous) fail('unordered namespace snapshot');
    const value = version(item.version, false);
    if (keysOnly && value.entries.length !== 0) fail('unexpected keys-only records');
    bytes += encoder.encode(item.key).length + 8;
    for (const record of value.entries) bytes += encoder.encode(record).length + 1;
    if (!Number.isSafeInteger(bytes) || bytes > options.maxBytes) fail('complete namespace byte limit');
    previous = item.key;
    return { key: item.key, version: value };
  });
}
export function catalogOf(items, partitions) {
  const root = items.find((item) => item.key === epochKey);
  if (!root) fail('missing reclamation epoch');
  const epoch = epochOf(root.version); const protectedObjects = new Map();
  for (const item of items) {
    if (item.key === epochKey) continue;
    const pointer = pointerOf(item.version, epoch, partitions);
    const identity = `${pointer.partition}/${pointer.key}`;
    if (protectedObjects.has(identity) && protectedObjects.get(identity) !== pointer.generation) fail('contradictory protected generations');
    protectedObjects.set(identity, pointer.generation);
  }
  return { epoch, protectedObjects };
}
