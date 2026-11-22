// Tests of src/main-protocol.js: the `--connect`, `--serve` and `--forward`
// commands of `clink` over real TCP sockets on port 0, with the standard
// streams and the stop signals injected.
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, test } from 'node:test';
import { Cli } from '../src/cli.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { LinksTokenTable } from '../src/permissions-tokens.js';
import { PermissionPolicy } from '../src/permissions.js';
import { ArityRange } from '../src/protocol-binary-packet.js';
import { BinaryLinoOptions } from '../src/protocol-binary.js';
import {
  binaryOptions,
  printBanner,
  runClient,
  runRelay,
  runServer,
} from '../src/main-protocol.js';
import { pause, traced, until } from './helpers/protocol-server.mjs';

const TIMEOUT = { timeout: 20_000 };
const dir = mkdtempSync(join(tmpdir(), 'clink-main-protocol-'));
after(() => rmSync(dir, { recursive: true, force: true }));
let stores = 0;

/** What Rust's `clink --connect A --token secret ''` prints for a store that granted `secret`. */
const TOKEN_STORE =
  '(Token: Token Token)\n' +
  "('sha256:2bb80d537b1da3e38bd30361aa855686bde0eacd7162fef6a25fe97bf527a25b': Token " +
  "'sha256:2bb80d537b1da3e38bd30361aa855686bde0eacd7162fef6a25fe97bf527a25b')\n";

/** Parses a `clink` command line. */
const cliOf = (...args) => Cli.parse(args).cli;

/** A stream that keeps what is written to it. */
function sink() {
  const chunks = [];
  return {
    write: (text) => chunks.push(String(text)),
    text: () => chunks.join(''),
  };
}

/** A new store in the temporary directory. */
function openStore() {
  stores += 1;
  return NamedTypesDecorator.open(join(dir, `store-${stores}.links`), { stderr: sink() });
}

/** The endpoints a banner announced. */
const announced = (stream) =>
  [...stream.text().matchAll(/^clink server listening on (.+)$/gm)].map((match) => match[1]);

/**
 * Starts `runServer` (or `runRelay` with `--forward`) on port 0 and resolves
 * once it announces its endpoint; `stop()` sends SIGINT and awaits the end.
 * With `processIo` the signals and standard error are those of the process.
 */
async function start(
  args,
  { storage = openStore(), policy = null, subject = 0, tokens, processIo = false } = {},
) {
  const signals = processIo ? process : new EventEmitter();
  const stdout = sink();
  const stderr = sink();
  const io = processIo ? { stdout } : { stdout, stderr, signals };
  const cli = cliOf('--serve', '127.0.0.1:0', ...args);
  const running =
    cli.forward == null
      ? runServer(cli, storage, policy, subject, tokens, io)
      : runRelay(cli, cli.forward, io);
  let failed = null;
  running.catch((error) => {
    failed = error;
  });
  await until(() => failed !== null || announced(stdout).length > 0);
  if (failed !== null) throw failed;
  const [address] = announced(stdout);
  return {
    address,
    storage,
    stdout,
    stderr,
    signals,
    running,
    async stop(signal = 'SIGINT') {
      signals.emit(signal);
      await running;
      if (!processIo) {
        assert.equal(signals.listenerCount('SIGINT'), 0);
        assert.equal(signals.listenerCount('SIGTERM'), 0);
      }
    },
  };
}

/** What `action` writes on the standard error of the process, kept from it. */
function withProcessStderr(action) {
  const write = process.stderr.write;
  const lines = [];
  process.stderr.write = (text) => lines.push(String(text));
  const restore = () => {
    process.stderr.write = write;
    return lines;
  };
  let result;
  try {
    result = action();
  } catch (error) {
    restore();
    throw error;
  }
  return result instanceof Promise ? result.finally(restore).then(() => lines) : restore();
}

/** Runs `body` with a server started by `start`, stopping it with SIGINT however `body` ends. */
async function serving(args, options, body) {
  const server = await start(args, options);
  try {
    await body(server);
  } finally {
    await server.stop();
  }
  return server;
}

/** Runs `clink --connect address args…` and returns its standard output. */
async function connectTo(address, ...args) {
  const stdout = sink();
  await runClient(cliOf('--connect', address, ...args), address, { stdout });
  return stdout.text();
}

test('binaryOptions takes the binary options of the command line', () => {
  assert.ok(binaryOptions(cliOf()).equals(new BinaryLinoOptions()));
  const options = binaryOptions(
    cliOf('--external-references', '--arity', '2..3', '--packed-widths'),
  );
  assert.ok(
    options.equals(
      new BinaryLinoOptions({
        externalReferences: true,
        arity: new ArityRange(2n, 3n),
        packedWidths: true,
      }),
    ),
  );
});

