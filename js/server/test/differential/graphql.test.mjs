// The JavaScript clink against the Rust clink as GraphQL servers
// (`--serve ADDRESS --graphql`): what each refuses to serve, compared with
// the harness, and every recorded file of docs/protocol/graphql replayed over
// HTTP and WebSocket against both servers at once. Every response is compared
// byte for byte (head and body), then both servers are stopped with SIGTERM
// and their output, exit status and the files they saved are compared too.
// Skipped without CLINK_RUST or without bin/clink.js.
import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { describe, it } from 'node:test';
import { gunzipSync } from 'node:zlib';

import { Json } from '../../src/graphql-json.js';
import { fixture, fixturePath } from '../helpers/graphql.mjs';
import { Wire, clientFrame } from '../helpers/graphql-wire.mjs';
import { differential, JS, RUST, skip as noRust } from './harness.mjs';

const skip = noRust || (existsSync(JS) ? false : `${JS} is missing`);
const LIMITS = { skip, timeout: 300_000 };

const ADMIN = 'x-hasura-admin-secret: adm\r\n';
const SERVE = ['--db', 'db.links', '--serve', '127.0.0.1:0', '--graphql'];
const SECRETS = ['--jwt-secret', 'secret', '--graphql-admin-secret', 'adm'];

/** One clink serving in `cwd`: its address and a `stop` that ends it with SIGTERM. */
async function serve(command, args, cwd) {
  const child = spawn(command, args, {
    cwd,
    env: { ...process.env, RUST_BACKTRACE: '0', NO_COLOR: '1' },
  });
  let [stdout, stderr] = ['', ''];
  child.stderr.on('data', (chunk) => (stderr += chunk));
  const closed = new Promise((resolve) =>
    child.on('close', (status, signal) => resolve({ status, signal })),
  );
  const address = await new Promise((resolve, reject) => {
    child.stdout.on('data', (chunk) => {
      stdout += chunk;
      const banner = /^clink GraphQL server listening on (\S+)\n/.exec(stdout);
      if (banner !== null) resolve(banner[1]);
    });
    closed.then(() => reject(new Error(`${command} ended: ${stderr}`)));
  });
  const stop = async () => {
    child.kill('SIGTERM');
    const ended = await closed;
    return { ...ended, stdout: stdout.replace(address, '<address>'), stderr };
  };
  return { address, stop };
}

/** Every file under `root`, by name. */
function files(root) {
  return Object.fromEntries(
    readdirSync(root).map((name) => [name, readFileSync(join(root, name))]),
  );
}

/** Two sandboxes, one for each clink, holding `contents` by name. */
function sandboxes(contents = {}) {
  const both = [
    mkdtempSync(join(tmpdir(), 'clink-rust-')),
    mkdtempSync(join(tmpdir(), 'clink-js-')),
  ];
  for (const sandbox of both) {
    for (const [name, content] of Object.entries(contents))
      writeFileSync(join(sandbox, name), content);
  }
  return both;
}

/**
 * Runs `body` with both servers started by `args` in `both` sandboxes; then
 * stops them and compares how they ended and what they saved.
 */
async function withServers(both, args, body) {
  const [rust, js] = await Promise.all([
    serve(RUST, args, both[0]),
    serve(process.execPath, [JS, ...args], both[1]),
  ]);
  let ended;
  try {
    await body(new Pair(rust.address, js.address));
  } finally {
    ended = await Promise.all([rust.stop(), js.stop()]);
  }
  assert.deepEqual(ended[1], ended[0], 'how the servers ended');
  const [rustFiles, jsFiles] = both.map(files);
  assert.deepEqual(Object.keys(jsFiles).sort(), Object.keys(rustFiles).sort(), 'saved files');
  for (const [name, bytes] of Object.entries(rustFiles)) {
    assert.ok(jsFiles[name].equals(bytes), `the bytes of ${name}`);
  }
}

const remove = (both) => both.forEach((path) => rmSync(path, { recursive: true, force: true }));

