// Selected maintenance helpers. No liveness policy and no background activity.
import { LinksError } from './errors.js';
const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true });
const revisionOf = (value) => typeof value === 'string' && /^[1-9][0-9]*$/.test(value) && BigInt(value) <= 9223372036854775807n;
function base64(text) { let binary = ''; for (const byte of encoder.encode(text)) binary += String.fromCharCode(byte); return btoa(binary); }
function decode(text) {
  if (typeof text !== 'string') throw Error('missing namespace base64');
  const bytes = atob(text);
  if (btoa(bytes) !== text) throw Error('noncanonical namespace base64');
  return decoder.decode(Uint8Array.from(bytes, (byte) => byte.charCodeAt(0)));
}
function keyOf(key) {
  if (typeof key !== 'string' || !/^[A-Za-z0-9_.-]{1,128}$/.test(key)) throw new TypeError('invalid namespace key');
}
export function scanOptions({ maxKeys = 4096, maxBytes = 8388608, pageKeys = 128, keysOnly = false } = {}) {
  if ([maxKeys, maxBytes, pageKeys].some((value) => !Number.isSafeInteger(value) || value <= 0) || pageKeys > 256 || typeof keysOnly !== 'boolean')
    throw new TypeError('invalid namespace scan limits');
  return { maxKeys, maxBytes, pageKeys, keysOnly };
}
export async function scanOnce(post, prefix, { maxKeys, maxBytes, pageKeys, keysOnly }) {
  const entries = []; let bytes = 0; let revision; let previous = prefix; let start = previous;
  const end = base64(prefix.slice(0, -1) + '0');
  for (;;) {
    const limit = Math.min(pageKeys, maxKeys - entries.length + 1);
    const range = { key: base64(start), range_end: end, limit: String(limit), sort_order: 'ASCEND', sort_target: 'KEY', keys_only: keysOnly };
    if (revision !== undefined) range.revision = revision;
    const response = await post({ success: [{ request_range: range }] });
    const current = response?.header?.revision;
    if (!revisionOf(current)) throw Error('invalid namespace revision');
    revision ??= current;
    if (BigInt(current) < BigInt(revision) || response.succeeded !== true || !Array.isArray(response.responses) || response.responses.length !== 1)
      throw Error('invalid namespace snapshot');
    const page = response.responses[0]?.response_range;
    if (!page || Array.isArray(page) || typeof page !== 'object' || (page.more !== undefined && typeof page.more !== 'boolean') ||
        (page.kvs !== undefined && !Array.isArray(page.kvs))) throw Error('invalid namespace range');
    const more = page.more === true; const values = page.kvs ?? [];
    const count = page.count === undefined ? '0' : page.count;
    if (count !== '0' && !revisionOf(count)) throw Error('invalid namespace count');
    if (values.length > limit || (more && (values.length === 0 || BigInt(count) <= BigInt(values.length))) || (!more && BigInt(count) !== BigInt(values.length)))
      throw Error('incomplete namespace page');
    for (const value of values) {
      const key = decode(value?.key);
      if (!key.startsWith(prefix)) throw Error('foreign namespace key');
      const name = key.slice(prefix.length); keyOf(name);
      if (key <= previous) throw Error('unordered namespace keys');
      const generation = value.mod_revision;
      if (!revisionOf(generation) || BigInt(generation) > BigInt(revision)) throw Error('invalid namespace generation');
      let records = [];
      if (keysOnly) {
        if (value.value !== undefined && value.value !== '') throw Error('unexpected namespace payload');
      } else {
        records = JSON.parse(decode(value.value));
        if (!Array.isArray(records) || records.some((record) => typeof record !== 'string' || decoder.decode(encoder.encode(record)) !== record)) throw Error('invalid namespace journal');
      }
      let size = encoder.encode(name).length + 8;
      for (const record of records) size += encoder.encode(record).length + 1;
      bytes += size;
      if (!Number.isSafeInteger(bytes) || bytes > maxBytes || entries.length === maxKeys) throw Error('namespace scan limit');
      entries.push({ key: name, version: { generation, entries: records } }); previous = key;
    }
    if (!more) return entries;
    if (entries.length === maxKeys) throw Error('namespace key limit');
    start = previous + '\0';
  }
}
export function deleteRequest(prefix, key, generation) {
  keyOf(key);
  if (!revisionOf(generation)) throw new TypeError('invalid deletion generation');
  key = base64(prefix + key);
  return { compare: [{ key, target: 'MOD', result: 'EQUAL', mod_revision: generation }], success: [{ request_delete_range: { key } }] };
}
export function deletionResult(response) {
  const succeeded = response?.succeeded === true;
  const results = response?.responses === undefined ? [] : response.responses;
  if (!revisionOf(response?.header?.revision) || (response.succeeded !== undefined && typeof response.succeeded !== 'boolean') || !Array.isArray(results) ||
      (!succeeded && results.length !== 0) || (succeeded && (results.length !== 1 || results[0]?.response_delete_range?.deleted !== '1')))
    throw new LinksError('indeterminate deletion response; read fresh state to discover the outcome', { outcome: 'unknown' });
  return succeeded;
}