test('printBanner prints on standard output, or on standard error beside stdio', () => {
  const stdout = sink();
  const stderr = sink();
  printBanner(['127.0.0.1:1', 'unix:/tmp/a.sock', 'stdio:x'], { stdout, stderr });
  assert.equal(
    stdout.text(),
    'clink server listening on 127.0.0.1:1\n' +
      'clink server listening on unix:/tmp/a.sock\n' +
      'clink server listening on stdio:x\n',
  );
  assert.equal(stderr.text(), '');
  printBanner(['127.0.0.1:1', 'stdio'], { stdout, stderr });
  assert.equal(
    stderr.text(),
    'clink server listening on 127.0.0.1:1\nclink server listening on stdio\n',
  );
});

test('printBanner defaults to the process streams', () => {
  const write = process.stdout.write;
  const lines = [];
  process.stdout.write = (text) => lines.push(text);
  try {
    printBanner(['127.0.0.1:2']);
  } finally {
    process.stdout.write = write;
  }
  assert.deepEqual(lines, ['clink server listening on 127.0.0.1:2\n']);
  const errors = withProcessStderr(() => printBanner(['stdio']));
  assert.deepEqual(errors, ['clink server listening on stdio\n']);
});

test('runServer and runRelay stop on the signals of the process', TIMEOUT, async () => {
  const before = [process.listenerCount('SIGINT'), process.listenerCount('SIGTERM')];
  const server = await start([], { processIo: true });
  let relayed = null;
  try {
    const errors = await withProcessStderr(async () => {
      relayed = await start(['--forward', server.address, '--trace'], { processIo: true });
    });
    assert.equal(process.listenerCount('SIGINT'), before[0] + 2);
    assert.equal(process.listenerCount('SIGTERM'), before[1] + 2);
    assert.deepEqual(errors, [`clink: forwarding to tcp:${server.address}\n`]);
    assert.equal(await connectTo(relayed.address, '() ((1 1))'), '() ((1: 1 1))\n');
  } finally {
    // One SIGINT stops both.
    await server.stop();
    await relayed?.running;
  }
  assert.deepEqual([process.listenerCount('SIGINT'), process.listenerCount('SIGTERM')], before);
});

test('runServer refuses a query and an unknown protocol before listening', async () => {
  const storage = openStore();
  for (const args of [['--query', '()'], ['()']]) {
    await assert.rejects(runServer(cliOf('--serve', '127.0.0.1:0', ...args), storage, null, 0), {
      message: '--serve does not take a query; send queries with --connect.',
    });
  }
  await assert.rejects(
    runServer(cliOf('--serve', '127.0.0.1:0', '--protocol', ' xml '), storage, null, 0),
    { message: "Invalid --protocol value 'xml'. Use 'any', 'text' or 'binary'." },
  );
});

test('runServer serves text and binary clients until SIGINT, then saves', TIMEOUT, async () => {
  const server = await start(['--protocol', 'ANY']);
  let saved = 0;
  const save = server.storage.save.bind(server.storage);
  server.storage.save = () => {
    saved += 1;
    save();
  };
  try {
    assert.match(server.address, /^127\.0\.0\.1:\d+$/);
    assert.equal(server.stderr.text(), '');
    // The replies of Rust's server to the same clients: no query reads every link,
    // and an empty store replies nothing.
    assert.equal(await connectTo(server.address), '');
    assert.equal(await connectTo(server.address, '() ((1 1))'), '() ((1: 1 1))\n');
    assert.equal(
      await connectTo(server.address, '--query', '((1 *)) ((1 *))'),
      '((1: 1 1)) ((1: 1 0))\n',
    );
    // The binary protocol, by name (trimmed, any case) or implied by a binary option.
    const binary = await connectTo(server.address, '--protocol', ' Binary ', '() ((2 2))');
    assert.equal(binary, '() ((2: 2 2))\n');
    assert.equal(
      await connectTo(server.address, '--packed-widths', '() ((3 3))'),
      '() ((3: 3 3))\n',
    );
    const all = '(1: 1 0)\n(2: 2 2)\n(3: 3 3)\n';
    assert.equal(await connectTo(server.address, '--protocol', 'text', ''), all);
    assert.equal(await connectTo(server.address, '--protocol', 'binary', '--arity', '2', ''), all);
  } finally {
    // Serving to the end saves the store once more.
    const before = saved;
    await server.stop();
    assert.equal(saved, before + 1);
  }
  assert.deepEqual(
    server.storage.allLinks().map((link) => [link.index, link.source, link.target]),
    [
      [1, 1, 0],
      [2, 2, 2],
      [3, 3, 3],
    ],
  );
});

