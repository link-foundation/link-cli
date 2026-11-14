// Tests of `main` (src/main.js), the port of rust/src/main.rs, in-process:
// help and version, the checks of the command line before any store opens,
// `--structure`, and the `--serve`, `--forward` and `--connect` commands over
// real TCP sockets on port 0. The differential tests run the same invocations
// through bin/clink.js and the Rust `clink` and compare them.
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import { Cli } from '../src/cli.js';
import { setTrace } from '../src/ipc.js';
import { main } from '../src/main.js';
import { traced, until } from './helpers/protocol-server.mjs';

const TIMEOUT = { timeout: 20_000 };
const dir = mkdtempSync(join(tmpdir(), 'clink-main-'));
after(() => rmSync(dir, { recursive: true, force: true }));
let stores = 0;

/** A stream that keeps what is written to it. */
function sink() {
  const chunks = [];
  return { write: (text) => chunks.push(String(text)), text: () => chunks.join('') };
}

/** A new store path in the temporary directory. */
const newDb = () => join(dir, `store-${++stores}.links`);

/** Runs `main(args)` with captured streams and an empty environment: `{ status, out, err }`. */
async function clink(args, io = {}) {
  const stdout = sink();
  const stderr = sink();
  const status = await main(args, { stdout, stderr, env: {}, cwd: dir, ...io });
  return { status, out: stdout.text(), err: stderr.text() };
}

/** The endpoints a banner announced. */
const announced = (stream) =>
  [...stream.text().matchAll(/^clink server listening on (.+)$/gm)].map((match) => match[1]);

/** Runs `main(args)` as a server and resolves once it announces its endpoint. */
async function serve(args) {
  const signals = new EventEmitter();
  const stdout = sink();
  const stderr = sink();
  const running = main(args, { stdout, stderr, signals, env: {}, cwd: dir });
  await until(() => announced(stdout).length > 0);
  return {
    address: announced(stdout)[0],
    stderr,
    async stop() {
      signals.emit('SIGINT');
      return running;
    },
  };
}

test('--help and --version print the texts of the command line', async () => {
  assert.deepEqual(await clink(['--help']), { status: 0, out: Cli.helpText(), err: '' });
  assert.deepEqual(await clink(['--version']), {
    status: 0,
    out: `${Cli.versionText()}\n`,
    err: '',
  });
});

test('conflicting commands fail before any store opens', async () => {
  const db = newDb();
  const cases = [
    [['--always', '--once'], 'Only one of --always, --once, or --never can be used at a time.'],
    [
      ['--serve', '127.0.0.1:0', '--connect', '127.0.0.1:1'],
      '--serve and --connect cannot be used together.',
    ],
    [
      ['--connect', '127.0.0.1:1', '--indexed-lookups'],
      '--indexed-lookups applies to local stores; enable it on the server.',
    ],
  ];
  for (const [args, message] of cases) {
    const result = await clink(['--db', db, ...args]);
    assert.equal(result.status, 1, args.join(' '));
    assert.equal(result.out, '');
    assert.match(
      result.err,
      new RegExp(`^Error: ${message.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}`),
    );
  }
});

test('--structure prints the structure of a link and skips the query', async () => {
  const db = newDb();
  assert.equal((await clink(['--db', db, '() ((1 1) (1 2))'])).status, 0);
  const result = await clink(['--db', db, '--structure', '2', '() ((3 3))', '--after']);
  // The Rust `clink` prints the same: the source spine is expanded.
  assert.deepEqual(result, { status: 0, out: '(2: (1: 1 1) 2)\n', err: '' });
  // The query was not run: the store still has two links.
  assert.equal((await clink(['--db', db, '--after'])).out, '(1: 1 1)\n(2: 1 2)\n');
});

test('--serve, --forward and --connect run in-process until SIGINT', TIMEOUT, async () => {
  const server = await serve(['--db', newDb(), '--serve', '127.0.0.1:0']);
  try {
    const connected = await clink(['--connect', server.address, '() ((1 1))']);
    assert.deepEqual(connected, { status: 0, out: '() ((1: 1 1))\n', err: '' });
    let relay;
    const lines = await traced(async () => {
      relay = await serve(['--serve', '127.0.0.1:0', '--forward', server.address, '--trace']);
      const read = await clink(['--connect', relay.address, '((1: 1 1)) ((1: 1 1))']);
      assert.deepEqual(read, { status: 0, out: '((1: 1 1)) ((1: 1 1))\n', err: '' });
      assert.equal(await relay.stop(), 0);
    });
    // `--trace` turns on the transport trace of the process.
    assert.equal(relay.stderr.text(), `clink: forwarding to tcp:${server.address}\n`);
    assert.ok(lines.length > 0, 'the transports traced nothing');
  } finally {
    setTrace(false);
    assert.equal(await server.stop(), 0);
  }
});
