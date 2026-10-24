// Conformance of @link-foundation/link-cli-client against real backends, in
// whichever runtime runs this file:
//
//   node test/conformance/run.mjs
//   bun test/conformance/run.mjs
//   deno run -A test/conformance/run.mjs
//
// Backends come from the environment; each one set is tested, and at least
// one must be:
//
//   CLINK_RUST    path of the Rust `clink` executable
//   CLINK_CSHARP  path of the C# `clink` executable (or of `clink.dll`)
//   CLINK_WASM    directory of the `wasm-pack build --target web` package of
//                 rust/wasm (e.g. js/pkg)
//
// The runner is its own minimal harness (node:test is not in every runtime):
// it prints one `ok` / `not ok` / `skip` line per scenario and exits non-zero
// when any scenario failed.

import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { mkdtemp, readFile, rm, stat } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';
import { ANY, createWasmClient, LinksClient, MAX_U32_ADDRESS } from '../../src/index.js';
import { connectProcess, startServer } from '../../src/process.js';
import { connectTcp } from '../../src/tcp.js';
import { engineConformance } from './engine.mjs';
import { clusterConformance } from './cluster.mjs';

const SCENARIO_TIMEOUT_MS = 60_000;
const runtime = globalThis.Deno ? `deno ${globalThis.Deno.version.deno}` : globalThis.Bun ? `bun ${globalThis.Bun.version}` : `node ${process.versions.node}`;
const golden = new URL('../../../../docs/protocol/links-operations.txt', import.meta.url);

/** `[{ request, reply }]` of the golden conversation both servers replay. */
async function goldenConversation() {
  const lines = (await readFile(golden, 'utf8')).split('\n').filter((line) => line !== '' && !line.startsWith('#'));
  const exchanges = [];
  for (const line of lines) {
    if (line.startsWith('> ')) exchanges.push({ request: line.slice(2), reply: [] });
    else exchanges.at(-1).reply.push(line);
  }
  return exchanges.map(({ request, reply }) => ({ request, reply: reply.join('\n') }));
}

// ---------------------------------------------------------------- backends

/** A server process backend: a fresh database in a directory with spaces. */
function processBackend(name, path, capabilities) {
  const [command, args] = path.endsWith('.dll') ? ['dotnet', [path]] : [path, []];
  return {
    name,
    process: true,
    persistent: true,
    async store() {
      const dir = await mkdtemp(join(tmpdir(), `links client ${name} `));
      const db = join(dir, 'conformance db.links');
      const options = { command, args, db, capabilities, timeout: 60_000 };
      return {
        db,
        options,
        /** A client on a server process of its own, stopped by `close()`. */
        open: (extra = {}) => connectProcess({ ...options, ...extra }),
        start: (extra = {}) => startServer({ ...options, ...extra }),
        dispose: () => rm(dir, { recursive: true, force: true }),
      };
    },
  };
}

/** The in-process WebAssembly store; it lives as long as the `Clink`. */
async function wasmBackend(dir) {
  const module = await import(pathToFileURL(join(dir, 'clink_wasm.js')).href);
  module.initSync({ module: readFileSync(join(dir, 'clink_wasm_bg.wasm')) });
  await engineConformance(module);
  await clusterConformance(module);
  return {
    name: 'wasm',
    process: false,
    persistent: false,
    async store() {
      const clink = new module.Clink();
      return {
        clink,
        /** A client borrowing the store: closing it leaves the store alive. */
        open: async (extra = {}) => createWasmClient(clink, extra),
        dispose: async () => clink.free(),
      };
    },
  };
}

async function backendsFromEnvironment() {
  const backends = [];
  const { CLINK_RUST, CLINK_CSHARP, CLINK_WASM } = process.env;
  // Stated, not discovered: the protocol does not say which server answers.
  if (CLINK_RUST) backends.push(processBackend('rust', CLINK_RUST, { duplicateLinks: true }));
  if (CLINK_CSHARP) backends.push(processBackend('csharp', CLINK_CSHARP, { duplicateLinks: false }));
  if (CLINK_WASM) backends.push(await wasmBackend(CLINK_WASM));
  for (const backend of backends) {
    if (backend.name !== 'wasm' && !existsSync(process.env[`CLINK_${backend.name.toUpperCase()}`])) {
      throw new Error(`${backend.name}: no executable at ${process.env[`CLINK_${backend.name.toUpperCase()}`]}`);
    }
  }
  return backends;
}

// ---------------------------------------------------------------- helpers

const indexes = (links) => links.map((link) => link.index);

/** `{ index: { source, target } }` of every link, for readback comparisons. */
async function snapshot(client) {
  return Object.fromEntries((await client.each()).map(({ index, source, target }) => [index, `${source} ${target}`]));
}