/** The Rust server and the JavaScript server, asked the same and answering the same. */
class Pair {
  constructor(rust, js) {
    this.addresses = [rust, js];
  }

  /** Sends `request` to both; asserts one response `[head, body]` and returns it. */
  async exchange(request, what = request) {
    const answers = [];
    for (const address of this.addresses) {
      const wire = await Wire.connect(address);
      try {
        answers.push(await wire.exchange(request));
      } finally {
        wire.destroy();
      }
    }
    // A token signs the second it was made in (`iat`): the rest must match.
    const masked = answers.map(([head, body]) => [
      head,
      body.replace(/"token":"[^"]*"/g, '"token":"<jwt>"'),
    ]);
    assert.deepEqual(masked[1], masked[0], what);
    return answers[1];
  }

  /** POSTs `body` to /v1/graphql of both; the body of the response, parsed. */
  async post(body, headers = '', what = body) {
    const length = Buffer.byteLength(body);
    const request = `POST /v1/graphql HTTP/1.1\r\nHost: x\r\n${headers}Content-Length: ${length}\r\n\r\n${body}`;
    return JSON.parse((await this.exchange(request, what))[1]);
  }

  /** A WebSocket of `protocol` on `path` of both: `[rust, js]`, upgraded alike. */
  async sockets(path, protocol) {
    const wires = await Promise.all(this.addresses.map((address) => Wire.connect(address)));
    const lines = `Sec-WebSocket-Protocol: ${protocol}\r\n`;
    const heads = await Promise.all(wires.map((wire) => wire.upgrade(path, lines)));
    assert.equal(heads[1], heads[0], 'the upgrade');
    return wires;
  }
}

/** The text of a request body: `query`, `operationName` and `variables`. */
function requestBody(query, operation, variables) {
  const name = operation === null ? '' : `,"operationName":${Json.str(operation).write()}`;
  return `{"query":${Json.str(query).write()}${name},"variables":${variables.write()}}`;
}

/** Replays the steps of a vector file; each answer must also be the recorded one. */
async function replaySteps(pair, vectors) {
  for (const step of vectors.get('steps').asList()) {
    const name = step.get('name').asStr();
    const body = requestBody(
      step.get('query').asStr(),
      step.get('operation')?.asStr() ?? null,
      step.get('variables') ?? Json.NULL,
    );
    const answer = Json.parse(JSON.stringify(await pair.post(body, '', name)));
    const code = step.get('error')?.asStr() ?? null;
    if (code === null) assert.equal(answer.get('data')?.write(), step.get('data').write(), name);
    else assert.ok(answer.write().includes(`"code":"${code}"`), name);
  }
}

describe('what --graphql refuses, as the Rust clink refuses it', { skip }, () => {
  it('more than one address, other endpoints, bad addresses and packages that do not read', () => {
    const graphql = (...args) => ({ args: [...SERVE.slice(0, 2), ...args, '--graphql'] });
    differential([
      graphql('--serve', '127.0.0.1:0', '--serve', '127.0.0.1:0'),
      graphql('--serve', 'unix:clink.sock'),
      graphql('--serve', 'nocolon'),
      graphql('--serve', 'h:99999'),
      graphql('--serve', '[::1]:99999'),
      { args: ['--graphql'] },
      {
        args: [...SERVE, '--deep-package', 'broken.json'],
        files: { 'broken.json': '[{"id":1,}]' },
      },
      {
        args: [...SERVE, '--deep-package', 'binary.json'],
        files: { 'binary.json': Buffer.from([0x5b, 0xff, 0x5d]) },
      },
      { args: [...SERVE, '--deep-package', 'missing.json'] },
    ]);
  });
});

