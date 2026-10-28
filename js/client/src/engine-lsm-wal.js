// Optional native-lsm1 semantic replay. This codec performs no storage I/O.
const encoder = new TextEncoder();
const maximum = 18446744073709551615n;
const defaults = { maxKeys: 4096, maxBytes: 8388608, bufferKeys: 64, bufferBytes: 1048576,
  maxRuns: 4, maxWalRecords: 1024, maxWalBytes: 16777216 };
const limits = { maxKeys: 65536, maxBytes: 8388608, bufferKeys: 65536, bufferBytes: 8388608,
  maxRuns: 64, maxWalRecords: 65536, maxWalBytes: 67108864 };
function invalid(message) { throw new TypeError('invalid LSM WAL: ' + message); }
export function lsmOptions(selected = {}) {
  if (selected === null || typeof selected !== 'object' || Array.isArray(selected)) invalid('options');
  const options = { ...defaults, ...selected };
  for (const [key, bound] of Object.entries(limits)) {
    if (!Number.isSafeInteger(options[key]) || options[key] < 1 || options[key] > bound) invalid('admission bounds');
  }
  if (options.bufferKeys > options.maxKeys || options.bufferBytes > options.maxBytes) invalid('buffer exceeds total admission');
  return options;
}
function counter(value, positive = false) {
  if (typeof value !== 'string' || value.length > 20 || value.length === 0
    || /[^0-9]/.test(value) || (value.length > 1 && value[0] === '0')) invalid('canonical u64');
  const result = BigInt(value);
  if (result > maximum || (positive && result === 0n)) invalid('u64 range');
  return result;
}
function rows(value, lower, upper, maxKeys, maxBytes) {
  if (!Array.isArray(value) || value.length > maxKeys) invalid('row count');
  let previous = '', bytes = 0;
  return Array.from(value, (row) => {
    if (!Array.isArray(row) || row.length !== 3) invalid('row shape');
    const [key, generation, entries] = row;
    if (typeof key !== 'string' || key.length < 1 || key.length > 128 || /[^A-Za-z0-9_.-]/.test(key) || key <= previous) invalid('row key ordering');
    const current = counter(generation, true);
    if (current <= lower || current > upper) invalid('row generation');
    previous = key; bytes += key.length + 8;
    if (entries !== null && !Array.isArray(entries)) invalid('row payload');
    const owned = entries === null ? null : Array.from(entries, (entry) => {
      if (typeof entry !== 'string' || !entry.isWellFormed()) invalid('record Unicode');
      bytes += encoder.encode(entry).length + 1;
      if (bytes > maxBytes) invalid('buffer bytes');
      return entry;
    });
    if (bytes > maxBytes) invalid('buffer bytes');
    return { key, generation, entries: owned };
  });
}
const wireRows = (items) => items.map(({ key, generation, entries }) => [key, generation, entries]);
export class LsmWal {
  static recover(records, selected) {
    const options = lsmOptions(selected);
    if (!Array.isArray(records) || records.length < 1 || records.length > options.maxWalRecords) invalid('record count');
    let bytes = 0;
    for (const record of records) {
      if (typeof record !== 'string' || !record.isWellFormed() || /[\r\n]/.test(record)) invalid('physical record');
      bytes += encoder.encode(record).length;
      if (bytes > options.maxWalBytes) invalid('WAL bytes');
    }
    const checkpoint = JSON.parse(records[0]);
    if (!Array.isArray(checkpoint) || checkpoint.length !== 6 || checkpoint[0] !== 'native-lsm1') invalid('checkpoint format');
    let generation = counter(checkpoint[1]);
    const covered = counter(checkpoint[2]), reserved = counter(checkpoint[3]), manifest = checkpoint[4];
    if (covered > generation || !Array.isArray(manifest) || manifest.length > options.maxRuns) invalid('manifest bounds');
    let previousId = 0n;
    const descriptors = Array.from(manifest, (item) => {
      if (!Array.isArray(item) || item.length !== 5) invalid('table descriptor');
      const [id, identity, digest, count, storedBytes] = item, current = counter(id, true);
      if (current <= previousId || current > reserved) invalid('table ID ordering or reservation');
      previousId = current;
      if (typeof identity !== 'string' || identity.length !== 32 || /[^0-9a-f]/.test(identity)) invalid('native table identity');
      counter(digest);
      const keys = counter(count), size = counter(storedBytes);
      if (keys > BigInt(options.maxKeys) || size < 15n || size > 67108864n) invalid('table admission');
      return { id, identity, digest, count: Number(keys), storedBytes: Number(size) };
    });
    if ((covered !== 0n) !== (descriptors.length !== 0)) invalid('covered table presence');
    let buffer = rows(checkpoint[5], covered, generation, options.bufferKeys, options.bufferBytes);
    if (buffer.reduce((latest, row) => BigInt(row.generation) > latest ? BigInt(row.generation) : latest, covered) !== generation) invalid('missing latest generation');
    for (let i = 1; i < records.length; i++) {
      const record = JSON.parse(records[i]);
      if (!Array.isArray(record) || record.length !== 3 || record[0] !== 'put') invalid('group format');
      const next = counter(record[1], true);
      if (next !== generation + 1n) invalid('group sequence');
      const changes = rows(record[2], generation, next, 32, options.bufferBytes);
      if (changes.length === 0) invalid('empty group');
      const merged = new Map(buffer.map(row => [row.key, row]));
      for (const row of changes) merged.set(row.key, row);
      buffer = rows(wireRows([...merged.values()].sort((a, b) => a.key < b.key ? -1 : 1)), covered, next, options.bufferKeys, options.bufferBytes);
      generation = next;
    }
    return { generation: generation.toString(), coveredGeneration: covered.toString(),
      lastReservedTableId: reserved.toString(), manifest: descriptors, buffer };
  }
  static encode(state, selected) {
    if (state === null || typeof state !== 'object' || !Array.isArray(state.manifest) || !Array.isArray(state.buffer)) invalid('checkpoint state');
    const manifest = state.manifest.map(item => {
      if (item === null || typeof item !== 'object' || !Number.isSafeInteger(item.count) || !Number.isSafeInteger(item.storedBytes)) invalid('descriptor counts');
      return [item.id, item.identity, item.digest, String(item.count), String(item.storedBytes)];
    });
    const encoded = JSON.stringify(['native-lsm1', state.generation, state.coveredGeneration,
      state.lastReservedTableId, manifest, wireRows(state.buffer)]);
    LsmWal.recover([encoded], selected);
    return encoded;
  }
}
