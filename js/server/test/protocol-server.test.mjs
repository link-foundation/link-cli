// A links server over real sockets (src/protocol-server.js): ports
// rust/tests/protocol_tcp_tests.rs, the server cases of
// rust/tests/subscriptions_tests.rs (with its replay of
// docs/protocol/subscriptions.txt over every protocol) and the scripted-client
// cases of rust/tests/query_and_client_edges_tests.rs, over TCP on
// 127.0.0.1:0 and over a unix socket. Trace lines are compared with the texts
// of the Rust `eprintln!`s, read from a Rust server started with `--trace`.
// The failures only fakes can produce are in protocol-server-faults.test.mjs.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { connect, listen } from '../src/ipc.js';
import { link, NULL_LINK } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { ArityRange } from '../src/protocol-binary-packet.js';
import { BinaryLinoOptions } from '../src/protocol-binary.js';
import { ProtocolError } from '../src/protocol-errors.js';
import { LinksOperation } from '../src/protocol-links-operations.js';
import { formatDocument, parseDocument } from '../src/protocol-lino.js';
import {
  BinaryLinoProtocol,
  LinoConnection,
  MessageFormat,
  TextLinoProtocol,
} from '../src/protocol-protocols.js';
import { RemoteLinks } from '../src/protocol-remote-links.js';
import {
  AcceptedProtocols,
  DEFAULT_MAX_PENDING_NOTIFICATIONS,
  LinksServer,
  ServerOptions,
} from '../src/protocol-server.js';
import { LinksSessions } from '../src/protocol-sessions.js';
import { QueryLimits } from '../src/query-options.js';
import { World } from './permissions-world.mjs';
import { PUSH_TIMEOUT, clientOf, traced, until } from './helpers/protocol-server.mjs';
import { scratchDirectory } from './helpers/storage.mjs';

const SILENCE_TIMEOUT = 200;
const TIMEOUT = { timeout: 60_000 };
const FIXTURE = new URL('../../../docs/protocol/subscriptions.txt', import.meta.url);

/** Text, and binary with every option, as the `protocols()` of the Rust tests. */
function protocols() {
  const all = [new TextLinoProtocol()];
  for (const externalReferences of [false, true]) {
    for (const arity of [ArityRange.DOUBLETS, ArityRange.between(2, 3), ArityRange.atLeast(1)]) {
      for (const packedWidths of [false, true]) {
        const options = new BinaryLinoOptions({ externalReferences, arity, packedWidths });
        all.push(new BinaryLinoProtocol({ options }));
      }
    }
  }
  return all;
}

let stores = 0;

/** A fresh named store in a scratch directory, as `RunningServer::start(_, true)`. */
function namedStore() {
  stores += 1;
  return NamedTypesDecorator.open(join(scratchDirectory(), `store-${stores}.links`));
}

/**
 * `RunningServer`: listens on `endpoint`, serves (`serve(server)`, or a fresh
 * store) and runs `body`; then stops the server and hangs up every client.
 */
async function withServer(options, body, settings = {}) {
  const { named = false, endpoint = '127.0.0.1:0', serve = null, prepare = (s) => s } = settings;
  const server = prepare(await LinksServer.listen(endpoint, new ServerOptions(options)));
  const serving =
    serve === null ? server.serve(named ? namedStore() : LinkStorage.inMemory()) : serve(server);
  const clients = [];
  let stopped = false;
  const address = endpoint.startsWith('127.0.0.1:')
    ? `127.0.0.1:${server.localAddr().port}`
    : server.endpoint();
  const running = {
    server,
    address,
    serving,
    async client(protocol = new TextLinoProtocol()) {
      const client = await clientOf(address, protocol);
      clients.push(client);
      return client;
    },
    async remote(protocol = new TextLinoProtocol()) {
      return new RemoteLinks(await running.client(protocol));
    },
    async stop() {
      if (stopped) return;
      stopped = true;
      server.shutdownHandle().shutdown();
      try {
        await serving;
      } finally {
        server.disconnectAll();
        for (const client of clients) client.stream().shutdown();
      }
    },
  };
  try {
    return await body(running);
  } finally {
    await running.stop();
  }
}