describe('the recorded files of docs/protocol/graphql over both servers', LIMITS, () => {
  it('execute-vectors.json: every step, over its seed', async () => {
    const vectors = Json.parse(fixture('execute-vectors.json'));
    const both = sandboxes({ 'seed.json': vectors.get('seed').write() });
    try {
      const args = [...SERVE, ...SECRETS, '--deep-package', 'seed.json'];
      await withServers(both, args, (pair) => replaySteps(pair, vectors));
    } finally {
      remove(both);
    }
  });

  it('edge-vectors.json: every step, over deep-core.json and its seed', async () => {
    const vectors = Json.parse(fixture('edge-vectors.json'));
    const both = sandboxes({
      'deep-core.json': fixture('deep-core.json'),
      'seed.json': vectors.get('seed').write(),
    });
    try {
      const packages = ['--deep-package', 'deep-core.json', '--deep-package', 'seed.json'];
      await withServers(both, [...SERVE, ...SECRETS, ...packages], (pair) =>
        replaySteps(pair, vectors),
      );
    } finally {
      remove(both);
    }
  });

  it('deep-core.json: loads once, and its links outlive a restart', async () => {
    const both = sandboxes({ 'deep-core.json': fixture('deep-core.json') });
    const count = JSON.stringify({ query: '{links_aggregate{aggregate{count}}}' });
    const core = ['--deep-package', 'deep-core.json'];
    try {
      let before;
      await withServers(both, [...SERVE, ...SECRETS, ...core, ...core], async (pair) => {
        before = await pair.post(count, ADMIN);
        await pair.post(JSON.stringify({ query: '{guest{linkId}}' }));
      });
      await withServers(both, [...SERVE, ...SECRETS, ...core], async (pair) => {
        const after = await pair.post(count, ADMIN);
        assert.equal(
          after.data.links_aggregate.aggregate.count,
          before.data.links_aggregate.aggregate.count + 1,
        );
      });
    } finally {
      remove(both);
    }
  });

  it('deepclient-operations.json: each operation of DeepClient over deep-fresh.json.gz', async () => {
    const fresh = gunzipSync(readFileSync(fixturePath('deep-fresh.json.gz')));
    const both = sandboxes({ 'fresh.json': fresh });
    const operations = JSON.parse(fixture('deepclient-operations.json'));
    try {
      const args = [...SERVE, ...SECRETS, '--deep-package', 'fresh.json'];
      await withServers(both, args, async (pair) => {
        for (const operation of operations) {
          if (operation.websocket !== undefined) {
            await subscribeBoth(pair, operation.websocket, operation.protocol);
            continue;
          }
          const body = JSON.stringify({
            query: operation.query,
            operationName: operation.operationName,
            variables: operation.variables,
          });
          const answer = await pair.post(
            body,
            operation.auth ? ADMIN : '',
            operation.operationName,
          );
          assert.ok(Object.hasOwn(answer, 'data'), operation.operationName);
        }
      });
    } finally {
      remove(both);
    }
  });

  it('introspection.json: what the schema answers, byte for byte', async () => {
    const recorded = fixture('introspection.json').trim();
    const query = `{__schema${selectionOf(JSON.parse(recorded).data.__schema)}}`;
    const both = sandboxes();
    try {
      await withServers(both, [...SERVE, ...SECRETS], async (pair) => {
        const request = `POST /v1/graphql HTTP/1.1\r\nContent-Length: ${Buffer.byteLength(
          JSON.stringify({ query }),
        )}\r\n\r\n${JSON.stringify({ query })}`;
        const [, body] = await pair.exchange(request, 'introspection');
        assert.equal(body, recorded);
        await pair.post(JSON.stringify({ query: '{__type(name:"links"){name kind}}' }));
      });
    } finally {
      remove(both);
    }
  });

  it('where-vectors.json: each condition reads the same links', async () => {
    const both = sandboxes({ 'deep-core.json': fixture('deep-core.json') });
    const vectors = JSON.parse(fixture('where-vectors.json'));
    const query =
      'query($w: links_bool_exp){links(where: $w, order_by: {id: asc}){id type_id from_id to_id}}';
    try {
      const args = [...SERVE, ...SECRETS, '--deep-package', 'deep-core.json'];
      await withServers(both, args, async (pair) => {
        for (const vector of vectors) {
          const body = JSON.stringify({ query, variables: { w: vector.where } });
          // One condition is malformed on purpose: both refuse it alike.
          const answer = await pair.post(body, ADMIN, JSON.stringify(vector.where));
          assert.ok(Array.isArray(answer.data?.links ?? answer.errors), body);
        }
      });
    } finally {
      remove(both);
    }
  });
});

