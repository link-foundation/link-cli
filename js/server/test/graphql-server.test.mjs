// The HTTP and WebSocket edges of the GraphQL server: the cases of
// rust/tests/graphql_server_protocol.rs and the socket cases of
// rust/tests/graphql_server.rs, with the server in this process over Deep's
// core (deep-core.json), and how `TcpListener::bind` resolves its address.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { lookup } from 'node:dns/promises';
import { createServer } from 'node:net';

import { Settings } from '../src/graphql-execute.js';
import {
  GraphqlServer,
  frame,
  message,
  sendFrame,
  socketAddresses,
} from '../src/graphql-server.js';
import { deepStore } from './helpers/graphql.mjs';
import { Wire, clientFrame, openSocket } from './helpers/graphql-wire.mjs';

const INSERT = 'mutation { insert_links(objects: {type_id: 1}) { returning { id } } }';
const WATCH =
  'subscription { links(where: {type_id: {_eq: 1}}, order_by: {id: desc}, limit: 1) { id } }';
const LIMITS = { timeout: 60_000 };

/** Runs `body` with a server over Deep's core, its secret `s3` and admin secret `admin`. */
async function withServer(admin, body) {
  const { store } = deepStore({ core: true });
  const server = await GraphqlServer.bind('127.0.0.1:0');
  const served = server.serve(store, new Settings(Buffer.from('s3'), admin));
  const wires = [];
  const connect = async (options) => {
    const wire = await Wire.connect(server.localAddr(), options);
    wires.push(wire);
    return wire;
  };
  const socket = async (options) => {
    const wire = await openSocket(server.localAddr(), options);
    wires.push(wire);
    return wire;
  };
  try {
    await body({ address: server.localAddr(), connect, socket, server });
  } finally {
    server.shutdown();
    await served;
    for (const wire of wires) wire.destroy();
  }
}

/** The body of a POST on a connection of its own. */
async function postText(connect, body, headers = '') {
  const wire = await connect();
  return wire.post(body, headers);
}

async function post(connect, query, headers = '') {
  return JSON.parse(await postText(connect, JSON.stringify({ query }), headers));
}