/** Reopens the store with a client that shares nothing with the previous one. */
async function fresh(store, previous) {
  await previous?.close();
  return store.open();
}

async function using(store, body) {
  let client = await store.open();
  try {
    return await body(client, async () => {
      client = await fresh(store, client);
      return client;
    });
  } finally {
    await client.close();
  }
}

// ---------------------------------------------------------------- scenarios

const scenarios = [
  {
    name: 'golden conversation (docs/protocol/links-operations.txt)',
    async run(store, backend) {
      const conversation = await goldenConversation();
      assert.equal(conversation.length, 19);
      await using(store, async (client) => {
        for (const { request, reply } of conversation) {
          const actual = (await client.requestText(request)).trimEnd();
          if (reply.startsWith('! ')) {
            assert.match(actual, /^\(error: /, request);
            assert.ok(actual.includes(reply.slice(2)), `${request}: ${actual}`);
          } else if (backend.name === 'wasm' && request.startsWith('(set-name:')) {
            // The in-memory names store answers the named link itself.
            assert.equal(actual, '(link: 2)', request);
          } else {
            assert.equal(actual, reply, request);
          }
        }
      });
    },
  },
  {
    name: 'addresses: the whole u32 range as BigInt, nothing beyond it',
    async run(store) {
      await using(store, async (client) => {
        assert.equal(client.capabilities.maxAddress, MAX_U32_ADDRESS);
        const link = await client.create(MAX_U32_ADDRESS, 1n);
        assert.deepEqual(link, { index: 1n, source: MAX_U32_ADDRESS, target: 1n });
        assert.equal(await client.count([1n, MAX_U32_ADDRESS]), 1n);
        // The client refuses what the store cannot hold before sending it...
        await assert.rejects(client.create(MAX_U32_ADDRESS + 1n, 1n), { name: 'AddressRangeError', outcome: 'not-sent' });
        await assert.rejects(client.create(2n ** 53n + 1n, 1n), { name: 'AddressRangeError', outcome: 'not-sent' });
        // ...and the backend refuses it too when sent as raw text.
        assert.match(await client.requestText(`(create: (${MAX_U32_ADDRESS + 1n} 1))`), /^\(error: /);
        assert.equal(await client.count(), 1n);
      });
    },
  },
  {
    name: 'duplicates: as the backend states in capabilities.duplicateLinks',
    async run(store) {
      await using(store, async (client, reopen) => {
        const first = await client.create(1n, 1n);
        assert.deepEqual(first, { index: 1n, source: 1n, target: 1n });
        const second = await client.create(1n, 1n);
        const result = await client.batch([{ op: 'create', source: 1n, target: 1n }]);
        assert.equal(result.completed, true);
        if (client.capabilities.duplicateLinks) {
          assert.deepEqual(second, { index: 2n, source: 1n, target: 1n });
          assert.deepEqual(result.results[0].value, { index: 3n, source: 1n, target: 1n });
          assert.equal(await client.count([ANY, 1n, 1n]), 3n);
        } else {
          // A deduplicating store merges the equal link: nothing is created.
          assert.equal(second, null);
          assert.equal(result.results[0].value, null);
          assert.equal(await client.count([ANY, 1n, 1n]), 1n);
        }
        const before = await snapshot(client);
        assert.deepEqual(await snapshot(await reopen()), before);
      });
    },
  },
  {
    name: 'holes: deletion cascades, frees addresses, and misses are errors',
    async run(store) {
      await using(store, async (client) => {
        const a = await client.create(0n, 0n);
        const b = await client.create(a.index, a.index);
        const c = await client.create(b.index, 0n);
        const d = await client.create(0n, 7n);
        assert.deepEqual(indexes([a, b, c, d]), [1n, 2n, 3n, 4n]);
        // Deleting b deletes c, which uses it.
        const changes = await client.delete(b.index);
        assert.deepEqual(changes.map((change) => change.before.index).sort(), [2n, 3n]);
        assert.ok(changes.every((change) => change.after === null));
        assert.deepEqual(indexes(await client.each()), [1n, 4n]);
        assert.equal(await client.count(), 2n);
        // A hole is a missing link, not a link to nothing.
        assert.deepEqual(await client.each([2n]), []);
        await assert.rejects(client.delete(2n), { name: 'RemoteError', outcome: 'failed', message: /Link not found: 2/ });
        await assert.rejects(client.update(3n, 1n, 1n), { name: 'RemoteError', outcome: 'failed' });
        assert.equal(await client.getName(2n), null);
        // Every backend reuses the freed addresses, lowest first.
        const e = await client.create(4n, 4n);
        const f = await client.create(4n, 1n);
        assert.deepEqual(indexes([e, f]), [2n, 3n]);
        assert.deepEqual(indexes(await client.each()), [1n, 2n, 3n, 4n]);
      });
    },
  },
  {
    name: 'append and restart: a fresh server process reads back every write',
    persistentOnly: true,
    async run(store) {
      let client = await store.open();
      try {
        const first = await client.batch(Array.from({ length: 50 }, (_, i) => ({ op: 'create', source: BigInt(i), target: 0n })));
        assert.equal(first.completed, true);
        await client.setName(1n, 'first run');
        const before = await snapshot(client);
        const { size } = await stat(store.db);
        assert.ok(size > 0, 'the database file at a path with spaces holds the links');
        client = await fresh(store, client);
        assert.deepEqual(await snapshot(client), before);
        assert.equal(await client.getByName('first run'), 1n);
        const second = await client.batch(Array.from({ length: 50 }, (_, i) => ({ op: 'create', source: BigInt(i), target: 1n })));
        assert.equal(second.completed, true);
        assert.deepEqual(indexes(second.results.map((result) => result.value)), Array.from({ length: 50 }, (_, i) => BigInt(51 + i)));
        const after = await snapshot(client);
        client = await fresh(store, client);
        assert.deepEqual(await snapshot(client), after);
        assert.equal(await client.count(), 100n);
      } finally {
        await client.close();
      }
    },
  },
  {
    name: 'aborted batch: every write reported ok is there, nothing reported not-sent is',
    async run(store) {
      const controller = new AbortController();
      const inner = await store.open();
      // Abort from inside the stream of requests, after the 40th was sent.
      let sent = 0;
      const counting = {
        capabilities: inner.transport.capabilities,
        send(text, scope) {
          sent += 1;
          if (sent === 40) queueMicrotask(() => controller.abort());
          return inner.transport.send(text, scope);
        },
        close: () => inner.close(),
      };
      const client = new LinksClient(counting);
      const operations = Array.from({ length: 200 }, (_, i) => ({ op: 'create', source: BigInt(i + 1), target: BigInt(i + 1) }));
      let result;
      try {
        result = await client.batch(operations, { window: 8, signal: controller.signal });
      } finally {
        await client.close();
      }
      assert.equal(result.completed, false);
      assert.equal(result.firstError.name, 'AbortError');
      assert.equal(result.ok + result.unknown + result.notSent + result.failed, 200);
      assert.equal(result.failed, 0);
      assert.ok(result.ok >= 32 && result.notSent > 0, JSON.stringify({ ...result, results: undefined }));
      // Verified with a client that shares nothing with the aborted one.
      await using(store, async (reader) => {
        const links = await reader.each();
        const stored = new Set(links.map((link) => link.source));
        const count = BigInt(links.length);
        assert.ok(count >= BigInt(result.ok) && count <= BigInt(result.ok + result.unknown), `${count} links after ${result.ok} ok, ${result.unknown} unknown`);
        result.results.forEach((entry, i) => {
          if (entry.status === 'ok') assert.ok(stored.has(entry.value.source), `ok #${i} is stored`);
          if (entry.status === 'not-sent') assert.ok(!stored.has(BigInt(i + 1)), `not-sent #${i} is absent`);
        });
      });
    },
  },
  {
    // Stopping the Rust server during a save used to leave a truncated
    // database: every link was gone, the acknowledged ones included (#108).
    name: 'a server stopped mid-write keeps every acknowledged write',
    processOnly: true,
    async run(store) {
      const create = (from, length) => Array.from({ length }, (_, i) => ({ op: 'create', source: BigInt(from + i), target: BigInt(from + i) }));
      let acknowledged = 0;
      for (let round = 0; round < 4; round += 1) {
        const server = await store.start();
        const client = await connectTcp(server.address);
        // A bigger store makes each save long enough to be interrupted.
        if (round === 0) acknowledged += (await client.batch(create(1, 300), { window: 32 })).ok;
        const controller = new AbortController();
        const pending = client.batch(create(1000 * (round + 1), 200), { window: 8, signal: controller.signal });
        setTimeout(() => {
          controller.abort();
          server.close();
        }, 10 * (round + 1));
        const result = await pending;
        await client.close();
        await server.exited;
        acknowledged += result.ok;
        await using(store, async (reader) => {
          const count = await reader.count();
          assert.ok(count >= BigInt(acknowledged), `round ${round}: ${count} links after ${acknowledged} acknowledged writes`);
        });
      }
    },
  },
  {
    name: 'Unicode and LiNo fidelity of names, read back by a fresh client',
    async run(store) {
      const names = [
        'Hà Nội — phòng đôi',
        'Привет, мир',
        '東京都 渋谷区',
        'emoji 🏨🛏️ ✓',
        `it's "quoted" \`thrice\``,
        "''",
        '(parens): and colon',
        'line\nbreak\r\nand\ttab',
        '~1{41}',
        '#hash',
        '  spaces around  ',
        'x'.repeat(2000),
      ];
      await using(store, async (client, reopen) => {
        const links = [];
        for (const [i, name] of names.entries()) {
          const link = await client.create(BigInt(1000 + i), BigInt(1000 + i));
          links.push(link);
          await client.setName(link.index, name);
        }
        const check = async (reader) => {
          for (const [i, name] of names.entries()) {
            assert.equal(await reader.getName(links[i].index), name, JSON.stringify(name));
            assert.equal(await reader.getByName(name), links[i].index, JSON.stringify(name));
          }
          const exported = await reader.exportLinks();
          assert.deepEqual(new Set(exported.map((link) => link.index)), new Set(names));
        };
        await check(client);
        await check(await reopen());
      });
    },
  },
  {
    name: 'empty names, renaming and moving names',
    async run(store) {
      await using(store, async (client, reopen) => {
        const a = await client.create(5n, 5n);
        const b = await client.create(6n, 6n);
        await client.setName(a.index, '');
        assert.equal(await client.getName(a.index), '');
        assert.equal(await client.getByName(''), a.index);
        await client.setName(a.index, 'moving');
        await client.setName(b.index, 'moving');
        assert.equal(await client.getByName('moving'), b.index);
        assert.equal(await client.getName(a.index), null);
        await client.removeName(b.index);
        const reader = await reopen();
        assert.equal(await reader.getByName('moving'), null);
        assert.equal(await reader.getName(b.index), null);
      });
    },
  },
  {
    name: 'limits: oversized requests are refused before sending',
    async run(store) {
      const client = await store.open({ maxRequestBytes: 64 });
      try {
        await assert.rejects(client.getByName('n'.repeat(100)), { name: 'LimitExceededError', outcome: 'not-sent' });
        assert.equal(await client.count(), 0n);
      } finally {
        await client.close();
      }
    },
  },
  {
    name: 'incompatible protocol: a binary-only server is refused up front',
    processOnly: true,
    async run(store) {
      const server = await store.start({ args: [...store.options.args, '--protocol', 'binary'] });
      try {
        await assert.rejects(connectTcp(server.address, { timeout: 10_000 }), { name: 'ProtocolMismatchError', message: /does not accept this protocol/ });
      } finally {
        await server.close();
      }
    },
  },
  {
    name: 'ownership: a client stops only the server process it started',
    processOnly: true,
    async run(store) {
      const owning = await store.open();
      const owned = owning.transport.server;
      await owning.close();
      const exit = await owned.exited;
      assert.ok(exit.code === 0 || exit.signal === 'SIGINT', JSON.stringify(exit));

      const server = await store.start();
      try {
        const borrowing = await connectTcp(server.address);
        await borrowing.create(1n, 1n);
        await borrowing.close();
        // The server it connected to outlives it.
        const other = await connectTcp(server.address);
        assert.equal(await other.count(), 1n);
        await other.close();
      } finally {
        await server.close();
      }
    },
  },
];

// ---------------------------------------------------------------- harness

function withTimeout(promise, ms, label) {
  let timer;
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => reject(new Error(`${label} took longer than ${ms} ms`)), ms);
  });
  return Promise.race([promise, timeout]).finally(() => clearTimeout(timer));
}

