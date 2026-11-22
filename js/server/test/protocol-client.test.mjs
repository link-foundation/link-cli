// The client of a links server (src/protocol-client.js): ports the client
// cases of rust/tests/query_and_client_edges_tests.rs, with its `scripted`
// server, and the client paths the Rust protocol tests reach through a real
// server (queries, replies of errors, a hung-up connection).
import { after, describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { listen } from '../src/ipc.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { LinksClient } from '../src/protocol-client.js';
import { IoError, ProtocolError } from '../src/protocol-errors.js';
import { parseDocument } from '../src/protocol-lino.js';
import { LinoConnection, TextLinoProtocol } from '../src/protocol-protocols.js';
import { LinksServer, ServerOptions } from '../src/protocol-server.js';
import { scratchDirectory } from './helpers/storage.mjs';

const TIMEOUT = { timeout: 30_000 };
const PUSH_TIMEOUT = 5_000;

/**
 * A server that, step by step, writes the documents of a step and then reads
 * one request; it hangs up when the steps run out or the client does. Rust's
 * `scripted` holds the connection for one more request after its steps.
 */
async function scripted(steps, { hold = true } = {}) {
  const listener = await listen('127.0.0.1:0');
  const address = `127.0.0.1:${listener.localAddr().port}`;
  const answering = (async () => {
    const stream = await listener.accept();
    listener.close();
    const connection = new LinoConnection(stream, new TextLinoProtocol());
    try {
      let open = true;
      for (const documents of steps) {
        for (const text of documents) await connection.send(parseDocument(text));
        if ((await connection.receive()) === null) {
          open = false;
          break;
        }
      }
      if (open && hold) await connection.receive();
    } catch {
      // The client hung up.
    }
    stream.shutdown();
  })();
  const client = await LinksClient.connect(address, new TextLinoProtocol());
  after(async () => {
    client.stream().shutdown();
    await answering;
  });
  return client;
}

/** Serves a fresh named store on 127.0.0.1:0 until the test file ends. */
async function startServer() {
  const store = NamedTypesDecorator.open(join(scratchDirectory(), 'client.links'));
  const server = await LinksServer.listen('127.0.0.1:0', new ServerOptions());
  const serving = server.serve(store);
  after(async () => {
    server.shutdownHandle().shutdown();
    await serving;
    server.disconnectAll();
  });
  return `127.0.0.1:${server.localAddr().port}`;
}

/** A client of `address` that hangs up when the test file ends. */
async function connected(address, open = LinksClient.connect) {
  const client = await open(address, new TextLinoProtocol());
  after(() => client.stream().shutdown());
  return client;
}

const isMalformed = (text) => (error) =>
  error instanceof ProtocolError && error.kind === 'malformed' && error.detail.includes(text);

describe('links client', TIMEOUT, () => {
  it('a notification before a reply is kept for later', async () => {
    const client = await scripted([['(notify: 1)', '(reply)'], []]);
    assert.equal((await client.request(parseDocument('(any)'))).length, 1);
    assert.equal(client.pendingNotifications(), 1);
    const queued = await client.receiveNotification(PUSH_TIMEOUT);
    assert.deepEqual(queued, parseDocument('(notify: 1)'));
    assert.equal(client.pendingNotifications(), 0);
  });

  it('a reply nobody asked for is malformed', async () => {
    const client = await scripted([['(reply)'], []]);
    await assert.rejects(client.receiveNotification(PUSH_TIMEOUT), isMalformed('nobody asked for'));
  });

  it('a push that arrives while nothing waits is received', async () => {
    const client = await scripted([['(notify: 2)'], []]);
    assert.deepEqual(await client.receiveNotification(PUSH_TIMEOUT), parseDocument('(notify: 2)'));
    assert.equal(client.pendingNotifications(), 0);
  });

  it('no push in time is null', async () => {
    const client = await scripted([[]]);
    assert.equal(await client.receiveNotification(50), null);
  });

  it('a hung-up connection fails a wait for a push', async () => {
    const client = await scripted([], { hold: false });
    await assert.rejects(client.receiveNotification(PUSH_TIMEOUT), (error) => {
      assert.ok(error instanceof ProtocolError);
      assert.equal(error.kind, 'io');
      assert.ok(error.cause instanceof IoError);
      assert.equal(error.cause.kind, 'UnexpectedEof');
      assert.equal(error.message, 'I/O error: connection closed');
      return true;
    });
  });

  it('a connection closed before the reply is malformed', async () => {
    const client = await scripted([[]], { hold: false });
    await assert.rejects(client.query('(any)'), isMalformed('connection closed before the reply'));
  });

  it('an error reply is a remote error', async () => {
    const client = await scripted([['(error: boom)'], []]);
    await assert.rejects(client.query('(any)'), (error) => {
      assert.ok(error instanceof ProtocolError);
      assert.equal(error.kind, 'remote');
      assert.equal(error.message, 'server error: boom');
      return true;
    });
  });

  it('a subject reply of any other shape is malformed', async () => {
    for (const reply of [
      '(subject: x)',
      '(other: 5)',
      '(subject: 99999999999999999999)',
      '(subject: 4294967296)',
      '(subject: 1 2)',
      'subject',
      '(subject: 1) (subject: 2)',
    ]) {
      const client = await scripted([[reply], []]);
      await assert.rejects(client.authenticate('token'), isMalformed('subject'), reply);
    }
  });

  it('authenticate returns the subject the server names', async () => {
    for (const [reply, subject] of [
      ['(subject: 7)', 7],
      ['(subject: 4294967295)', 0xffffffff],
    ]) {
      const client = await scripted([[reply], []]);
      assert.equal(await client.authenticate('token'), subject);
    }
  });

  it('queries a served store as documents, lossless envelopes and text', async () => {
    const address = await startServer();
    const client = await connected(address);
    assert.equal(client.kind(), 'tcp');
    assert.equal(client.stream().kind, 'tcp');
    assert.deepEqual(await client.query('() ((1 1))'), parseDocument('() ((1: 1 1))'));
    assert.deepEqual(await client.queryLossless('() ((2 2))'), parseDocument('() ((2: 2 2))'));
    assert.equal(await client.queryText(''), '(1: 1 1)\n(2: 2 2)');
    assert.equal(await client.queryText('((1: 1 1)) ((1: 1 1))'), '((1: 1 1)) ((1: 1 1))');
  });

  it('wraps a connected stream and connects to endpoints', async () => {
    const address = await startServer();
    const viaEndpoint = await connected(address, LinksClient.connectEndpoint);
    assert.equal(await viaEndpoint.queryText('() ((1 1))'), '() ((1: 1 1))');

    const borrowed = await LinksClient.connect(address, new TextLinoProtocol());
    const wrapped = LinksClient.fromStream(borrowed.stream(), new TextLinoProtocol());
    after(() => wrapped.stream().shutdown());
    assert.equal(wrapped.stream(), borrowed.stream());
    assert.equal(await wrapped.queryText(''), '(1: 1 1)');
    assert.ok(new LinksClient(wrapped.stream(), new TextLinoProtocol()) instanceof LinksClient);
  });

  it('a failed connection is an I/O protocol error', async () => {
    const listener = await listen('127.0.0.1:0');
    const address = `127.0.0.1:${listener.localAddr().port}`;
    listener.close();
    await assert.rejects(LinksClient.connect(address, new TextLinoProtocol()), (error) => {
      assert.ok(error instanceof ProtocolError);
      assert.equal(error.kind, 'io');
      assert.ok(error.cause !== undefined);
      return true;
    });
  });
});
