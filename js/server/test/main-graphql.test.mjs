// `clink --serve ADDRESS --graphql`: `run_graphql_server` of rust/src/main.rs,
// the cases of rust/tests/graphql_server.rs that need the whole command (its
// banner, Deep packages, permissions, a stop signal and a restart) and the
// refusals Rust prints for what it cannot serve.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

import { main } from '../src/main.js';
import { runGraphqlServer } from '../src/main-graphql.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { PermissionPolicy } from '../src/permissions.js';
import { fixturePath, writeRules } from './helpers/graphql.mjs';
import { Wire, openSocket } from './helpers/graphql-wire.mjs';
import { capture, scratchDirectory } from './helpers/storage.mjs';

const CORE = fileURLToPath(fixturePath('deep-core.json'));
const LIMITS = { timeout: 60_000 };

/** The fields of `Cli` that `--graphql` reads. */
const cliOf = (fields = {}) => ({
  serveAlso: [],
  deepPackages: [CORE],
  jwtSecret: 's3',
  graphqlAdminSecret: null,
  ...fields,
});

const open = (path) => NamedTypesDecorator.open(path).withIndexedLookups();

/** A stdout that resolves `banner` with the first line written to it. */
function bannerOutput() {
  const output = capture();
  output.banner = new Promise((resolve) => {
    output.write = (chunk) => {
      output.text += chunk;
      resolve(output.text);
    };
  });
  return output;
}

/**
 * Starts the server over the database at `path`; `{ address, stop, done,
 * stdout }`, where `stop` sends `signal` and `done` settles once the server
 * has ended and saved.
 */
async function start(path, { cli = cliOf(), policy = null, signal = 'SIGTERM', lookup } = {}) {
  const storage = open(path);
  const signals = new EventEmitter();
  const stdout = bannerOutput();
  const done = runGraphqlServer(
    cli,
    '127.0.0.1:0',
    storage,
    policy === null ? null : policy(storage),
    { stdout, signals, lookup },
  );
  const banner = await stdout.banner;
  const address = /^clink GraphQL server listening on (\S+)\n$/.exec(banner)[1];
  return { address, done, stdout, stop: () => signals.emit(signal), signals };
}

async function post(address, query, headers = '') {
  const wire = await Wire.connect(address);
  try {
    return JSON.parse(await wire.post(JSON.stringify({ query }), headers));
  } finally {
    wire.destroy();
  }
}

const ids = (answer, table) => answer.data[table].map((row) => row.id);
const bearer = (token) => `Authorization: Bearer ${token}\r\n`;
const INSERT = 'mutation { insert_links(objects: {type_id: 1}) { returning { id } } }';
const inserted = (answer) => answer.data.insert_links.returning[0].id;

describe('what --graphql refuses', () => {
  it('more than one address, an endpoint that is not TCP and packages that do not read', async () => {
    const path = join(scratchDirectory(), 'db.links');
    const refused = (cli, address, message) =>
      assert.rejects(runGraphqlServer(cli, address, open(path), null), message);
    await refused(cliOf({ serveAlso: ['127.0.0.1:0'] }), '127.0.0.1:0', {
      message: '--graphql serves HTTP on one TCP address; give --serve once.',
    });
    await refused(cliOf(), 'unix:/tmp/clink.sock', {
      message: '--graphql serves HTTP over TCP only, not over `unix:`.',
    });
    const directory = scratchDirectory();
    const missing = join(directory, 'missing.json');
    await refused(cliOf({ deepPackages: [missing] }), '127.0.0.1:0', { code: 'ENOENT' });
    const binary = join(directory, 'binary.json');
    writeFileSync(binary, Buffer.from([0x5b, 0xff, 0x5d]));
    await refused(cliOf({ deepPackages: [binary] }), '127.0.0.1:0', {
      message: 'stream did not contain valid UTF-8',
    });
    const broken = join(directory, 'broken.json');
    writeFileSync(broken, '[{"id":1,}]');
    await refused(cliOf({ deepPackages: [broken] }), '127.0.0.1:0', {
      message: `${broken}: Invalid JSON object at byte 9.`,
    });
    await refused(cliOf({ deepPackages: [] }), 'nocolon', { message: 'invalid socket address' });
  });

  it('prints the refusal as the Rust clink does', async () => {
    const path = join(scratchDirectory(), 'db.links');
    const [stdout, stderr] = [capture(), capture()];
    const args = ['--db', path, '--serve', '127.0.0.1:0', '--serve', '127.0.0.1:0', '--graphql'];
    assert.equal(await main(args, { stdout, stderr }), 1);
    assert.equal(stdout.text, '');
    assert.equal(
      stderr.text,
      'Error: --graphql serves HTTP on one TCP address; give --serve once.\n',
    );
  });
});