test('runServer stops on SIGTERM and honours --protocol text and --trace', TIMEOUT, async () => {
  const lines = await traced(async (seen) => {
    const server = await start(['--protocol', 'text', '--trace']);
    try {
      assert.equal(await connectTo(server.address, '() ((1 1))'), '() ((1: 1 1))\n');
      await assert.rejects(connectTo(server.address, '--protocol', 'binary', '()'), {
        message: 'server error: this server does not accept this protocol',
      });
      await until(() => seen.filter((line) => line === '[server] client hung up').length === 2);
    } finally {
      await server.stop('SIGTERM');
    }
  });
  // The lines of Rust's server; when the first connection's hang-up is seen
  // depends on the scheduling of the connections, in Rust as here.
  assert.deepEqual(lines.slice(0, 3), [
    '[server] tcp connection accepted',
    '[server] request: () ((1 1))',
    '[server] reply: () ((1: 1 1))',
  ]);
  assert.deepEqual(lines.slice(3).sort(), [
    '[server] client hung up',
    '[server] client hung up',
    '[server] tcp connection accepted',
  ]);
});

test('runClient refuses protocols it does not speak', async () => {
  await assert.rejects(runClient(cliOf('--protocol', 'xml'), '127.0.0.1:1'), {
    message: "Invalid --protocol value 'xml'. Use 'text' or 'binary'.",
  });
  await assert.rejects(
    runClient(cliOf('--protocol', ' TEXT ', '--external-references'), '127.0.0.1:1'),
    {
      message: 'Binary protocol options require --protocol binary.',
    },
  );
});

test('runClient writes to the process standard output by default', TIMEOUT, async () => {
  const server = await start([]);
  const write = process.stdout.write;
  const lines = [];
  process.stdout.write = (text) => lines.push(text);
  try {
    await runClient(cliOf('() ((1 1))'), server.address);
  } finally {
    process.stdout.write = write;
    await server.stop();
  }
  assert.deepEqual(lines, ['() ((1: 1 1))\n']);
});

test('runServer authenticates the tokens of the data store', TIMEOUT, async () => {
  const storage = openStore();
  new LinksTokenTable(storage).grant('secret', 1);
  await serving([], { storage, tokens: { store: null } }, async ({ address }) => {
    // The token table holds the `Token` point and the token, as `--grant-token` writes them.
    assert.equal(await connectTo(address, '--token', 'secret', ''), TOKEN_STORE);
    await assert.rejects(connectTo(address, '--token', 'wrong', '()'), {
      message: 'server error: Forbidden: authentication failed',
    });
  });
});

test('runServer authenticates the tokens of a store apart', TIMEOUT, async () => {
  const tokens = openStore();
  new LinksTokenTable(tokens).grant('apart', 7);
  await serving([], { tokens: { store: tokens } }, async ({ address }) => {
    assert.equal(await connectTo(address, '--token', 'apart', ''), '');
  });
});

test('runServer shows each subject what the policy lets it see', TIMEOUT, async () => {
  const storage = openStore();
  storage.create(0, 0);
  const options = { storage, policy: PermissionPolicy.inData(storage), subject: 9 };
  await serving([], options, async ({ address }) => {
    // Subject 9 is granted nothing, so it sees no link.
    assert.equal(await connectTo(address, ''), '');
  });
  // The link and the six permission markers `PermissionPolicy.inData` names.
  assert.equal(storage.allLinks().length, 7);
});

test('runRelay refuses what a relay cannot do', async () => {
  await assert.rejects(runRelay(cliOf('--forward', '127.0.0.1:1'), '127.0.0.1:1'), {
    message: '--forward relays the connections of --serve; give --serve as well.',
  });
  for (const args of [['--query', '()'], ['()']]) {
    await assert.rejects(
      runRelay(cliOf('--serve', '127.0.0.1:0', '--forward', 'x', ...args), 'x'),
      { message: '--forward does not take a query; send queries with --connect.' },
    );
  }
  await assert.rejects(
    runRelay(cliOf('--serve', '127.0.0.1:0', '--forward', 'x', '--graphql'), 'x'),
    { message: '--forward and --graphql cannot be used together.' },
  );
  // The target parses before anything listens.
  await assert.rejects(
    runRelay(cliOf('--serve', '127.0.0.1:0', '--forward', 'stdio:x'), 'stdio:x'),
    { message: '`stdio` takes no address' },
  );
});

test('runRelay relays connections byte for byte until SIGINT', TIMEOUT, async () => {
  await serving([], {}, async (server) => {
    const args = ['--forward', server.address, '--trace'];
    await serving(args, {}, async (relayed) => {
      // Rust prints the parsed `Endpoint`, whose display names the transport.
      assert.equal(relayed.stderr.text(), `clink: forwarding to tcp:${server.address}\n`);
      assert.equal(await connectTo(relayed.address, '() ((1 1))'), '() ((1: 1 1))\n');
      assert.equal(await connectTo(relayed.address, '--protocol', 'binary', ''), '(1: 1 1)\n');
    });
  });
});

test('runRelay stops on SIGTERM and without --trace prints only the banner', TIMEOUT, async () => {
  await serving([], {}, async (server) => {
    const relayed = await start(['--forward', server.address]);
    assert.equal(relayed.stderr.text(), '');
    assert.equal(relayed.stdout.text(), `clink server listening on ${relayed.address}\n`);
    await pause(1);
    await relayed.stop('SIGTERM');
  });
});