describe('HTTP', LIMITS, () => {
  it('routes share one connection', () =>
    withServer(null, async ({ connect }) => {
      const wire = await connect();
      for (const path of ['/healthz', '/api/healthz', '/v1/healthz', '/healthz?full=1']) {
        const [head, body] = await wire.exchange(`GET ${path} HTTP/1.1\r\n\r\n`);
        assert.ok(head.startsWith('HTTP/1.1 200'), head);
        assert.equal(body, '{"status":"ok"}');
      }
      for (const path of ['/v1/version', '/v1/version/']) {
        const request = `GET ${path} HTTP/1.1\r\nno colon on this line\r\n\r\n`;
        assert.equal((await wire.exchange(request))[1], '{"version":"clink"}');
      }
      for (const request of [
        'GET /missing HTTP/1.1\r\n\r\n',
        'POST /missing HTTP/1.1\r\n\r\n',
        'GET /v1/graphql HTTP/1.1\r\n\r\n',
        // Only GraphQL paths upgrade, and only to a WebSocket.
        'GET /missing HTTP/1.1\r\nUpgrade: websocket\r\n\r\n',
        'GET /v1/graphql HTTP/1.1\r\nUpgrade: h2c\r\n\r\n',
        // A blank request line is a request for nothing.
        '\r\n\r\n',
      ]) {
        const [head, body] = await wire.exchange(request);
        assert.ok(head.startsWith('HTTP/1.1 404'), `${request}: ${head}`);
        assert.equal(body, '{"error":"resource does not exist"}');
      }
    }));

  it('answers with the headers Rust writes', () =>
    withServer(null, async ({ connect }) => {
      const wire = await connect();
      assert.deepEqual(await wire.exchange('GET /healthz HTTP/1.1\r\n\r\n'), [
        'HTTP/1.1 200 OK\r\nContent-Type: application/json; charset=utf-8\r\n' +
          'Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\n' +
          'Access-Control-Allow-Headers: authorization, content-type, x-hasura-admin-secret, ' +
          'x-hasura-client, x-hasura-role, x-hasura-user-id\r\nContent-Length: 15\r\n',
        '{"status":"ok"}',
      ]);
    }));

  it('a preflight echoes what it is asked about', () =>
    withServer(null, async ({ connect }) => {
      const wire = await connect();
      const [head] = await wire.exchange(
        'OPTIONS /v1/graphql HTTP/1.1\r\nAccess-Control-Request-Headers: x-mine\r\n\r\n',
      );
      assert.ok(head.startsWith('HTTP/1.1 204'), head);
      assert.ok(head.includes('Allow-Headers: x-mine'), head);
      const [other] = await wire.exchange('OPTIONS /other HTTP/1.1\r\n\r\n');
      assert.ok(other.includes('x-hasura-admin-secret'), other);
    }));

  it('requests choose operations and variables', () =>
    withServer(null, async ({ connect }) => {
      const body =
        '{"operationName":"Second","variables":{"id":1},"query":"query First { links(limit: 0) { id } } query Second($id: bigint) { links(where: {id: {_eq: $id}}) { id } }"}';
      assert.equal(await postText(connect, body), '{"data":{"links":[{"id":1}]}}');
      // Fields of the wrong kind count as absent, and a missing query is refused.
      assert.equal(
        await postText(connect, '{"query":1,"operationName":2}'),
        '{"errors":[{"message":"A document has at least one operation.","extensions":{"path":"$.query","code":"validation-failed"}}]}',
      );
    }));

  it('bad requests get an error and keep the server', () =>
    withServer('root', async ({ connect }) => {
      assert.equal(
        await postText(connect, '{not json'),
        '{"errors":[{"message":"Invalid JSON object at byte 1.","extensions":{"path":"$","code":"validation-failed"}}]}',
      );
      const guest = 'query { guest { token } }';
      const forged = await post(connect, guest, 'Authorization: Bearer a.b.c\r\n');
      assert.equal(forged.errors[0].extensions.code, 'invalid-jwt');
      assert.ok(
        Object.hasOwn(await post(connect, guest, 'x-hasura-admin-secret: wrong\r\n'), 'errors'),
      );
      assert.ok(
        Object.hasOwn(await post(connect, guest, 'x-hasura-admin-secret: root\r\n'), 'data'),
      );

      // A length that is not a number means no body.
      const many = 'POST /v1/graphql HTTP/1.1\r\nContent-Length: many\r\n\r\n';
      assert.ok((await (await connect()).exchange(many))[1].includes('errors'));

      const huge = `POST /v1/graphql HTTP/1.1\r\nContent-Length: ${2n ** 63n - 1n}\r\n\r\n`;
      const [head, body] = await (await connect()).exchange(huge);
      assert.ok(head.startsWith('HTTP/1.1 413'), head);
      assert.equal(body, '{}');

      // Clients that vanish at each step of a request, or send what is not UTF-8.
      for (const partial of [
        '',
        'GET /healthz HTTP/1.1',
        'GET /healthz HTTP/1.1\r\n',
        'POST /v1/graphql HTTP/1.1\r\nContent-Length: 5\r\n\r\nab',
        Buffer.from([0xff, 0x0a]),
      ]) {
        const wire = await connect();
        wire.write(partial);
        wire.finish();
        assert.equal((await wire.rest()).length, 0, String(partial));
      }
      assert.ok(Object.hasOwn(await post(connect, INSERT), 'data'));
    }));
});