/** A raw connection to `address`, its reads failing after ten seconds. */
async function raw(address) {
  const stream = await connect(address);
  stream.setReadTimeout(PUSH_TIMEOUT);
  return stream;
}

/** Everything `stream` sends until it hangs up, as text. */
async function readToEnd(stream) {
  const chunks = [];
  for (;;) {
    const chunk = await stream.read(65536);
    if (chunk.length === 0) return Buffer.concat(chunks).toString('utf8');
    chunks.push(chunk);
  }
}

/** Reads `stream` until `count` lines arrived; returns them with their newlines. */
async function readLines(stream, count, pending = { text: '' }) {
  while (pending.text.split('\n').length <= count) {
    pending.text += (await stream.read(65536)).toString('utf8');
  }
  const lines = pending.text.split('\n');
  pending.text = lines.slice(count).join('\n');
  return lines.slice(0, count).map((line) => `${line}\n`);
}

test('crud works over every protocol', TIMEOUT, async () => {
  for (const [number, protocol] of protocols().entries()) {
    const name = `protocol ${number}`;
    await withServer({}, async (server) => {
      const client = await server.client(protocol);
      // Create.
      assert.equal(await client.queryText('() ((1 1))'), '() ((1: 1 1))');
      assert.equal(await client.queryText('() ((2 2))'), '() ((2: 2 2))');
      // Read.
      assert.equal(await client.queryText(''), '(1: 1 1)\n(2: 2 2)', name);
      assert.equal(await client.queryText('((1: 1 1)) ((1: 1 1))'), '((1: 1 1)) ((1: 1 1))');
      // A query that creates and deletes nothing changes nothing.
      assert.equal(await client.queryText('() ()'), '', name);
      // Update.
      assert.equal(await client.queryText('((1: 1 1)) ((1: 1 2))'), '((1: 1 1)) ((1: 1 2))');
      // Delete.
      assert.equal(await client.queryText('((1: 1 2)) ()'), '((1: 1 2)) ()', name);
      assert.equal(await client.queryText(''), '(2: 2 2)', name);
    });
  }
});

test('text and binary replies are identical documents', TIMEOUT, async () => {
  await withServer(
    { autoCreateMissingReferences: true },
    async (server) => {
      const text = await server.client(new TextLinoProtocol());
      const options = new BinaryLinoOptions({
        externalReferences: true,
        arity: ArityRange.atLeast(1),
      });
      const binary = await server.client(new BinaryLinoProtocol({ options }));
      await text.query('() ((child: father mother))');
      await binary.query("() (('two words': 'it''s' \"x\"))");
      assert.deepEqual(await text.query(''), await binary.query(''));
      const listing = await text.queryText('');
      assert.ok(listing.includes('(child: father mother)'), listing);
      assert.ok(listing.includes("('two words': \"it's\" x)"), listing);
    },
    { named: true },
  );
});

test('query errors come back as remote errors', TIMEOUT, async () => {
  await withServer({}, async (server) => {
    for (const protocol of protocols()) {
      const client = await server.client(protocol);
      await assert.rejects(client.query('((99: 1 1)) ()'), (error) => {
        assert.ok(error instanceof ProtocolError);
        assert.equal(error.kind, 'remote');
        assert.notEqual(error.detail, '');
        return true;
      });
      // The connection stays usable after an error reply.
      assert.equal(await client.queryText(''), '');
    }
  });
});

test('a raw text session works with crlf', TIMEOUT, async () => {
  await withServer({}, async (server) => {
    const stream = await raw(server.address);
    const pending = { text: '' };
    await stream.write(Buffer.from('() ((1 1))\r\n.\r\n'));
    assert.deepEqual(await readLines(stream, 2, pending), ['() ((1: 1 1))\n', '.\n']);
    await stream.write(Buffer.from('.\n'));
    assert.deepEqual(await readLines(stream, 1, pending), ['(1: 1 1)\n']);
    stream.shutdown();
  });
});

