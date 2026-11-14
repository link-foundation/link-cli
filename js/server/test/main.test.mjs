// Tests of `main` (src/main.js), the port of rust/src/main.rs, in-process:
// help and version, the checks of the command line before any store opens,
// `--structure`, the transactions and version-control runs, the standard
// output the pipeline falls back to, and the `--serve`, `--forward` and
// `--connect` commands over real TCP sockets on port 0. The differential tests run the same invocations
// through bin/clink.js and the Rust `clink` and compare them.
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { existsSync, mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import { Cli } from '../src/cli.js';
import { setTrace } from '../src/ipc.js';
import { finish, main, openNamedStorage, run, runQueryPipelineWith } from '../src/main.js';
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

test('--structure of a missing link fails as in the Rust clink', async () => {
  const db = newDb();
  assert.equal((await clink(['--db', db, '() ((1 1))'])).status, 0);
  assert.deepEqual(await clink(['--db', db, '--structure', '99']), {
    status: 1,
    out: '',
    err: 'Error: Link not found: 99\n',
  });
});

test('--transactions runs the query pipeline over the transitions log', async () => {
  const db = newDb();
  // Every expected text is what the Rust clink prints for the same invocation.
  assert.deepEqual(
    await clink(['--db', db, '--transactions', '() ((1 1) (1 2))', '--changes', '--after']),
    {
      status: 0,
      out: '() ((1: 1 1))\n() ((2: 1 2))\n(1: 1 1)\n(2: 1 2)\n',
      err: '',
    },
  );
  assert.deepEqual(
    await clink(['--db', db, '--transactions', '--before', '((2: 1 2)) ()', '--changes']),
    {
      status: 0,
      out: '(1: 1 1)\n(2: 1 2)\n((2: 1 2)) ()\n',
      err: '',
    },
  );
  assert.ok(existsSync(db.replace(/\.links$/, '.transitions.links')));
  const log = await clink(['--db', db, '--log']);
  assert.equal(log.status, 0);
  const transitions = log.out
    .trimEnd()
    .split('\n')
    .map((line) => line.split('\t'));
  assert.deepEqual(
    transitions.map(([sequence, , kind, , change]) => [sequence, kind, change]),
    [
      ['1', 'Create', '(0,0,0) -> (1,1,1)'],
      ['2', 'Create', '(0,0,0) -> (2,1,2)'],
      ['3', 'Delete', '(2,1,2) -> (0,0,0)'],
    ],
  );
  for (const [, timestamp, , transaction] of transitions) {
    assert.match(timestamp, /^[0-9]+$/);
    assert.match(transaction, /^[0-9a-f]{32}$/);
  }
});

test('--vc runs the query pipeline under version control', async () => {
  const db = newDb();
  const vc = (...args) => clink(['--db', db, '--vc', ...args]);
  assert.deepEqual(await vc('() ((1 1))', '--changes', '--after'), {
    status: 0,
    out: '() ((1: 1 1))\n(1: 1 1)\n',
    err: '',
  });
  assert.ok(existsSync(db.replace(/\.links$/, '.versioncontrol.links')));
  assert.deepEqual(await vc('--tag', 'v1', '--list-tags'), { status: 0, out: 'v1\t1\n', err: '' });
  assert.deepEqual(await vc('--before', '((1: 1 1)) ()', '--changes'), {
    status: 0,
    out: '(1: 1 1)\n((1: 1 1)) ()\n',
    err: '',
  });
  assert.deepEqual(await vc('--list-branches'), {
    status: 0,
    out: '* main\tparent=-\tfork=0\thead=2\n',
    err: '',
  });
  assert.deepEqual(await vc('--checkout', 'v1', '--after'), {
    status: 0,
    out: '(1: 1 1)\n',
    err: '',
  });
});

/**
 * Runs `call()` with `process.stdout.write` captured and resolves what it
 * printed. The capture lasts only while `call` runs synchronously, so the
 * test runner's own output is never caught.
 */
async function printedBy(call) {
  const chunks = [];
  const write = process.stdout.write;
  process.stdout.write = (chunk) => chunks.push(String(chunk)) > 0;
  let result;
  try {
    result = call();
  } finally {
    process.stdout.write = write;
  }
  await result;
  return chunks.join('');
}

test('the pipeline writes to the standard output when no stream is given', async () => {
  const cli = (...args) => Cli.parse(['--db', newDb(), ...args]).cli;
  const noTrigger = () => false;

  const piped = cli('() ((1 1))', '--changes');
  const pipedStore = openNamedStorage(piped.db, piped);
  assert.equal(
    await printedBy(() => runQueryPipelineWith(piped, pipedStore, noTrigger)),
    '() ((1: 1 1))\n',
  );

  const ran = cli('() ((1 1) (1 2))', '--after');
  const ranStore = openNamedStorage(ran.db, ran);
  assert.equal(await printedBy(() => run(ran, ranStore, noTrigger)), '(1: 1 1)\n(2: 1 2)\n');

  const finished = cli('--before', '() ((2 2))');
  const finishedStore = openNamedStorage(finished.db, finished);
  finishedStore.create(1, 1);
  assert.equal(await printedBy(() => finish(finished, finishedStore)), '(1: 1 1)\n');
  assert.deepEqual(finishedStore.linoLines(), ['(1: 1 1)', '(2: 2 2)']);
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