describe('WebSocket', LIMITS, () => {
  it('every GraphQL path upgrades and the protocol is negotiated', () =>
    withServer(null, async ({ connect }) => {
      for (const path of ['/v1/graphql', '/gql', '/v1alpha1/graphql']) {
        const head = await (await connect()).upgrade(path);
        assert.equal(
          head,
          'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n' +
            'Sec-WebSocket-Accept: s3pPLMBiTxaQ9kYGzzhZRbK+xOo=\r\nSec-WebSocket-Protocol: graphql-ws\r\n',
        );
      }
      const lines = 'Sec-WebSocket-Protocol: other, graphql-transport-ws\r\n';
      const head = await (await connect()).upgrade('/gql', lines);
      assert.ok(head.includes('Protocol: graphql-transport-ws\r\n'), head);
      // A missing key still gets an answer.
      const [answer] = await (
        await connect()
      ).exchange('GET /gql HTTP/1.1\r\nUpgrade: websocket\r\n\r\n');
      assert.ok(answer.startsWith('HTTP/1.1 101'), answer);
    }));

  it('frames join and control frames interleave', () =>
    withServer(null, async ({ socket }) => {
      const wire = await socket();
      const ping = Buffer.from('{"type":"ping"}');
      // Split over a fragment, with a ping and a pong between the parts.
      wire.write(clientFrame(0x01, ping.subarray(0, 5)));
      wire.write(clientFrame(0x89, 'hello'));
      assert.deepEqual(await wire.raw(), [10, Buffer.from('hello')]);
      wire.write(clientFrame(0x8a, 'late'));
      wire.write(clientFrame(0x80, ping.subarray(5)));
      assert.equal(await wire.kind(), 'pong');

      // Without a mask, and with each length encoding.
      wire.write(clientFrame(0x81, ping, false));
      assert.equal(await wire.kind(), 'pong');
      for (const size of [200, 70_000]) {
        wire.send(`{"type":"ping"${' '.repeat(size)}}`);
        assert.equal(await wire.kind(), 'pong', `${size}`);
      }

      // What cannot be understood is passed over.
      for (const text of ['not json', '{}', '{"type":"unknown"}', '{"type":1}']) wire.send(text);
      wire.send('{"type":"ping"}');
      assert.deepEqual(await wire.raw(), [1, Buffer.from('{"type":"pong"}')]);

      // A close is echoed, and ends the connection.
      wire.write(clientFrame(0x88, 'bye'));
      assert.deepEqual(await wire.raw(), [8, Buffer.from('bye')]);
      await wire.assertClosed();
    }));

  it('a frame over the limit or cut short ends the connection', () =>
    withServer(null, async ({ socket }) => {
      const oversized = await socket();
      oversized.write(Buffer.from([0x81, 0x7f, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]));
      await oversized.assertClosed();

      const short = await socket();
      short.write(Buffer.from([0x81]));
      short.finish();
      await short.assertClosed();
    }));

  it('connection_init carries credentials', () =>
    withServer('root', async ({ connect, socket }) => {
      const answer = await post(connect, 'query { guest { token } }');
      const token = answer.data.guest.token;
      for (const payload of [
        `{"Authorization":"Bearer ${token}"}`,
        `{"authorization":"Bearer ${token}"}`,
        `{"headers":{"Authorization":"Bearer ${token}"}}`,
        '{"headers":{"x-hasura-admin-secret":"root"}}',
        '{"Authorization":1}',
        '{}',
      ]) {
        const wire = await socket();
        wire.send(`{"type":"connection_init","payload":${payload}}`);
        assert.deepEqual(await wire.message(), { type: 'connection_ack' }, payload);
      }

      // The legacy protocol also starts keeping the connection alive.
      const legacy = await socket({ legacy: true });
      await legacy.init();
      assert.equal(await legacy.kind(), 'ka');

      const refused = await socket();
      refused.send('{"type":"connection_init","payload":{"Authorization":"Bearer a.b.c"}}');
      assert.deepEqual(await refused.message(), {
        type: 'connection_error',
        payload: { message: 'Could not verify JWT: Invalid JSON at byte 0.' },
      });
      await refused.assertClosed();

      // A bad token on the upgrade is only a guest until something better arrives.
      const guest = await connect();
      await guest.upgrade(
        '/gql',
        'Authorization: Bearer a.b.c\r\nSec-WebSocket-Protocol: graphql-transport-ws\r\n',
      );
      await guest.init();
      const terminated = await socket();
      terminated.send('{"type":"connection_terminate"}');
      await terminated.assertClosed();
    }));

  it('a connection keeps its subject through a connection_init without credentials', () =>
    withServer('root', async ({ connect, socket }) => {
      const admin = await socket();
      admin.send('{"type":"connection_init","payload":{"x-hasura-admin-secret":"root"}}');
      assert.equal(await admin.kind(), 'connection_ack');
      // A later `connection_init` without credentials keeps the admin.
      admin.send('{"type":"connection_init"}');
      assert.equal(await admin.kind(), 'connection_ack');
      admin.subscribe('subscribe', 'all', '{ links(where: {id: {_eq: 1}}) { id } }');
      assert.deepEqual(await admin.message(), {
        type: 'next',
        id: 'all',
        payload: { data: { links: [{ id: 1 }] } },
      });
      const upgraded = await connect();
      await upgraded.upgrade('/gql', 'x-hasura-admin-secret: root\r\n');
      upgraded.send('{"type":"start","id":"q","payload":{"query":"{ links(limit: 1) { id } }"}}');
      assert.deepEqual(await upgraded.message(), {
        type: 'data',
        id: 'q',
        payload: { data: { links: [{ id: 1 }] } },
      });
    }));

  it('subscriptions report errors in the form of their protocol', () =>
    withServer(null, async ({ socket }) => {
      const modern = await socket();
      await modern.init();
      modern.subscribe('subscribe', '1', 'subscription { nothing_here { id } }');
      const refused = await modern.message();
      assert.equal(refused.type, 'error');
      assert.equal(refused.id, '1');
      assert.ok(refused.payload.length > 0);

      const legacy = await socket({ legacy: true });
      await legacy.init();
      assert.equal(await legacy.kind(), 'ka');
      legacy.subscribe('start', '2', 'subscription { nothing_here { id } }');
      const data = await legacy.message();
      assert.equal(data.type, 'data');
      assert.ok(Object.hasOwn(data.payload, 'errors'));

      // An id is optional; the reply then names an empty one.
      modern.send(JSON.stringify({ type: 'subscribe', payload: { query: WATCH } }));
      assert.equal((await modern.message()).id, '');
      // So is a payload: no query is a query that is refused.
      modern.send('{"type":"subscribe","id":"bare"}');
      assert.equal((await modern.message()).type, 'error');
    }));

  it('stopping ends only the named subscription', () =>
    withServer(null, async ({ connect, socket }) => {
      const wire = await socket();
      await wire.init();
      wire.subscribe('subscribe', 'kept', WATCH);
      assert.equal(await wire.kind(), 'next');
      wire.subscribe('subscribe', 'ended', WATCH);
      assert.equal(await wire.kind(), 'next');
      wire.send('{"type":"complete","id":"ended"}');
      // Answered after the stop, so the stop has been made.
      wire.send('{"type":"ping"}');
      assert.equal(await wire.kind(), 'pong');

      await post(connect, INSERT);
      assert.equal((await wire.message()).id, 'kept');
      wire.send('{"type":"ping"}');
      assert.equal(await wire.kind(), 'pong', 'the ended one stays silent');

      // Without an id every subscription of the connection ends.
      const watcher = await socket();
      await watcher.init();
      watcher.subscribe('subscribe', 'a', WATCH);
      assert.equal(await watcher.kind(), 'next');
      watcher.send('{"type":"complete"}');
      watcher.send('{"type":"ping"}');
      assert.equal(await watcher.kind(), 'pong');
      await post(connect, INSERT);
      assert.equal((await wire.message()).id, 'kept');
      watcher.send('{"type":"ping"}');
      assert.equal(await watcher.kind(), 'pong');

      // The legacy protocol confirms a stop, named or not.
      const legacy = await socket({ legacy: true });
      await legacy.init();
      assert.equal(await legacy.kind(), 'ka');
      legacy.subscribe('start', 'old', WATCH);
      assert.equal(await legacy.kind(), 'data');
      legacy.send('{"type":"stop","id":"old"}');
      assert.deepEqual(await legacy.message(), { type: 'complete', id: 'old' });
      legacy.send('{"type":"stop"}');
      assert.deepEqual(await legacy.message(), { type: 'complete' });
    }));

  it('a write reaches only subscriptions whose answer changed', () =>
    withServer(null, async ({ socket }) => {
      const watcher = await socket();
      await watcher.init();
      watcher.subscribe(
        'subscribe',
        'same',
        'subscription { links(where: {id: {_eq: 1}}) { id } }',
      );
      assert.equal(await watcher.kind(), 'next');
      watcher.subscribe('subscribe', 'moves', WATCH);
      assert.equal(await watcher.kind(), 'next');

      // A mutation run through a subscription start counts as a write too.
      const writer = await socket();
      await writer.init();
      writer.subscribe('subscribe', 'write', INSERT);
      assert.equal(await writer.kind(), 'next');

      assert.equal((await watcher.message()).id, 'moves');
      watcher.send('{"type":"ping"}');
      assert.equal(await watcher.kind(), 'pong');
    }));

  it('long answers use the longer frame lengths', () =>
    withServer(null, async ({ connect, socket }) => {
      const wire = await socket();
      await wire.init();
      for (const [size, id] of [
        [200, 'medium'],
        [70_000, 'long'],
      ]) {
        const text = 'x'.repeat(size);
        const answer = await post(
          connect,
          `mutation { insert_links(objects: {type_id: 1, string: {data: {value: "${text}"}}}) { returning { id } } }`,
        );
        const row = answer.data.insert_links.returning[0];
        wire.subscribe(
          'subscribe',
          id,
          `subscription { links(where: {id: {_eq: ${row.id}}}) { value } }`,
        );
        const reply = await wire.message();
        assert.equal(reply.id, id);
        assert.equal(reply.payload.data.links[0].value.value, text);
      }
    }));

  it('a subscriber that no longer takes writes is dropped', async () => {
    const { store } = deepStore({ core: true });
    const listener = createServer();
    const accepted = [];
    listener.on('connection', (socket) => accepted.push(socket));
    await new Promise((resolve) => listener.listen({ host: '127.0.0.1', port: 0 }, resolve));
    const server = new GraphqlServer(listener);
    const served = server.serve(store, new Settings(Buffer.from('s3'), null));
    const address = server.localAddr();
    // Keeps its side open when the server ends its own.
    const watcher = await Wire.connect(address, { allowHalfOpen: true });
    const writer = await Wire.connect(address);
    try {
      await watcher.upgrade('/v1/graphql', 'Sec-WebSocket-Protocol: graphql-transport-ws\r\n');
      await watcher.init();
      watcher.subscribe('subscribe', 'w', WATCH);
      assert.equal(await watcher.kind(), 'next');
      accepted[0].end();
      assert.ok(
        Object.hasOwn(JSON.parse(await writer.post(JSON.stringify({ query: INSERT }))), 'data'),
      );
      assert.equal((await watcher.rest()).length, 0);
      // The server goes on: the next write finds no subscriber left.
      assert.ok(
        Object.hasOwn(JSON.parse(await writer.post(JSON.stringify({ query: INSERT }))), 'data'),
      );
    } finally {
      server.shutdown();
      await served;
      watcher.destroy();
      writer.destroy();
    }
  });
});

