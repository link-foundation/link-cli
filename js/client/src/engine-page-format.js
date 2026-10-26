// SQL-free native doublets pages under the synchronous ordered-log contract.
// Wire format and persistence assumptions: docs/protocol/native-pages.md.
export const PAGE = 4096, CELLS = 252, MAX_PAGES = Math.floor(Number.MAX_SAFE_INTEGER / PAGE);
export const encoder = new TextEncoder();
export const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
export const META_MAGIC = encoder.encode('LNKDMT01'), PAGE_MAGIC = encoder.encode('LNKDPG01');
export const EXTERNAL = 1n << 63n, PAYLOAD = (1n << 56n) - 1n;
export const fail = (text) => { throw new Error('invalid native pages: ' + text); };
export const same = (a, b) => a.length === b.length && a.every((value, index) => value === b[index]);
export const view = (bytes) => new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
export const hash = (bytes, initial = 0xcbf29ce484222325n) => {
  let result = initial;
  for (const byte of bytes) result = BigInt.asUintN(64, (result ^ BigInt(byte)) * 0x100000001b3n);
  return result;
};
export const pageHash = (bytes) => hash(bytes.subarray(64), hash(bytes.subarray(0, 56)));
export const number = (value) => {
  if (value > BigInt(Number.MAX_SAFE_INTEGER)) fail('integer exceeds portable range');
  return Number(value);
};
export const finish = (value) => { if (value !== undefined) throw new TypeError('page I/O must complete synchronously'); };

export function metadata(state) {
  const bytes = new Uint8Array(128), data = view(bytes);
  bytes.set(META_MAGIC); data.setUint32(8, 1, true); data.setUint32(12, state.flags, true); bytes.set(state.identity, 16);
  for (const [index, value] of [state.generation, state.pages, state.rows, state.records, state.root, state.lastHash].entries())
    data.setBigUint64(32 + index * 8, BigInt(value), true);
  data.setBigUint64(120, hash(bytes.subarray(0, 120)), true);
  return bytes;
}
export function decodeMetadata(bytes, flags) {
  if (bytes === undefined || bytes.length === 0) return undefined;
  if (!(bytes instanceof Uint8Array)) fail('metadata bytes');
  if (bytes.length !== 128) return null;
  const data = view(bytes);
  if (hash(bytes.subarray(0, 120)) !== data.getBigUint64(120, true)) return null;
  if (!same(bytes.subarray(0, 8), META_MAGIC) || data.getUint32(8, true) !== 1) fail('unsupported metadata version');
  if (data.getUint32(12, true) !== flags || bytes.subarray(80, 120).some(Boolean)) fail('unsupported metadata flags');
  const values = Array.from({ length: 5 }, (_, i) => number(data.getBigUint64(32 + i * 8, true)));
  const [generation, pages, rows, records, root] = values;
  const lastHash = data.getBigUint64(72, true);
  if (pages > MAX_PAGES || generation > pages || rows > pages * CELLS || records > rows || rows < pages || records < generation)
    fail('metadata counts');
  if (pages === 0 ? values.some(Boolean) || lastHash !== 0n
    : generation === 0 || root <= (pages - 1) * CELLS || root > pages * CELLS) fail('metadata root');
  return { flags, identity: bytes.slice(16, 32), generation, pages, rows, records, root, lastHash };
}