test('malformed messages get an error and the connection closes', TIMEOUT, async () => {
  await withServer({}, async (server) => {
    const stream = await raw(server.address);
    // A binary header announcing more links than the default limit allows.
    await stream.write(Buffer.from([0x10, 0xff, 0xff, 0xff, 0xff, 0x0f]));
    // The reply of a Rust server to the same bytes.
    assert.equal(
      await readToEnd(stream),
      "(error: 'limit exceeded: packet declares more than 4194304 links')\n.\n",
    );
    stream.shutdown();
  });
});

test('a stopped server executes nothing', TIMEOUT, async () => {
  await withServer({}, async (server) => {
    const client = await server.client();
    assert.equal(await client.queryText('() ((1 1))'), '() ((1: 1 1))');
    server.server.shutdownHandle().shutdown();
    await server.serving;
    // The connection outlives `serve`, but the store is gone.
    await assert.rejects(client.queryText(''), {
      message: 'server error: server is shutting down',
    });
  });
});

test('servers can restrict the accepted protocol', TIMEOUT, async () => {
  for (const accept of [AcceptedProtocols.BINARY, AcceptedProtocols.TEXT]) {
    await withServer({ accept }, async (server) => {
      const text = await server.client(new TextLinoProtocol());
      const binary = await server.client(new BinaryLinoProtocol());
      const [refused, accepted] =
        accept === AcceptedProtocols.BINARY ? [text, binary] : [binary, text];
      await assert.rejects(refused.query('() ((1 1))'), {
        message: 'server error: this server does not accept this protocol',
      });
      assert.equal(await accepted.queryText('() ((1 1))'), '() ((1: 1 1))');
    });
  }
});

test('tracing servers keep serving, tracing as the Rust server does', TIMEOUT, async () => {
  const lines = await traced(async (lines) => {
    await withServer({ trace: true }, async (server) => {
      const first = await clientOf(server.address);
      assert.equal(await first.queryText('() ((1 1))'), '() ((1: 1 1))');
      first.stream().shutdown();
      await until(() => lines.includes('[server] client hung up'));

      const stream = await raw(server.address);
      await stream.write(Buffer.from('() ((1 1))\n'));
      await stream.closeWrite();
      assert.equal(
        await readToEnd(stream),
        '(error: "malformed message: stream ended before the \'.\' terminator line")\n.\n',
      );
      stream.shutdown();
      await until(() => lines.length === 6);

      const last = await clientOf(server.address);
      assert.equal(await last.queryText(''), '(1: 1 1)');
      last.stream().shutdown();
      await until(() => lines.length === 10);
    });
  });
  // The lines a Rust server traces for the same three connections.
  assert.deepEqual(lines, [
    '[server] tcp connection accepted',
    '[server] request: () ((1 1))',
    '[server] reply: () ((1: 1 1))',
    '[server] client hung up',
    '[server] tcp connection accepted',
    "[server] connection closed: malformed message: stream ended before the '.' terminator line",
    '[server] tcp connection accepted',
    '[server] request: ',
    '[server] reply: (1: 1 1)',
    '[server] client hung up',
  ]);
});

test('concurrent clients share one store', TIMEOUT, async () => {
  await withServer(
    {},
    async (server) => {
      const worker = async (number) => {
        const protocol = number % 2 === 0 ? new TextLinoProtocol() : new BinaryLinoProtocol();
        const client = await server.client(protocol);
        for (let item = 0; item < 5; item += 1) {
          const name = `w${number}i${item}`;
          const reply = await client.queryText(`() ((${name}: ${name} ${name}))`);
          assert.equal(reply, `() ((${name}: ${name} ${name}))`);
        }
      };
      await Promise.all([0, 1, 2, 3].map(worker));
      const listing = await (await server.client()).queryText('');
      assert.equal(listing.split('\n').length, 20, listing);
    },
    { named: true },
  );
});