describe('HTTP and WebSocket of both servers', LIMITS, () => {
  it('answer the same at their edges', async () => {
    const both = sandboxes();
    try {
      await withServers(both, [...SERVE, ...SECRETS], async (pair) => {
        for (const request of [
          'GET /healthz HTTP/1.1\r\n\r\n',
          'GET /nothing HTTP/1.1\r\n\r\n',
          'OPTIONS /v1/graphql HTTP/1.1\r\n\r\n',
          'POST /v1/graphql HTTP/1.1\r\nContent-Length: 9223372036854775807\r\n\r\n',
        ]) {
          await pair.exchange(request);
        }
        await pair.post('{not json');
        await pair.post('{"query":1,"operationName":2}');
        const all = JSON.stringify({ query: '{links{id}}' });
        await pair.post(all, 'x-hasura-admin-secret: wrong\r\n');
        await pair.post(all, 'Authorization: Bearer a.b.c\r\n');
        const { token } = (await pair.post(JSON.stringify({ query: '{guest{token}}' }))).data.guest;
        await pair.post(all, `Authorization: Bearer ${token}\r\n`);
        await subscribeBoth(pair, '/v1/graphql', 'graphql-transport-ws');
      });
    } finally {
      remove(both);
    }
  });
});

/**
 * Opens a socket of `protocol` on `path` of both servers, subscribes to the
 * count of links, writes a link over HTTP and compares every frame.
 */
async function subscribeBoth(pair, path, protocol) {
  const wires = await pair.sockets(path, protocol);
  const legacy = protocol === 'graphql-ws';
  const frames = async (count) => {
    const read = await Promise.all(
      wires.map(async (wire) => {
        const taken = [];
        for (let at = 0; at < count; at++) taken.push((await wire.raw())[1].toString('utf8'));
        return taken;
      }),
    );
    assert.deepEqual(read[1], read[0], `frames of ${protocol}`);
  };
  try {
    for (const wire of wires) wire.send('{"type":"connection_init","payload":{}}');
    await frames(legacy ? 2 : 1);
    const subscribe = JSON.stringify({
      id: '1',
      type: legacy ? 'start' : 'subscribe',
      payload: { query: 'subscription{links_aggregate{aggregate{count}}}' },
    });
    for (const wire of wires) wire.send(subscribe);
    await frames(1);
    await pair.post(
      JSON.stringify({ query: 'mutation{insert_links_one(object:{type_id:1}){id}}' }),
      ADMIN,
    );
    await frames(1);
    for (const wire of wires) wire.write(clientFrame(0x88, Buffer.from([0x03, 0xe8])));
    await frames(1);
  } finally {
    for (const wire of wires) wire.destroy();
  }
}

/** The selection that reads every field `value` holds, in the order it holds them. */
function selectionOf(value) {
  const fields = new Map();
  const gather = (item) => {
    if (Array.isArray(item)) item.forEach(gather);
    else if (item !== null && typeof item === 'object') {
      for (const [key, inner] of Object.entries(item)) {
        if (!fields.has(key)) fields.set(key, []);
        fields.get(key).push(inner);
      }
    }
  };
  gather(value);
  const parts = [...fields].map(([key, inner]) => {
    const nested = inner.some((item) => item !== null && typeof item === 'object');
    return nested ? `${key}${selectionOf(inner)}` : key;
  });
  // A list that is empty wherever it is read takes any selection.
  return `{${parts.length === 0 ? '__typename' : parts.join(' ')}}`;
}
