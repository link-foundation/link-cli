// Synthetic consumer acceptance and benchmark for @link-foundation/link-cli-client.
//
// It plays a consumer like an accommodation search: multilingual records with
// nested addresses, offers whose prices repeat across records, offers that
// arrive twice (duplicate offers) and trace references to the crawl that
// produced each offer. Everything is written in bulk batches; then a client
// that shares nothing with the writer (a new server process for the servers)
// reads every record back and compares it with the input.
//
//   CLINK_RUST=... node examples/synthetic-consumer.mjs --backend rust --records 500
//   CLINK_CSHARP=... bun examples/synthetic-consumer.mjs --backend csharp
//   CLINK_WASM=js/pkg deno run -A examples/synthetic-consumer.mjs --backend wasm --json
//
// It prints latency (per phase and per request), requests, client memory,
// server memory (Linux) and disk use, and exits non-zero if the readback
// differs. It is an acceptance check with numbers, not a comparative
// benchmark suite (that is link-cli#31 / #35).
//
// The link model, in doublets (every text is a link whose name is the text):
//
//   atom       (A+i A+i)          named with its text; A = 2^31 marks atoms
//   field      (type value)       e.g. (name 'Khách sạn Sông Hồng'), shared
//   record     (record key)
//   member     (record field)     a record's name, city, country, rating
//   address    (cityField countryField), member (record address)
//   offer      (record priceField), traced (offer crawlTrace)
//
// Equal pairs are deduplicated by the consumer before writing, so the result
// is the same on a backend that keeps duplicates (Rust) and on one that
// merges them (C#); duplicate offers are counted and dropped, not emulated.