test('shutdown stops the server', TIMEOUT, async () => {
  await withServer({}, async (server) => {
    const client = await server.client();
    await client.query('() ((1 1))');
    server.server.shutdownHandle().shutdown();
    await server.serving;
    await assert.rejects(client.query(''));
  });
});

test('a server listens on a unix socket, which has no TCP address', TIMEOUT, async () => {
  const path = join(scratchDirectory(), 'links.sock');
  const lines = await traced(async (lines) => {
    await withServer(
      { trace: true },
      async (server) => {
        assert.equal(server.address, `unix:${path}`);
        assert.deepEqual(server.server.endpoints(), [`unix:${path}`]);
        assert.throws(() => server.server.localAddr(), {
          name: 'ProtocolError',
          message: `I/O error: \`unix:${path}\` is not a TCP endpoint`,
        });
        const client = await server.client();
        assert.equal(client.kind(), 'unix');
        assert.equal(await client.queryText('() ((1 1))'), '() ((1: 1 1))');
        client.stream().shutdown();
        await until(() => lines.length === 4);
      },
      { endpoint: `unix:${path}` },
    );
  });
  assert.deepEqual(lines, [
    '[server] unix connection accepted',
    '[server] request: () ((1 1))',
    '[server] reply: () ((1: 1 1))',
    '[server] client hung up',
  ]);
});

test('a server listens on several endpoints at once, sharing one store', TIMEOUT, async () => {
  const path = join(scratchDirectory(), 'shared.sock');
  const server = await LinksServer.listenAll(['127.0.0.1:0', `unix:${path}`]);
  const tcp = `127.0.0.1:${server.localAddr().port}`;
  assert.deepEqual(server.endpoints(), [tcp, `unix:${path}`]);
  assert.equal(server.endpoint(), `${tcp} unix:${path}`);
  const serving = server.serve(LinkStorage.inMemory());
  const overTcp = await clientOf(tcp);
  const overUnix = await clientOf(`unix:${path}`);
  try {
    assert.equal(await overTcp.queryText('() ((1 1))'), '() ((1: 1 1))');
    assert.equal(await overUnix.queryText(''), '(1: 1 1)');
  } finally {
    server.shutdownHandle().shutdown();
    await serving;
    server.disconnectAll();
    overTcp.stream().shutdown();
    overUnix.stream().shutdown();
  }
});

test('listening failures are protocol errors', TIMEOUT, async () => {
  await assert.rejects(LinksServer.listenAll([]), {
    name: 'ProtocolError',
    message: 'I/O error: no endpoint to listen on',
  });
  const path = join(scratchDirectory(), 'taken.sock');
  const first = await LinksServer.listen(`unix:${path}`);
  try {
    await assert.rejects(LinksServer.listen(`unix:${path}`), (error) => {
      assert.ok(error instanceof ProtocolError, String(error));
      assert.equal(error.kind, 'io');
      return true;
    });
  } finally {
    first.shutdownHandle().shutdown();
  }
});

/** The steps of docs/protocol/subscriptions.txt, as the Rust `conversation()`. */
function conversation() {
  const starts = (line) => /^[AB][<>] /.test(line);
  const lines = readFileSync(FIXTURE, 'utf8')
    .split('\n')
    .map((line) => line.replace(/\r$/, ''))
    .filter((line) => line !== '' && !line.startsWith('#'));
  const steps = [];
  let start = 0;
  while (start < lines.length) {
    assert.ok(starts(lines[start]), `expected a step, found '${lines[start]}'`);
    let end = start + 1;
    while (end < lines.length && !starts(lines[end])) end += 1;
    const isPush = lines[start][1] === '<';
    const message = lines[start].slice(3);
    const expected = [...(isPush ? [message] : []), ...lines.slice(start + 1, end)];
    steps.push({ connection: lines[start][0], isPush, message, expected: expected.join('\n') });
    start = end;
  }
  return steps;
}