describe('the life of a server', LIMITS, () => {
  it('a connection its peer resets ends without stopping the server', async () => {
    const listener = createServer();
    const accepted = new Promise((resolve) => listener.once('connection', resolve));
    await new Promise((resolve) => listener.listen({ host: '127.0.0.1', port: 0 }, resolve));
    const server = new GraphqlServer(listener);
    const served = server.serve(deepStore().store, new Settings(Buffer.from('s3'), null));
    const reset = await Wire.connect(server.localAddr());
    const next = await Wire.connect(server.localAddr());
    try {
      const socket = await accepted;
      const closed = new Promise((resolve) => socket.once('close', resolve));
      reset.write('GET /healthz HTTP/1.1\r\n');
      reset.socket.resetAndDestroy();
      assert.equal(await closed, true, 'closed by an error');
      assert.equal((await next.exchange('GET /healthz HTTP/1.1\r\n\r\n'))[1], '{"status":"ok"}');
    } finally {
      server.shutdown();
      await served;
      next.destroy();
    }
  });

  it('connections made before it serves wait for it', async () => {
    const listener = createServer();
    const accepted = new Promise((resolve) => listener.once('connection', resolve));
    await new Promise((resolve) => listener.listen({ host: '127.0.0.1', port: 0 }, resolve));
    const server = new GraphqlServer(listener);
    const wire = await Wire.connect(server.localAddr());
    try {
      wire.write('GET /healthz HTTP/1.1\r\n\r\n');
      await accepted;
      const { store } = deepStore();
      const served = server.serve(store, new Settings(Buffer.from('s3'), null));
      assert.equal((await wire.exchange(''))[1], '{"status":"ok"}');
      server.shutdown();
      server.shutdown();
      await served;
      await wire.rest();
    } finally {
      wire.destroy();
    }
  });

  it('listens on IPv6 and prints the address as Rust does', async () => {
    const server = await GraphqlServer.bind('[::1]:0');
    const served = server.serve(deepStore().store, new Settings(Buffer.from('s3'), null));
    const wire = await Wire.connect(server.localAddr());
    try {
      assert.match(server.localAddr(), /^\[::1\]:\d+$/);
      assert.equal((await wire.exchange('GET /healthz HTTP/1.1\r\n\r\n'))[1], '{"status":"ok"}');
    } finally {
      server.shutdown();
      await served;
      wire.destroy();
    }
  });
});