describe('a --graphql server', LIMITS, () => {
  it('prints its address, stops on a signal and serves its links again after a restart', async () => {
    const path = join(scratchDirectory(), 'db.links');
    const first = await start(path, { signal: 'SIGHUP' });
    const answer = await post(
      first.address,
      'mutation { insert_links(objects: {type_id: 1, string: {data: {value: "kept"}}}) { returning { id value { id value } } } }',
    );
    const kept = answer.data.insert_links.returning[0];
    assert.equal(kept.value.value, 'kept');
    const id = kept.id;
    const guest = (await post(first.address, '{ guest { linkId } }')).data.guest.linkId;
    first.stop();
    await first.done;
    assert.equal(first.signals.listenerCount('SIGHUP'), 0);

    const again = await start(path, { signal: 'SIGINT' });
    try {
      const found = await post(
        again.address,
        `query { links(where: {id: {_in: [${id}, ${guest}]}}, order_by: {id: asc}) { id value { id value } } }`,
      );
      // The value is a row of its own, as Rust answers it.
      assert.deepEqual(found.data.links, [kept, { id: guest, value: null }]);
    } finally {
      again.stop();
      await again.done;
    }
  });

  it('the token is the subject under permissions', async () => {
    const path = join(scratchDirectory(), 'db.links');
    // Whoever it is may create: creating needs some right to write.
    writeRules(path, '(AllowWrite (Anything Doc))');
    const server = await start(path, {
      cli: cliOf({ graphqlAdminSecret: 'root' }),
      policy: (storage) => PermissionPolicy.inData(storage),
    });
    try {
      const all = 'query { links(where: {id: {_lte: 2}}) { id } }';
      assert.deepEqual(ids(await post(server.address, all), 'links'), []);
      const admin = await post(server.address, all, 'x-hasura-admin-secret: root\r\n');
      assert.equal(ids(admin, 'links').length, 2);
      const wrong = await post(server.address, all, 'x-hasura-admin-secret: wrong\r\n');
      assert.ok(Object.hasOwn(wrong, 'errors'));

      // What a subject created is its own: visible to it, not to another.
      const token = (await post(server.address, '{ guest { token } }')).data.guest.token;
      const other = (await post(server.address, '{ guest { token } }')).data.guest.token;
      const own = inserted(
        await post(
          server.address,
          'mutation { insert_links(objects: {id: 9000, type_id: 9000}) { returning { id } } }',
          bearer(token),
        ),
      );
      const byId = `query { links(where: {id: {_eq: ${own}}}) { id } }`;
      assert.deepEqual(ids(await post(server.address, byId, bearer(token)), 'links'), [own]);
      assert.deepEqual(ids(await post(server.address, byId, bearer(other)), 'links'), []);
      assert.deepEqual(ids(await post(server.address, byId), 'links'), []);

      // The same over a WebSocket, whose token comes in `connection_init`.
      const socket = await openSocket(server.address, { legacy: true });
      try {
        socket.send(
          `{"type":"connection_init","payload":{"headers":{"Authorization":"Bearer ${token}"}}}`,
        );
        assert.equal(await socket.kind(), 'connection_ack');
        assert.equal(await socket.kind(), 'ka');
        socket.subscribe('start', '1', byId);
        const data = await socket.message();
        assert.equal(data.type, 'data');
        assert.deepEqual(ids(data.payload, 'links'), [own]);
      } finally {
        socket.destroy();
      }

      // A forged token is refused.
      assert.ok(Object.hasOwn(await post(server.address, all, bearer('a.b.c')), 'errors'));
    } finally {
      server.stop();
      await server.done;
    }
  });

  it('can shows the rules of the policy', async () => {
    const path = join(scratchDirectory(), 'db.links');
    writeRules(path, '(AllowRead (Anything Anything))');
    const server = await start(path, { policy: (storage) => PermissionPolicy.inData(storage) });
    try {
      assert.deepEqual(await post(server.address, 'query { can { subject_id object_id } }'), {
        data: { can: [{ subject_id: 0, object_id: 0 }] },
      });
      const read = await post(server.address, 'query { links(where: {id: {_eq: 1}}) { id } }');
      assert.deepEqual(ids(read, 'links'), [1]);
    } finally {
      server.stop();
      await server.done;
    }
  });

  it('subscriptions of both protocols see a write', async () => {
    const path = join(scratchDirectory(), 'db.links');
    const server = await start(path);
    const WATCH =
      'subscription { links(where: {type_id: {_eq: 1}}, order_by: {id: desc}, limit: 1) { id } }';
    try {
      for (const [legacy, start, data] of [
        [true, 'start', 'data'],
        [false, 'subscribe', 'next'],
      ]) {
        const socket = await openSocket(server.address, { legacy });
        try {
          socket.send('{"type":"connection_init","payload":{}}');
          assert.equal(await socket.kind(), 'connection_ack');
          if (legacy) assert.equal(await socket.kind(), 'ka');
          socket.subscribe(start, '1', WATCH);
          const first = await socket.message();
          assert.equal(first.type, data);
          const id = inserted(await post(server.address, INSERT));
          const update = await socket.message();
          assert.equal(update.type, data);
          assert.notDeepEqual(update, first);
          assert.deepEqual(ids(update.payload, 'links'), [id]);
        } finally {
          socket.destroy();
        }
      }
    } finally {
      server.stop();
      await server.done;
    }
  });

  it('resolves a name, and by default makes its own secret and listens to the process', async () => {
    const path = join(scratchDirectory(), 'db.links');
    const storage = open(path);
    const before = process.listenerCount('SIGTERM');
    const write = process.stdout.write;
    let banner;
    // The test runner reports through the same stdout: only the banner is kept.
    const printed = new Promise((resolve) => {
      process.stdout.write = (chunk, ...rest) =>
        String(chunk).startsWith('clink GraphQL server')
          ? resolve(String(chunk))
          : write.call(process.stdout, chunk, ...rest);
    });
    // Without `io`, the server answers to the process and writes to its stdout.
    const done = runGraphqlServer(
      cliOf({ jwtSecret: null, deepPackages: [] }),
      '127.0.0.1:0',
      storage,
      null,
    );
    try {
      banner = await printed;
    } finally {
      process.stdout.write = write;
    }
    try {
      assert.equal(process.listenerCount('SIGTERM'), before + 1);
      const address = /^clink GraphQL server listening on (127\.0\.0\.1:\d+)\n$/.exec(banner)[1];
      // A token of the secret `s3` is not one of this server's.
      const token = (await post(address, '{ guest { token } }')).data.guest.token;
      const known = await start(join(scratchDirectory(), 'db.links'), {
        cli: cliOf({ deepPackages: [] }),
      });
      try {
        const answer = await post(known.address, '{ links { id } }', bearer(token));
        assert.equal(answer.errors[0].extensions.code, 'invalid-jwt');
      } finally {
        known.stop();
        await known.done;
      }
      // A name is looked up with `io.lookup`.
      const named = await runNamed();
      assert.match(named, /^clink GraphQL server listening on 127\.0\.0\.1:\d+\n$/);
    } finally {
      process.emit('SIGTERM');
      await done;
    }
    assert.equal(process.listenerCount('SIGTERM'), before);
  });
});

/** Serves on `deep:0`, a name its lookup gives as 127.0.0.1; the banner. */
async function runNamed() {
  const storage = open(join(scratchDirectory(), 'db.links'));
  const signals = new EventEmitter();
  const stdout = bannerOutput();
  const lookup = (host, options, callback) => {
    assert.equal(host, 'deep');
    callback(null, [{ address: '127.0.0.1', family: 4 }]);
  };
  const done = runGraphqlServer(cliOf({ deepPackages: [] }), 'deep:0', storage, null, {
    stdout,
    signals,
    lookup,
  });
  const banner = await stdout.banner;
  signals.emit('SIGINT');
  await done;
  return banner;
}
