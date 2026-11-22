// The JavaScript clink against the Rust clink as protocol clients, servers
// and relays (`--connect`, `--serve`, `--forward`): every refusal compared
// with the harness, then a Rust and a JavaScript server on port 0, each
// queried by both clients over text and binary, directly and through a
// relay of the other implementation. Every reply is compared byte for byte;
// then both servers are stopped with SIGTERM and their output, exit status
// and the files they saved are compared too. Skipped without CLINK_RUST or
// without bin/clink.js.
//
// Not compared: endpoints that need a DNS lookup (`999.1.1.1:5`), whose
// time and text depend on the resolver of the machine.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, it } from 'node:test';
import { differential, JS, RUST, skip as noRust } from './harness.mjs';

const skip = noRust || (existsSync(JS) ? false : `${JS} is missing`);
const LIMITS = { skip, timeout: 120_000 };
const ENV = { ...process.env, RUST_BACKTRACE: '0', NO_COLOR: '1' };
const CLINK = { rust: [RUST], js: [process.execPath, JS] };

/** The invocations both refuse, with their exact error and status. */
const REFUSED = {
  'connection refused': ['--connect', '127.0.0.1:1', '()'],
  'connection refused with a token': ['--connect', '127.0.0.1:1', '--token', 't', '()'],
  'unknown client protocol': ['--connect', '127.0.0.1:1', '--protocol', 'xml', '()'],
  'text with a binary option': [
    '--connect',
    '127.0.0.1:1',
    '--protocol',
    'text',
    '--packed-widths',
    '()',
  ],
  'text, trimmed and in capitals, with an arity': [
    '--connect',
    '127.0.0.1:1',
    '--protocol',
    ' TEXT ',
    '--arity',
    '3',
    '()',
  ],
  'indexed lookups of a client': ['--connect', '127.0.0.1:1', '--indexed-lookups', '()'],
  'a port that is no number': ['--connect', 'bogus:x', '()'],
  'a missing unix socket': ['--connect', 'unix:/nonexistent/x.sock', '()'],
  'a unix endpoint without address': ['--connect', 'unix:', '()'],
  'stdio with an address': ['--connect', 'stdio:x', '()'],
  'shared memory nobody listens on': ['--connect', 'shm:/nonexistent/x', '()'],
  'an empty endpoint': ['--connect', '', '()'],
  'serve and connect': ['--serve', '127.0.0.1:0', '--connect', '127.0.0.1:1'],
  'serve with a query': ['--serve', '127.0.0.1:0', '()'],
  'serve with --query': ['--serve', '127.0.0.1:0', '--query', '()'],
  'unknown server protocol': ['--serve', '127.0.0.1:0', '--protocol', 'xml'],
  'empty server protocol': ['--serve', '127.0.0.1:0', '--protocol', ''],
  'forward without serve': ['--forward', '127.0.0.1:1'],
  'forward with a query': ['--serve', '127.0.0.1:0', '--forward', '127.0.0.1:1', '()'],
  'forward with graphql': ['--serve', '127.0.0.1:0', '--forward', '127.0.0.1:1', '--graphql'],
  'forward to stdio with an address': ['--serve', '127.0.0.1:0', '--forward', 'stdio:x'],
  'forward to a unix endpoint without address': ['--serve', '127.0.0.1:0', '--forward', 'unix:'],
  'serve on a port that is no number': ['--serve', 'bogus:x'],
  'serve on stdio with an address': ['--serve', 'stdio:x'],
  'serve on a unix endpoint without address': ['--serve', 'unix:'],
};

/** The queries every client sends, in order, with the options of its protocol. */
const SESSION = [
  [],
  ['() ((1 1))'],
  ['--query', '((1 *)) ((1 *))'],
  ['--protocol', ' Binary ', '() ((2 2))'],
  ['--packed-widths', '() ((3 3))'],
  ['--external-references', '--arity', '1..', '() ((4 4))'],
  ['() ((type: type type))'],
  ['--protocol', 'text', ''],
  ['--protocol', 'binary', '--arity', '2', ''],
  ['((4: 4 4)) ()'],
  ['(nonsense'],
  ['((9 9)) ((9 9))'],
  [''],
];