import { existsSync, readFileSync } from 'node:fs';
import { mkdtemp, readdir, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';
import { createWasmClient, LinksClient } from '../src/index.js';
import { connectProcess } from '../src/process.js';

const ATOM_BASE = 2n ** 31n;

function parseArguments(argv) {
  const options = { backend: null, records: 300, window: 64, seed: 108, json: false };
  for (let i = 0; i < argv.length; i += 1) {
    const flag = argv[i];
    if (flag === '--json') options.json = true;
    else if (flag === '--backend') options.backend = argv[++i];
    else if (flag === '--records') options.records = Number(argv[++i]);
    else if (flag === '--window') options.window = Number(argv[++i]);
    else if (flag === '--seed') options.seed = Number(argv[++i]);
    else throw new Error(`unknown argument ${flag}`);
  }
  if (!options.backend) {
    const env = process.env;
    options.backend = env.CLINK_RUST ? 'rust' : env.CLINK_CSHARP ? 'csharp' : env.CLINK_WASM ? 'wasm' : null;
  }
  if (!['rust', 'csharp', 'wasm'].includes(options.backend)) throw new Error('--backend rust|csharp|wasm (with CLINK_RUST, CLINK_CSHARP or CLINK_WASM set)');
  if (!Number.isInteger(options.records) || options.records < 1) throw new Error('--records is a positive integer');
  return options;
}

// ---------------------------------------------------------------- data

/** A small deterministic generator, so every run writes the same records. */
function random(seed) {
  let state = seed >>> 0;
  return () => {
    state = (state + 0x6d2b79f5) >>> 0;
    let t = state;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 2 ** 32;
  };
}

const PLACES = [
  ['Hà Nội', 'Việt Nam'],
  ['Đà Nẵng', 'Việt Nam'],
  ['Thành phố Hồ Chí Minh', 'Việt Nam'],
  ['東京', '日本'],
  ['Москва', 'Россия'],
  ['München', 'Deutschland'],
  ['São Paulo', 'Brasil'],
  ['القاهرة', 'مصر'],
  ['Zürich', 'Schweiz'],
  ['Kraków', 'Polska'],
];
const WORDS = ['Khách sạn', 'Nhà nghỉ', 'ホテル', 'Гостиница', 'Gästehaus', 'Pousada', 'فندق', "L'Auberge", 'Hostel "Central"', 'Villa (garden)'];
const SUFFIXES = ['Sông Hồng', 'さくら', 'Звезда', 'Am See', 'Do Sol', 'النيل', 'du Lac', '🏨 Deluxe', 'Old Town', 'Riverside'];

function generateRecords(count, seed) {
  const next = random(seed);
  const pick = (list) => list[Math.floor(next() * list.length)];
  const records = [];
  for (let i = 0; i < count; i += 1) {
    const [city, country] = pick(PLACES);
    const offers = [];
    const crawl = `trace:crawl-${Math.floor(i / 25)}/page-${i % 25}`;
    for (let o = 0, n = 1 + Math.floor(next() * 4); o < n; o += 1) {
      offers.push({ price: `${(20 + Math.floor(next() * 80)) * 5} VND×1000`, trace: crawl });
    }
    // Every fourth record receives its first offer twice (a duplicate offer).
    if (i % 4 === 0) offers.push({ ...offers[0] });
    records.push({
      key: `hotel-${String(i).padStart(6, '0')}`,
      name: `${pick(WORDS)} ${pick(SUFFIXES)} ${i}`,
      rating: `${1 + Math.floor(next() * 5)}★`,
      address: { city, country },
      offers,
    });
  }
  return records;
}

// ---------------------------------------------------------------- backends

async function openBackend(name) {
  if (name === 'wasm') {
    const dir = process.env.CLINK_WASM;
    if (!dir) throw new Error('set CLINK_WASM to the wasm-pack --target web package directory');
    const module = await import(pathToFileURL(join(dir, 'clink_wasm.js')).href);
    module.initSync({ module: readFileSync(join(dir, 'clink_wasm_bg.wasm')) });
    const clink = new module.Clink();
    return {
      // The store lives in this process; a fresh client shares only the store.
      open: async () => createWasmClient(clink),
      serverMemory: () => null,
      disk: async () => null,
      dispose: async () => clink.free(),
    };
  }
  const path = process.env[name === 'rust' ? 'CLINK_RUST' : 'CLINK_CSHARP'];
  if (!path || !existsSync(path)) throw new Error(`set CLINK_${name.toUpperCase()} to the clink executable`);
  const [command, args] = path.endsWith('.dll') ? ['dotnet', [path]] : [path, []];
  const dir = await mkdtemp(join(tmpdir(), `synthetic consumer ${name} `));
  const db = join(dir, 'consumer db.links');
  let current = null;
  return {
    open: async () => {
      current = await connectProcess({ command, args, db, timeout: 60_000 });
      return current;
    },
    serverMemory: () => serverMemory(current?.transport.server.pid),
    disk: async () => {
      let bytes = 0;
      const files = [];
      for (const file of await readdir(dir)) {
        const { size } = await stat(join(dir, file));
        bytes += size;
        files.push({ file, bytes: size });
      }
      return { bytes, files };
    },
    dispose: () => rm(dir, { recursive: true, force: true }),
  };
}

/** Peak resident memory of a process in bytes, where /proc tells it. */
function serverMemory(pid) {
  if (pid === undefined) return null;
  try {
    const status = readFileSync(`/proc/${pid}/status`, 'utf8');
    const match = /VmHWM:\s+(\d+) kB/.exec(status);
    return match ? Number(match[1]) * 1024 : null;
  } catch {
    return null;
  }
}

/** Wraps a client so that every request is timed. */
function timed(client) {
  const durations = [];
  const transport = client.transport;
  const wrapper = {
    capabilities: transport.capabilities,
    async send(text, scope) {
      const started = performance.now();
      try {
        return await transport.send(text, scope);
      } finally {
        durations.push(performance.now() - started);
      }
    },
    close: () => client.close(),
  };
  return { client: new LinksClient(wrapper), durations };
}

// ---------------------------------------------------------------- consumer

class Phases {
  constructor(window) {
    this.window = window;
    this.list = [];
  }

  /** Runs operations as one bulk batch and fails unless every one completed. */
  async batch(client, name, operations) {
    const started = performance.now();
    const result = await client.batch(operations, { window: this.window });
    const ms = performance.now() - started;
    this.list.push({ phase: name, requests: operations.length, ms });
    if (!result.completed) {
      throw new Error(`${name}: ${result.ok} ok, ${result.failed} failed, ${result.unknown} unknown, ${result.notSent} not sent: ${result.firstError?.message}`);
    }
    return result.results.map((entry) => entry.value);
  }
}

/** Interns values: the same key always maps to one position. */
class Interned {
  constructor() {
    this.positions = new Map();
    this.items = [];
  }

  add(key, item) {
    let position = this.positions.get(key);
    if (position === undefined) {
      position = this.items.length;
      this.positions.set(key, position);
      this.items.push(item);
    }
    return position;
  }
}

async function write(client, records, phases) {
  const texts = new Interned();
  const text = (value) => texts.add(value, value);
  const TYPES = ['record', 'name', 'rating', 'city', 'country', 'price'];
  for (const type of TYPES) text(`type:${type}`);
  const plan = records.map((record) => ({
    key: text(record.key),
    name: text(record.name),
    rating: text(record.rating),
    city: text(record.address.city),
    country: text(record.address.country),
    offers: record.offers.map((offer) => ({ price: text(offer.price), trace: text(offer.trace) })),
  }));

  // 1. atoms, then their names
  const atoms = await phases.batch(client, 'atoms', texts.items.map((_, i) => ({ op: 'create', source: ATOM_BASE + BigInt(i), target: ATOM_BASE + BigInt(i) })));
  const atom = (position) => atoms[position].index;
  await phases.batch(client, 'names', texts.items.map((value, i) => ({ op: 'setName', index: atom(i), name: value })));
  const type = (name) => atom(texts.positions.get(`type:${name}`));

  // 2. fields (type value), shared between records
  const fields = new Interned();
  const field = (typeName, value) => fields.add(`${typeName} ${value}`, { source: type(typeName), target: atom(value) });
  const fieldPlan = plan.map((p) => ({
    name: field('name', p.name),
    rating: field('rating', p.rating),
    city: field('city', p.city),
    country: field('country', p.country),
    prices: p.offers.map((offer) => field('price', offer.price)),
  }));
  const fieldLinks = await phases.batch(client, 'fields', fields.items.map((pair) => ({ op: 'create', ...pair })));
  const fieldIndex = (position) => fieldLinks[position].index;

  // 3. records and addresses
  const recordLinks = await phases.batch(client, 'records', plan.map((p) => ({ op: 'create', source: type('record'), target: atom(p.key) })));
  const addresses = new Interned();
  const addressPlan = fieldPlan.map((f) => addresses.add(`${f.city} ${f.country}`, { source: fieldIndex(f.city), target: fieldIndex(f.country) }));
  const addressLinks = await phases.batch(client, 'addresses', addresses.items.map((pair) => ({ op: 'create', ...pair })));

  // 4. members and offers; a duplicate offer is the same pair, written once
  const members = new Interned();
  let duplicateOffers = 0;
  const offerPositions = [];
  plan.forEach((p, r) => {
    const record = recordLinks[r].index;
    const f = fieldPlan[r];
    for (const position of [f.name, f.rating]) members.add(`${record} ${fieldIndex(position)}`, { source: record, target: fieldIndex(position) });
    members.add(`${record} a${addressPlan[r]}`, { source: record, target: addressLinks[addressPlan[r]].index });
    p.offers.forEach((offer, o) => {
      const key = `${record} ${fieldIndex(f.prices[o])}`;
      const before = members.items.length;
      const position = members.add(key, { source: record, target: fieldIndex(f.prices[o]) });
      if (members.items.length === before) duplicateOffers += 1;
      else offerPositions.push({ position, trace: offer.trace });
    });
  });
  const memberLinks = await phases.batch(client, 'members', members.items.map((pair) => ({ op: 'create', ...pair })));
  await phases.batch(client, 'traces', offerPositions.map(({ position, trace }) => ({ op: 'create', source: memberLinks[position].index, target: atom(trace) })));

  return { duplicateOffers, links: atoms.length + fieldLinks.length + recordLinks.length + addressLinks.length + memberLinks.length + offerPositions.length };
}

/** Rebuilds the records from a store, using only what a fresh client reads. */
async function readBack(client, phases) {
  const started = performance.now();
  const links = await client.each();
  phases.list.push({ phase: 'read links', requests: 1, ms: performance.now() - started });
  const byIndex = new Map(links.map((link) => [link.index, link]));
  const atomIndexes = links.filter((link) => link.source >= ATOM_BASE && link.source === link.target).map((link) => link.index);
  const names = await phases.batch(client, 'read names', atomIndexes.map((index) => ({ op: 'getName', index })));
  const text = new Map(atomIndexes.map((index, i) => [index, names[i]]));
  const typeOf = new Map([...text].filter(([, value]) => value.startsWith('type:')).map(([index, value]) => [index, value.slice(5)]));
  const fieldOf = (index) => {
    const link = byIndex.get(index);
    return link && typeOf.has(link.source) ? { type: typeOf.get(link.source), value: text.get(link.target) } : null;
  };
  const records = new Map();
  for (const link of links) {
    if (typeOf.get(link.source) === 'record') records.set(link.index, { key: text.get(link.target), offers: [] });
  }
  const offers = new Map();
  for (const link of links) {
    const record = records.get(link.source);
    if (!record) continue;
    const field = fieldOf(link.target);
    if (field?.type === 'price') {
      const offer = { price: field.value, trace: null };
      record.offers.push(offer);
      offers.set(link.index, offer);
    } else if (field) {
      record[field.type] = field.value;
    } else {
      const address = byIndex.get(link.target);
      record.address = { city: fieldOf(address.source).value, country: fieldOf(address.target).value };
    }
  }
  for (const link of links) {
    const offer = offers.get(link.source);
    if (offer) offer.trace = text.get(link.target);
  }
  return { links: links.length, records: [...records.values()] };
}

/** The input as it must read back: duplicate offers dropped, offers in any order. */
function normalize(records) {
  return records
    .map(({ key, name, rating, address, offers }) => {
      const unique = [...new Map(offers.map((offer) => [`${offer.price}\u0000${offer.trace}`, offer])).values()];
      return JSON.stringify({ key, name, rating, address: { city: address.city, country: address.country }, offers: unique.map((o) => `${o.price} @ ${o.trace}`).sort() });
    })
    .sort();
}

function percentile(sorted, p) {
  return sorted.length === 0 ? 0 : sorted[Math.min(sorted.length - 1, Math.floor((p / 100) * sorted.length))];
}

async function main() {
  const options = parseArguments(process.argv.slice(2));
  const records = generateRecords(options.records, options.seed);
  const backend = await openBackend(options.backend);
  const heapBefore = process.memoryUsage().heapUsed;
  const report = { backend: options.backend, records: options.records, window: options.window };
  try {
    const writePhases = new Phases(options.window);
    const writer = timed(await backend.open());
    const writeStarted = performance.now();
    let written;
    try {
      written = await write(writer.client, records, writePhases);
    } finally {
      report.serverPeakRssBytes = backend.serverMemory();
      await writer.client.close();
    }
    const writeMs = performance.now() - writeStarted;

    const readPhases = new Phases(options.window);
    const reader = timed(await backend.open());
    const readStarted = performance.now();
    let read;
    try {
      read = await readBack(reader.client, readPhases);
    } finally {
      await reader.client.close();
    }
    const readMs = performance.now() - readStarted;

    const expected = normalize(records);
    const actual = normalize(read.records);
    const mismatches = expected.filter((record, i) => record !== actual[i]);
    const latency = (durations) => {
      const sorted = [...durations].sort((a, b) => a - b);
      return { p50: percentile(sorted, 50), p95: percentile(sorted, 95), p99: percentile(sorted, 99), max: sorted.at(-1) ?? 0 };
    };
    Object.assign(report, {
      runtime: globalThis.Deno ? 'deno' : globalThis.Bun ? 'bun' : 'node',
      linksWritten: written.links,
      linksRead: read.links,
      duplicateOffersDropped: written.duplicateOffers,
      verified: mismatches.length === 0 && actual.length === expected.length && read.links === written.links,
      write: { ms: writeMs, requests: writer.durations.length, requestLatencyMs: latency(writer.durations), phases: writePhases.list },
      read: { ms: readMs, requests: reader.durations.length, requestLatencyMs: latency(reader.durations), phases: readPhases.list },
      clientHeapDeltaBytes: process.memoryUsage().heapUsed - heapBefore,
      clientRssBytes: process.memoryUsage().rss,
      disk: await backend.disk(),
    });
    if (options.json) {
      console.log(JSON.stringify(report, null, 2));
    } else {
      const ms = (value) => `${value.toFixed(1)} ms`;
      const bytes = (value) => (value === null || value === undefined ? 'n/a' : `${(value / 1024).toFixed(0)} KiB`);
      console.log(`synthetic consumer on ${report.runtime}, backend ${report.backend}: ${report.records} records, window ${report.window}`);
      for (const side of ['write', 'read']) {
        const { ms: total, requests, requestLatencyMs: l, phases } = report[side];
        console.log(`  ${side}: ${ms(total)}, ${requests} requests (${(requests / (total / 1000)).toFixed(0)}/s); request p50 ${ms(l.p50)}, p95 ${ms(l.p95)}, p99 ${ms(l.p99)}, max ${ms(l.max)}`);
        for (const phase of phases) console.log(`    ${phase.phase.padEnd(12)} ${String(phase.requests).padStart(6)} requests  ${ms(phase.ms)}`);
      }
      console.log(`  links: ${report.linksWritten} written, ${report.linksRead} read back; ${report.duplicateOffersDropped} duplicate offers dropped`);
      console.log(`  memory: client heap +${bytes(report.clientHeapDeltaBytes)}, client RSS ${bytes(report.clientRssBytes)}, server peak RSS ${bytes(report.serverPeakRssBytes)}`);
      console.log(`  disk: ${report.disk ? `${bytes(report.disk.bytes)} in ${report.disk.files.map((f) => `${f.file} (${bytes(f.bytes)})`).join(', ')}` : 'n/a (in memory)'}`);
      console.log(`  readback by a fresh client: ${report.verified ? 'identical' : `${mismatches.length} records differ, e.g. ${mismatches[0]}`}`);
    }
    if (!report.verified) process.exitCode = 1;
  } finally {
    await backend.dispose();
  }
}

await main();