test('servers replay docs/protocol/subscriptions.txt over every protocol', TIMEOUT, async () => {
  const steps = conversation();
  assert.equal(steps.length, 24);
  for (const protocol of protocols()) {
    await withServer(
      {},
      async (server) => {
        const clients = { A: await server.client(protocol), B: await server.client(protocol) };
        for (const step of steps) {
          const client = clients[step.connection];
          if (step.isPush) {
            const push = await client.receiveNotification(PUSH_TIMEOUT);
            assert.notEqual(push, null, step.message);
            assert.equal(formatDocument(push), step.expected, step.message);
            continue;
          }
          if (step.expected.startsWith('! ')) {
            await assert.rejects(client.queryText(step.message), (error) =>
              error.message.endsWith(step.expected.slice(2)),
            );
          } else {
            assert.deepEqual(
              [step.message, await client.queryText(step.message)],
              [step.message, step.expected],
            );
          }
          // Every push made before this reply arrived before it, and the
          // conversation lists none.
          assert.equal(client.pendingNotifications(), 0, step.message);
        }
        assert.equal(await clients.A.receiveNotification(SILENCE_TIMEOUT), null);
        assert.equal(await clients.B.receiveNotification(SILENCE_TIMEOUT), null);
      },
      { named: true },
    );
  }
});

test(
  'a subscriber is told about another connection’s writes until it unsubscribes',
  TIMEOUT,
  async () => {
    for (const protocol of protocols()) {
      await withServer(
        {},
        async (server) => {
          const subscriber = await server.remote(protocol);
          const writer = await server.remote(protocol);
          const subscription = await subscriber.subscribe([null, null, null]);
          assert.equal(subscription, 1);
          await writer.changes(LinksOperation.create(0, 0));
          assert.deepEqual(await subscriber.receiveNotification(PUSH_TIMEOUT), {
            subscription: 1,
            changes: [[NULL_LINK, link(1, 0, 0)]],
          });
          assert.equal(await subscriber.receiveNotification(SILENCE_TIMEOUT), null);

          await subscriber.unsubscribe(subscription);
          await writer.changes(LinksOperation.create(0, 0));
          assert.equal(await subscriber.countMatching([]), 2);
          assert.equal(await subscriber.receiveNotification(SILENCE_TIMEOUT), null);
        },
        { named: true },
      );
    }
  },
);

test('a subscriber sees only what it may read', TIMEOUT, async () => {
  // A link whose source is 2 is hidden: one that becomes hidden is gone, one
  // that stops being hidden is new.
  await withServer(
    {},
    async (server) => {
      const subscriber = await server.remote();
      const writer = await server.client();
      await subscriber.subscribe([]);
      for (const request of [
        '(create: (0 0))',
        '(update: (1 2 2))',
        '(update: (1 2 1))',
        '(update: (1 1 1))',
      ]) {
        await writer.query(request);
      }
      const pushed = (before, after) => ({ subscription: 1, changes: [[before, after]] });
      const next = () => subscriber.receiveNotification(PUSH_TIMEOUT);
      assert.deepEqual(await next(), pushed(NULL_LINK, link(1, 0, 0)));
      assert.deepEqual(await next(), pushed(link(1, 0, 0), NULL_LINK));
      // A change between two hidden states is not pushed at all.
      assert.deepEqual(await next(), pushed(NULL_LINK, link(1, 1, 1)));
      assert.equal(await subscriber.receiveNotification(SILENCE_TIMEOUT), null);
    },
    { prepare: (server) => server.withReadable((value) => value.source !== 2) },
  );
});