/** One clink serving `args` in `cwd`: its address and a `stop` that ends it with SIGTERM. */
async function serve([command, ...prefix], args, cwd) {
  const child = spawn(command, [...prefix, ...args], { cwd, env: ENV });
  let [stdout, stderr] = ['', ''];
  child.stderr.on('data', (chunk) => (stderr += chunk));
  const closed = new Promise((resolve) =>
    child.on('close', (status, signal) => resolve({ status, signal })),
  );
  const address = await new Promise((resolve, reject) => {
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
      const banner = /^clink server listening on (\S+)\n/.exec(stdout);
      if (banner !== null) resolve(banner[1]);
    });
    closed.then(() => reject(new Error(`${command} ended: ${stderr}`)));
  });
  const stop = async () => {
    child.kill('SIGTERM');
    const ended = await closed;
    return { ...ended, stdout: stdout.replaceAll(address, '<address>'), stderr };
  };
  return { address, stop };
}

/** Runs one client to the end. */
function client([command, ...prefix], address, args) {
  const result = spawnSync(command, [...prefix, '--connect', address, ...args], {
    env: ENV,
    encoding: 'utf8',
    timeout: 60_000,
  });
  if (result.error) throw result.error;
  return { status: result.status, stdout: result.stdout, stderr: result.stderr };
}

/** Every file under `root`, by name. */
const files = (root) =>
  Object.fromEntries(readdirSync(root).map((name) => [name, readFileSync(join(root, name))]));

/** A Rust and a JavaScript server of the same `args`, each in a sandbox of its own. */
async function bothServers(args, body) {
  const roots = {
    rust: mkdtempSync(join(tmpdir(), 'clink-rust-')),
    js: mkdtempSync(join(tmpdir(), 'clink-js-')),
  };
  try {
    const rust = await serve(CLINK.rust, args, roots.rust);
    let js;
    try {
      js = await serve(CLINK.js, args, roots.js);
    } catch (error) {
      await rust.stop();
      throw error;
    }
    try {
      await body({ rust: rust.address, js: js.address });
    } finally {
      const [rustEnd, jsEnd] = await Promise.all([rust.stop(), js.stop()]);
      assert.deepEqual(jsEnd, rustEnd, 'how the servers ended');
    }
    const [rustFiles, jsFiles] = [files(roots.rust), files(roots.js)];
    assert.deepEqual(Object.keys(jsFiles).sort(), Object.keys(rustFiles).sort());
    for (const [name, bytes] of Object.entries(rustFiles)) {
      assert.ok(jsFiles[name].equals(bytes), `the bytes of ${name}`);
    }
  } finally {
    for (const root of Object.values(roots)) rmSync(root, { recursive: true, force: true });
  }
}

/** Sends the session with the `clientKind` client to both servers and compares every reply. */
async function compareSession(clientKind, addresses) {
  for (const args of SESSION) {
    const rust = client(CLINK[clientKind], addresses.rust, args);
    const js = client(CLINK[clientKind], addresses.js, args);
    assert.deepEqual(js, rust, `the ${clientKind} client sending ${JSON.stringify(args)}`);
  }
}

describe('clink protocol refusals', () => {
  for (const [name, args] of Object.entries(REFUSED)) {
    it(name, { skip }, () => differential([{ args }]));
  }
});

describe('clink protocol servers', () => {
  for (const clientKind of ['rust', 'js']) {
    it(`serve the ${clientKind} client alike`, LIMITS, () =>
      bothServers(['--db', 'db.links', '--serve', '127.0.0.1:0'], (addresses) =>
        compareSession(clientKind, addresses),
      ),
    );
  }

  it('serve only text with --protocol text', LIMITS, () =>
    bothServers(['--db', 'db.links', '--serve', '127.0.0.1:0', '--protocol', 'text'], (addresses) =>
      compareSession('js', addresses),
    ),
  );

  it('relay, each to a server of the other kind', LIMITS, async () => {
    // A Rust and a JavaScript server behind a JavaScript and a Rust relay:
    // the replies through each relay are those of a direct connection.
    await bothServers(['--db', 'db.links', '--serve', '127.0.0.1:0'], async (servers) => {
      const relayArgs = (target) => ['--serve', '127.0.0.1:0', '--forward', target];
      const root = mkdtempSync(join(tmpdir(), 'clink-relay-'));
      const jsRelay = await serve(CLINK.js, relayArgs(servers.rust), root);
      const rustRelay = await serve(CLINK.rust, relayArgs(servers.js), root);
      try {
        await compareSession('rust', { rust: jsRelay.address, js: rustRelay.address });
      } finally {
        const [jsEnd, rustEnd] = await Promise.all([jsRelay.stop(), rustRelay.stop()]);
        rmSync(root, { recursive: true, force: true });
        assert.deepEqual(jsEnd, rustEnd, 'how the relays ended');
      }
    });
  });
});