async function main() {
  const backends = await backendsFromEnvironment();
  if (backends.length === 0) {
    console.error('set CLINK_RUST, CLINK_CSHARP and/or CLINK_WASM; see the top of this file');
    process.exit(2);
  }
  console.log(`# link-cli-client conformance on ${runtime}: ${backends.map((backend) => backend.name).join(', ')}`);
  let failed = 0;
  let passed = 0;
  let skipped = 0;
  for (const backend of backends) {
    for (const scenario of scenarios) {
      const label = `${backend.name}: ${scenario.name}`;
      if ((scenario.persistentOnly && !backend.persistent) || (scenario.processOnly && !backend.process)) {
        skipped += 1;
        console.log(`skip - ${label} (${scenario.persistentOnly ? 'not persistent' : 'no server process'})`);
        continue;
      }
      const started = performance.now();
      const store = await backend.store();
      try {
        await withTimeout(scenario.run(store, backend), SCENARIO_TIMEOUT_MS, label);
        passed += 1;
        console.log(`ok - ${label} (${Math.round(performance.now() - started)} ms)`);
      } catch (error) {
        failed += 1;
        console.log(`not ok - ${label}`);
        console.log(String(error?.stack ?? error).replace(/^/gm, '  # '));
      } finally {
        await store.dispose();
      }
    }
  }
  console.log(`# ${passed} passed, ${failed} failed, ${skipped} skipped`);
  process.exit(failed === 0 ? 0 : 1);
}

await main();