test('a subscriber is pushed only what its subject may read', TIMEOUT, async () => {
  // Issue #149: with permissions every connection is a session of its
  // subject, and its pushes are filtered by them.
  const w = new World(false);
  const { t, v } = w;
  const sessions = LinksSessions.withPermissions(w.links, w.policy).withAuthenticator(
    (_, token) => v[token] ?? null,
  );
  await withServer(
    {},
    async (server) => {
      const aliceClient = await server.client();
      assert.equal(await aliceClient.authenticate('alice'), v.alice);
      const subscriber = new RemoteLinks(aliceClient);
      const writer = await server.client();
      assert.equal(await writer.authenticate('boss'), v.boss);
      await subscriber.subscribe([]);

      // alice may not read the secret: a change she cannot see is not pushed
      // to her at all...
      await writer.query(`(update: (${v.secret} ${t.Secret} ${v.alice}))`);
      // ...while one to a note she may read is.
      await writer.query(`(update: (${v.note} ${t.Note} ${v.alice}))`);

      assert.deepEqual(await subscriber.receiveNotification(PUSH_TIMEOUT), {
        subscription: 1,
        changes: [[link(v.note, t.Note, v.note), link(v.note, t.Note, v.alice)]],
      });
      assert.equal(await subscriber.receiveNotification(SILENCE_TIMEOUT), null);
    },
    { serve: (server) => server.serveSessions(sessions) },
  );
});

test('a subscriber that falls too far behind is disconnected', TIMEOUT, async () => {
  await withServer({ maxPendingNotifications: 0 }, async (server) => {
    const subscriber = await server.client();
    const writer = await server.client();
    await subscriber.query('(subscribe: ())');
    await writer.query('(create: (0 0))');
    await assert.rejects(async () => {
      await subscriber.receiveNotification(PUSH_TIMEOUT);
      await subscriber.query('(count: ())');
    });
    assert.equal(await writer.queryText('(count: ())'), '(count: 1)');
  });
});

test('a server in triplet mode answers and pushes triplets', TIMEOUT, async () => {
  // The replies of a Rust `clink --serve --triplets` to the same requests.
  await withServer({ triplets: true }, async (server) => {
    const subscriber = await server.client();
    const writer = await server.client();
    assert.equal(await subscriber.queryText('(subscribe: ())'), '(subscription: 1)');
    assert.equal(await writer.queryText('(create: (0 0 0))'), '() ((3: 3 0 0))');
    const push = await subscriber.receiveNotification(PUSH_TIMEOUT);
    assert.equal(formatDocument(push), '(notify: 1)\n() ((3: 3 0 0))');
    assert.equal(await writer.queryText('(count: ())'), '(count: 1)');
    assert.equal(await subscriber.queryText('(each: ())'), '(3: 3 0 0)');
    await assert.rejects(subscriber.query('(create: (0 0))'), {
      message: 'server error: malformed message: expected 3 numbers, found (0 0)',
    });
  });
});

test('serveFor serves a store without sessions', TIMEOUT, async () => {
  const store = LinkStorage.inMemory();
  let limits = null;
  // The limits the requests are read with are taken once, when serving starts.
  store.queryLimits = () => limits;
  const lines = await traced(async (lines) => {
    await withServer(
      { trace: true },
      async (server) => {
        const client = await clientOf(server.address);
        assert.equal(await client.queryText('() ((1 1))'), '() ((1: 1 1))');
        await assert.rejects(client.query('(subscribe: ())'), {
          message: 'server error: subscriptions belong to a connection to a links server',
        });
        limits = new QueryLimits({
          maxAddress: 0xffffffff,
          maxBytes: 1024,
          maxDepth: 8,
          maxNodes: 2,
          maxResults: 16,
          maxBindings: 16,
          maxBindingBytes: 1024,
        });
        // A request the store does not admit is not traced, its reply is.
        await assert.rejects(client.query('() ((1 1))'), {
          message: 'server error: protocol node limit exceeded',
        });
        limits = null;
        client.stream().shutdown();
        await until(() => lines.includes('[server] client hung up'));
        // The connection is gone: its close needs nothing from the store.
        const next = await clientOf(server.address);
        assert.equal(await next.queryText(''), '(1: 1 1)');
        next.stream().shutdown();
        await until(() => lines.length === 11);
      },
      { serve: (server) => server.serveFor(store) },
    );
  });
  assert.deepEqual(lines, [
    '[server] tcp connection accepted',
    '[server] request: () ((1 1))',
    '[server] reply: () ((1: 1 1))',
    '[server] request: (subscribe: ())',
    "[server] reply: (error: 'subscriptions belong to a connection to a links server')",
    "[server] reply: (error: 'protocol node limit exceeded')",
    '[server] client hung up',
    '[server] tcp connection accepted',
    '[server] request: ',
    '[server] reply: (1: 1 1)',
    '[server] client hung up',
  ]);
});

