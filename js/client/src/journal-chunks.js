// Optional immutable-object layout; keep count/byte costs off the default path.
import { LinksError } from './errors.js';
const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
const limits = { maxRecords: 65536, maxBytes: 8388608, maxChunks: 4096 };
const defaults = { maxRecords: 64, maxBytes: 65536, maxChunks: 1024 };
const decimal = (value, maximum) => typeof value === 'string' && /^(0|[1-9][0-9]*)$/.test(value) && BigInt(value) <= maximum;
const invalid = () => { throw new LinksError('invalid chunk manifest or immutable chunk', { outcome: 'not-sent' }); };

export function chunkOptions(value) {
  if (value === null) return null;
  if (typeof value !== 'object' || Array.isArray(value)) throw new TypeError('provide journal chunk limits');
  const options = { ...defaults, ...value };
  for (const name of Object.keys(limits)) if (!Number.isSafeInteger(options[name]) || options[name] < 1 || options[name] > limits[name]) throw new TypeError('invalid journal chunk limits');
  return options;
}

export function splitChunks(records, options) {
  const chunks = []; let current = []; let bytes = 0;
  for (const record of records) {
    const encoded = encoder.encode(record);
    if (decoder.decode(encoded) !== record || encoded.length > options.maxBytes) throw new TypeError('chunk record exceeds byte limit or is not well-formed Unicode');
    if (current.length > 0 && (current.length === options.maxRecords || encoded.length > options.maxBytes - bytes)) {
      chunks.push(current); current = []; bytes = 0;
    }
    current.push(record); bytes += encoded.length;
    if (chunks.length >= options.maxChunks) throw new TypeError('journal chunk count limit');
  }
  if (current.length > 0) chunks.push(current);
  return chunks;
}

// All structural checks finish before the caller fetches any data object.
export function chunkManifest(parts, partitions) {
  if (parts[0] !== 'chunks1') return null;
  if (parts.length < 4 || !partitions.has(parts[1]) || !decimal(parts[2], 4294967295n) || !decimal(parts[3], 4096n)) invalid();
  const count = Number(parts[3]);
  if (parts.length !== 4 + count * 3) invalid();
  let total = 0; const references = [];
  for (let i = 4; i < parts.length; i += 3) {
    if (!/^blob1-[0-9a-f]{16}$/.test(parts[i]) || !decimal(parts[i + 1], 18446744073709551615n) || parts[i + 1] === '0' || !decimal(parts[i + 2], 65536n) || parts[i + 2] === '0') invalid();
    const records = Number(parts[i + 2]); total += records;
    references.push({ key: parts[i], generation: parts[i + 1], records });
  }
  if (total !== Number(parts[2])) invalid();
  return { partition: parts[1], references };
}

export function validateChunk(records, count) {
  if (records.length !== count) invalid();
  let bytes = 0;
  for (const record of records) {
    const encoded = encoder.encode(record); bytes += encoded.length;
    if (bytes > limits.maxBytes || decoder.decode(encoded) !== record) invalid();
  }
}
