// Tests of `main` (src/main.js), the port of rust/src/main.rs, in-process:
// help and version, the checks of the command line before any store opens,
// `--structure`, the transactions and version-control runs, the standard
// output the pipeline falls back to, and the `--serve`, `--forward` and
// `--connect` commands over real TCP sockets on port 0. The differential tests run the same invocations
// through bin/clink.js and the Rust `clink` and compare them.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { EventEmitter, once } from 'node:events';
import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createInterface } from 'node:readline';
import { after, test } from 'node:test';
import { fileURLToPath } from 'node:url';
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

test('LiNo import depth defaults to 64 and a bounded override applies to all aliases', async () => {
  const db = newDb();
  const input = `${db}.lino`;
  const run = (...args) => clink(['--db', db, ...args]);
  const document = (depth) => `(root: end ${'(item '.repeat(depth - 1)}end${')'.repeat(depth)}`;

  writeFileSync(input, document(64));
  assert.equal((await run('--in', input)).status, 0);
  const before = await run('--after');
  for (const [depth, limit, args] of [
    [65, 64, ['--import']],
    [129, 128, ['--in-max-depth=128', '--lino-input']],
    [2, 1, ['--in-max-depth', '1', '--in']],
  ]) {
    writeFileSync(input, `(unwritten: fresh another)\n${document(depth)}`);
    const failure = await run(...args, input);
    assert.equal(failure.status, 1, failure.err);
    assert.match(failure.err, new RegExp(`nesting depth exceeds the maximum of ${limit}`));
    assert.deepEqual(await run('--after'), before);
  }

  writeFileSync(input, document(128));
  const imported = await run('--in-max-depth', '128', '--lino-input', input, '--after');
  assert.equal(imported.status, 0, imported.err);
  assert.match(imported.out, /^\(root: end /m);
  assert.equal(imported.out.trimEnd().split('\n').length, 130);
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

test('clink keeps its journal in a bounded text file (issue #195)', async () => {
  const db = newDb();
  for (let i = 1; i <= 3; i++) {
    const result = await clink([
      '--db',
      db,
      '--transactions',
      '--retention',
      'sized:1',
      `() ((${i}: ${i} ${i}))`,
    ]);
    assert.equal(result.status, 0, result.err);
  }
  const journal = db.replace(/\.links$/, '.transitions.log');
  assert.ok(!existsSync(db.replace(/\.links$/, '.transitions.links')));
  const entries = readFileSync(journal, 'utf8').split('\n').slice(0, -1);
  assert.ok(entries.length > 0);
  assert.ok(entries.every((entry) => entry.startsWith('__transactions:')));
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
  assert.ok(existsSync(db.replace(/\.links$/, '.transitions.log')));
  assert.ok(!existsSync(db.replace(/\.links$/, '.transitions.links')));
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

test('a transactional import and query each commit once and an unchanged import commits nothing', async () => {
  const db = newDb();
  const input = `${db}.lino`;
  const journal = db.replace(/\.links$/, '.transitions.log');
  const document = Array.from({ length: 10 }, (_, i) => `(k${i}: a${i} b${i})`);
  writeFileSync(input, document.join('\n'));
  const args = ['--db', db, '--transactions', '--trace', '--in', input];

  const imported = await clink(args);
  assert.equal(imported.status, 0, imported.err);
  assert.equal([...imported.err.matchAll(/Committed tx /g)].length, 1);
  assert.equal([...imported.err.matchAll(/\] Saved /g)].length, 4);
  const before = readFileSync(journal);
  assert.equal((await clink(args)).status, 0);
  assert.deepEqual(readFileSync(journal), before);

  document[0] = '(k0: b0 a0)';
  writeFileSync(input, document.join('\n'));
  const query = await clink([...args, '() ((31 31) (32 32))', '--after']);
  assert.equal(query.status, 0, query.err);
  assert.equal([...query.err.matchAll(/Committed tx /g)].length, 1);
  for (const line of [...document, '(31: 31 31)', '(32: 32 32)']) {
    assert.ok(query.out.split('\n').includes(line), `missing ${line} in ${query.out}`);
  }
});

test('a failed query rolls back the import and leaves version history unchanged', async () => {
  const db = newDb();
  const input = `${db}.lino`;
  const args = ['--db', db, '--vc', '--in', input];
  writeFileSync(input, '(base: base base)\n');
  assert.equal((await clink(args)).status, 0);
  const branches = await clink(['--db', db, '--list-branches']);

  writeFileSync(input, '(base: fresh base)\n(new: base fresh)\n');
  const failed = await clink([...args, '(((']);
  assert.equal(failed.status, 1);
  assert.match(failed.err, /Parse error/);
  assert.equal((await clink(['--db', db, '--vc', '--after'])).out, '(base: base base)\n');
  assert.equal((await clink(['--db', db, '--list-branches'])).out, branches.out);
});

test(
  'killing an import before commit leaves the original store',
  { skip: process.platform === 'win32' },
  async () => {
    const db = newDb();
    const input = `${db}.lino`;
    const fifo = `${db}.fifo`;
    const args = ['--db', db, '--transactions', '--in', input];
    assert.equal(spawnSync('mkfifo', [fifo]).status, 0);
    writeFileSync(input, '(base: base base)\n');
    assert.equal((await clink(args)).status, 0);
    writeFileSync(input, '(base: added base)\n(new: base added)\n');

    const bin = fileURLToPath(new URL('../bin/clink.js', import.meta.url));
    const child = spawn(process.execPath, [bin, ...args, '--after', '--out', fifo]);
    const closed = once(child, 'close');
    const lines = createInterface({ input: child.stdout });
    // The export blocks until the test opens the FIFO, after printing the pending changes.
    const kill = setTimeout(() => child.kill('SIGKILL'), 15_000);
    let sawImport = false;
    try {
      for await (const line of lines) {
        if (line === '(new: base added)') {
          sawImport = true;
          break;
        }
      }
    } finally {
      clearTimeout(kill);
      child.kill('SIGKILL');
      await closed;
    }
    assert.ok(sawImport, 'the child did not reach the export FIFO');
    assert.equal(
      (await clink(['--db', db, '--transactions', '--after'])).out,
      '(base: base base)\n',
    );
  },
);

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