test('server options default as Rust’s and accepted protocols filter formats', () => {
  const options = new ServerOptions();
  assert.equal(options.trace, false);
  assert.equal(options.autoCreateMissingReferences, false);
  assert.equal(options.accept, AcceptedProtocols.ANY);
  assert.equal(options.limits.maxTextBytes, 64 << 20);
  assert.equal(options.maxPendingNotifications, DEFAULT_MAX_PENDING_NOTIFICATIONS);
  assert.equal(DEFAULT_MAX_PENDING_NOTIFICATIONS, 1024);
  assert.equal(options.triplets, false);
  const binary = MessageFormat.binary(new BinaryLinoOptions());
  const accepts = (accept) => [
    AcceptedProtocols.accepts(accept, MessageFormat.TEXT),
    AcceptedProtocols.accepts(accept, binary),
  ];
  assert.deepEqual(accepts(AcceptedProtocols.ANY), [true, true]);
  assert.deepEqual(accepts(AcceptedProtocols.TEXT), [true, false]);
  assert.deepEqual(accepts(AcceptedProtocols.BINARY), [false, true]);
});

/**
 * A server that answers the n-th request with the documents of the n-th
 * reply, as the Rust `scripted` helper does.
 */
async function scripted(replies, body) {
  const listener = await listen('127.0.0.1:0');
  const address = `127.0.0.1:${listener.localAddr().port}`;
  const answering = (async () => {
    const stream = await listener.accept();
    listener.close();
    const connection = new LinoConnection(stream, new TextLinoProtocol());
    try {
      for (const documents of replies) {
        for (const text of documents) await connection.send(parseDocument(text));
        if ((await connection.receive()) === null) break;
      }
      // Hold the connection until the client has read everything.
      await connection.receive();
    } catch {
      // The client hung up.
    }
    stream.shutdown();
  })();
  const client = await clientOf(address);
  try {
    await body(client);
  } finally {
    client.stream().shutdown();
    await answering;
  }
}

test('a notification before a reply is kept for later', TIMEOUT, async () => {
  // The first request sends a push followed by the reply it is waiting for.
  await scripted([['(notify: 1)', '(reply)'], []], async (client) => {
    assert.equal((await client.request(parseDocument('(any)'))).length, 1);
    assert.equal(client.pendingNotifications(), 1);
    const queued = await client.receiveNotification(5_000);
    assert.deepEqual(queued, parseDocument('(notify: 1)'));
    assert.equal(client.pendingNotifications(), 0);
  });
});

test('a reply nobody asked for is malformed', TIMEOUT, async () => {
  await scripted([['(reply)'], []], async (client) => {
    await assert.rejects(client.receiveNotification(5_000), (error) => {
      assert.equal(error.kind, 'malformed');
      assert.ok(error.detail.includes('nobody asked for'), error.message);
      return true;
    });
  });
});

test('a subject reply of any other shape is malformed', TIMEOUT, async () => {
  for (const reply of [
    '(subject: x)',
    '(other: 5)',
    '(subject: 99999999999999999999)',
    '(subject: 1 2)',
    'subject',
  ]) {
    await scripted([[reply], []], async (client) => {
      await assert.rejects(client.authenticate('token'), (error) => {
        assert.equal(error.kind, 'malformed', reply);
        assert.ok(error.detail.includes('subject'), reply);
        return true;
      });
    });
  }
});