describe('binding', LIMITS, () => {
  const lookupOf =
    (error, addresses = []) =>
    (host, options, callback) => {
      assert.deepEqual(options, { all: true, verbatim: true });
      callback(
        error,
        addresses.map((address) => ({ address })),
      );
    };

  it('reads the address as ToSocketAddrs does', async () => {
    // Recorded from `clink --serve ADDRESS --graphql` of the Rust clink.
    for (const [address, text] of [
      ['nocolon', 'invalid socket address'],
      ['h:99999', 'invalid port value'],
      ['h:-1', 'invalid port value'],
      ['[::1]:99999', 'invalid port value'],
      [':80', 'failed to lookup address information: No address associated with hostname'],
    ]) {
      await assert.rejects(socketAddresses(address), { message: text }, address);
    }
    assert.deepEqual(await socketAddresses('[::1]:80'), [['::1', 80]]);
    assert.deepEqual(await socketAddresses('::1:80'), [['::1', 80]]);
    assert.deepEqual(await socketAddresses('127.0.0.1:+80'), [['127.0.0.1', 80]]);
    const asked = [];
    const names = (host, options, callback) => {
      asked.push(host);
      lookupOf(null, ['10.0.0.1', '::2'])(host, options, callback);
    };
    assert.deepEqual(await socketAddresses('[zz]:7', names), [
      ['10.0.0.1', 7],
      ['::2', 7],
    ]);
    assert.deepEqual(asked, ['[zz]']);
    const local = await lookup('localhost', { all: true, verbatim: true });
    assert.deepEqual(
      await socketAddresses('localhost:7'),
      local.map((entry) => [entry.address, 7]),
    );
  });

  it('reports a failed lookup in the words of getaddrinfo', async () => {
    const failure = (code, errno, message = 'unused') =>
      Object.assign(new Error(message), { code, errno });
    for (const [error, text] of [
      [failure('ENOTFOUND', -3007), 'No address associated with hostname'],
      [failure('ENOTFOUND', -3008), 'Name or service not known'],
      [failure('EAI_AGAIN', -3001), 'Temporary failure in name resolution'],
      [failure('EAI_FAIL', -3004), 'Non-recoverable failure in name resolution'],
      [failure('EAI_MEMORY', -3006), 'Memory allocation failure'],
      [failure('EAI_FAMILY', -3003), 'ai_family not supported'],
      [failure('EAI_SERVICE', -3010), 'Servname not supported for ai_socktype'],
      [failure('EOTHER', -1, 'something else'), 'something else'],
    ]) {
      await assert.rejects(GraphqlServer.bind('name:1', { lookup: lookupOf(error) }), {
        name: 'IoError',
        kind: 'Uncategorized',
        message: `failed to lookup address information: ${text}`,
      });
    }
  });

  it('tries each address in turn and reports the last failure', async () => {
    const taken = createServer();
    await new Promise((resolve) => taken.listen({ host: '127.0.0.1', port: 0 }, resolve));
    const { port } = taken.address();
    try {
      await assert.rejects(GraphqlServer.bind('name:1', { lookup: lookupOf(null, []) }), {
        name: 'IoError',
        kind: 'InvalidInput',
        message: 'could not resolve to any addresses',
      });
      const busy = { lookup: lookupOf(null, ['127.0.0.1']) };
      await assert.rejects(GraphqlServer.bind(`name:${port}`, busy), { code: 'EADDRINUSE' });
      await assert.rejects(GraphqlServer.bind(`127.0.0.1:${port}`), { code: 'EADDRINUSE' });
      const server = await GraphqlServer.bind(`name:${port}`, {
        lookup: lookupOf(null, ['127.0.0.1', '::1']),
      });
      assert.equal(server.localAddr(), `[::1]:${port}`);
      server.shutdown();
    } finally {
      taken.close();
    }
  });
});

describe('frames and messages', () => {
  it('are written as Rust writes them', () => {
    assert.equal(message('ka', null, null), '{"type":"ka"}');
    assert.equal(message('next', 'a"b', '{}'), '{"type":"next","id":"a\\"b","payload":{}}');
    assert.deepEqual([...frame(9, Buffer.from('hi'))], [0x89, 2, 0x68, 0x69]);
    assert.deepEqual([...frame(1, 'x'.repeat(126)).subarray(0, 4)], [0x81, 126, 0, 126]);
    assert.deepEqual(
      [...frame(1, 'x'.repeat(65536)).subarray(0, 10)],
      [0x81, 127, 0, 0, 0, 0, 0, 1, 0, 0],
    );
    assert.equal(frame(1, 'x'.repeat(65536)).length, 65546);
    const written = [];
    assert.equal(
      sendFrame({ writable: true, write: (bytes) => written.push(bytes) }, 1, 'a'),
      true,
    );
    assert.deepEqual(written, [Buffer.from([0x81, 1, 0x61])]);
    assert.equal(sendFrame({ writable: false }, 1, 'a'), false);
  });
});
