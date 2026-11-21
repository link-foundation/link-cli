// HTTP and WebSocket transport of the GraphQL layer.
// Port of rust/src/graphql/server.rs.
//
// Rust gives each connection a thread and runs every request on the serving
// thread through a channel; here each connection is an async reader and a
// request runs to its end before the next one starts, so the store is never
// shared either. After each write every subscription runs again, and is sent
// if its answer changed. WebSocket speaks the subprotocols `graphql-ws`
// (`subscriptions-transport-ws`, which `DeepClient` uses) and
// `graphql-transport-ws` (the `graphql-ws` library).

import { lookup as dnsLookup } from 'node:dns';
import { createServer, isIPv4, isIPv6 } from 'node:net';

import { websocketAccept } from './graphql-crypto.js';
import { GqlError } from './graphql-error.js';
import { execute } from './graphql-execute.js';
import { Json, parseU64, trimRust } from './graphql-json.js';
import { asciiLower } from './ipc-endpoint.js';
import { IoError } from './protocol-errors.js';

const MAX_BODY = 64 * 1024 * 1024;

const CORS =
  'Access-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: GET, POST, OPTIONS\r\n';
/** Headers allowed when a preflight does not name any; Hasura echoes the ones it is asked about. */
const CORS_HEADERS =
  'authorization, content-type, x-hasura-admin-secret, x-hasura-client, x-hasura-role, x-hasura-user-id';

const GRAPHQL_PATHS = ['/v1/graphql', '/gql', '/v1alpha1/graphql'];

const strict = new TextDecoder('utf-8', { fatal: true });
const lossy = new TextDecoder();

/** What a socket or the listener reports once nobody waits for it. */
const ignore = () => {};

/** The texts of `gai_strerror` of the GNU C library, by the codes Node.js gives them. */
const GAI_TEXT = {
  EAI_AGAIN: 'Temporary failure in name resolution',
  EAI_FAIL: 'Non-recoverable failure in name resolution',
  EAI_MEMORY: 'Memory allocation failure',
  EAI_FAMILY: 'ai_family not supported',
  EAI_SERVICE: 'Servname not supported for ai_socktype',
};
/** libuv's `UV_EAI_NODATA`, which Node.js reports as `ENOTFOUND` too. */
const UV_EAI_NODATA = -3007;

/** The `io::Error` Rust makes of a failed `getaddrinfo`. */
function lookupError(detail) {
  return new IoError('Uncategorized', `failed to lookup address information: ${detail}`);
}

function gaiText(error) {
  if (error.code === 'ENOTFOUND') {
    return error.errno === UV_EAI_NODATA
      ? 'No address associated with hostname'
      : 'Name or service not known';
  }
  return GAI_TEXT[error.code] ?? error.message;
}

/**
 * The addresses `TcpListener::bind` tries, as `[host, port]`, in the order
 * of `ToSocketAddrs for str`: a socket address, else `host:port` with an IP
 * or a name to look up.
 */
export async function socketAddresses(address, lookup = dnsLookup) {
  const bracketed = /^\[(.*)\]:([0-9]+)$/.exec(address);
  if (bracketed !== null && isIPv6(bracketed[1]) && Number(bracketed[2]) <= 65535) {
    return [[bracketed[1], Number(bracketed[2])]];
  }
  const colon = address.lastIndexOf(':');
  if (colon < 0) throw new IoError('InvalidInput', 'invalid socket address');
  const host = address.slice(0, colon);
  const port = parseU64(address.slice(colon + 1));
  if (port === null || port > 65535n) throw new IoError('InvalidInput', 'invalid port value');
  if (isIPv4(host) || isIPv6(host)) return [[host, Number(port)]];
  // `getaddrinfo` finds nothing for an empty name; Node.js refuses to ask.
  if (host === '') throw lookupError('No address associated with hostname');
  const found = await new Promise((resolve, reject) => {
    lookup(host, { all: true, verbatim: true }, (error, list) => {
      if (error) reject(lookupError(gaiText(error)));
      else resolve(list);
    });
  });
  return found.map((entry) => [entry.address, Number(port)]);
}

function listen(server, host, port) {
  return new Promise((resolve, reject) => {
    server.once('error', reject);
    server.listen({ host, port }, () => {
      server.off('error', reject);
      resolve();
    });
  });
}

/** Bytes of a socket, read as `BufReader` does: lines and exact lengths. */
class Reader {
  #chunks = [];
  #length = 0;
  #ended = false;
  #wake = ignore;

  constructor(socket) {
    socket.on('data', (chunk) => {
      this.#chunks.push(chunk);
      this.#length += chunk.length;
      this.#wake();
    });
    const end = () => {
      this.#ended = true;
      this.#wake();
    };
    socket.on('end', end);
    socket.on('close', end);
  }

  #more() {
    return new Promise((resolve) => {
      this.#wake = resolve;
    });
  }

  #take(count) {
    const all = Buffer.concat(this.#chunks, this.#length);
    this.#chunks = [all.subarray(count)];
    this.#length -= count;
    return all.subarray(0, count);
  }

  /** `read_line`: a line with its `\n`, what is left at the end, or `null` when nothing is. */
  async readLine() {
    let scanned = 0;
    for (;;) {
      const all = Buffer.concat(this.#chunks, this.#length);
      this.#chunks = [all];
      const at = all.indexOf(10, scanned);
      if (at >= 0) return strict.decode(this.#take(at + 1));
      if (this.#ended) return this.#length === 0 ? null : strict.decode(this.#take(this.#length));
      scanned = all.length;
      await this.#more();
    }
  }

  /** `read_exact`: `count` bytes, or a failure when the peer ends first. */
  async readExact(count) {
    while (this.#length < count) {
      if (this.#ended) throw new IoError('UnexpectedEof', 'failed to fill whole buffer');
      await this.#more();
    }
    return this.#take(count);
  }
}

/** `{"type":..,"id":..,"payload":..}`, without the parts that are `null`. */
export function message(kind, id, payload) {
  const idPart = id === null ? '' : `,"id":${Json.str(id).write()}`;
  const payloadPart = payload === null ? '' : `,"payload":${payload}`;
  return `{"type":"${kind}"${idPart}${payloadPart}}`;
}

/** One final WebSocket frame, unmasked as a server sends it. */
export function frame(opcode, payload) {
  const bytes = typeof payload === 'string' ? Buffer.from(payload) : payload;
  let head;
  if (bytes.length <= 125) head = Buffer.from([0x80 | opcode, bytes.length]);
  else if (bytes.length <= 65535) {
    head = Buffer.from([0x80 | opcode, 126, 0, 0]);
    head.writeUInt16BE(bytes.length, 2);
  } else {
    head = Buffer.alloc(10);
    head[0] = 0x80 | opcode;
    head[1] = 127;
    head.writeBigUInt64BE(BigInt(bytes.length), 2);
  }
  return Buffer.concat([head, bytes]);
}

/** Sends a frame; `false` when the socket no longer takes writes, as Rust's `Err`. */
export function sendFrame(socket, opcode, payload) {
  if (!socket.writable) return false;
  socket.write(frame(opcode, payload));
  return true;
}

function sendData(socket, id, payload, legacy) {
  return sendFrame(socket, 1, message(legacy ? 'data' : 'next', id, payload));
}

/** A failed subscription: `errors`, or the whole response as data for `graphql-ws`. */
function sendError(socket, id, response, legacy) {
  if (legacy) return sendData(socket, id, response.write(), true);
  return sendFrame(socket, 1, message('error', id, response.get('errors').write()));
}

/** Reads one WebSocket message, joining fragments and answering pings. */
async function readMessage(reader, socket) {
  const parts = [];
  for (;;) {
    const header = await reader.readExact(2);
    const opcode = header[0] & 0x0f;
    let length = BigInt(header[1] & 0x7f);
    if (length === 126n) length = BigInt((await reader.readExact(2)).readUInt16BE(0));
    else if (length === 127n) length = (await reader.readExact(8)).readBigUInt64BE(0);
    if (length > BigInt(MAX_BODY)) return null;
    const mask = header[1] & 0x80 ? await reader.readExact(4) : Buffer.alloc(4);
    const payload = Buffer.from(await reader.readExact(Number(length)));
    for (let at = 0; at < payload.length; at += 1) payload[at] ^= mask[at % 4];
    if (opcode === 8) {
      sendFrame(socket, 8, payload);
      return null;
    }
    if (opcode === 9) sendFrame(socket, 10, payload);
    else if (opcode !== 10) {
      parts.push(payload);
      if (header[0] & 0x80) return lossy.decode(Buffer.concat(parts));
    }
  }
}

function requestParts(body) {
  return [
    body.get('query')?.asStr() ?? '',
    body.get('operationName')?.asStr() ?? null,
    body.get('variables') ?? Json.NULL,
  ];
}

function respond(socket, status, body) {
  socket.write(
    `HTTP/1.1 ${status}\r\nContent-Type: application/json; charset=utf-8\r\n${CORS}` +
      `Access-Control-Allow-Headers: ${CORS_HEADERS}\r\nContent-Length: ${Buffer.byteLength(body)}\r\n\r\n${body}`,
  );
}

/** Closes a socket once what was written to it is sent. */
function close(socket) {
  socket.end(() => socket.destroy());
}

const GUEST = Object.freeze({ user: null, admin: false });
/** What `connection_init` without credentials gives: the connection keeps its subject. */
const KEEP = Symbol('keep');

/** A GraphQL server on one TCP address. */
export class GraphqlServer {
  #server;
  #sockets = new Set();
  #pending = [];
  #store = null;
  #settings = null;
  #subscriptions = [];
  #connections = 0;
  #stopping = false;
  #finish;
  #done;

  constructor(server) {
    this.#server = server;
    this.#done = new Promise((resolve) => {
      this.#finish = resolve;
    });
    server.on('error', ignore);
    server.on('connection', (socket) => {
      this.#sockets.add(socket);
      socket.on('error', ignore);
      socket.on('close', () => this.#sockets.delete(socket));
      if (this.#store === null) this.#pending.push(socket);
      else this.#accept(socket);
    });
  }

  /**
   * `TcpListener::bind`: listens on the first address `address` resolves to
   * that takes it; the failure of the last one otherwise. `lookup` is
   * `dns.lookup`'s shape.
   */
  static async bind(address, { lookup = dnsLookup } = {}) {
    let failure = new IoError('InvalidInput', 'could not resolve to any addresses');
    for (const [host, port] of await socketAddresses(address, lookup)) {
      const server = createServer();
      try {
        await listen(server, host, port);
        return new GraphqlServer(server);
      } catch (error) {
        failure = error;
      }
    }
    throw failure;
  }

  /** The address listened on, as Rust's `SocketAddr` prints it. */
  localAddr() {
    const { address, port } = this.#server.address();
    return address.includes(':') ? `[${address}]:${port}` : `${address}:${port}`;
  }

  /** Serves until `shutdown`; the promise settles then. */
  serve(store, settings) {
    this.#store = store;
    this.#settings = settings;
    for (const socket of this.#pending.splice(0)) this.#accept(socket);
    return this.#done;
  }

  /** Stops serving: no more connections, and those open are closed. */
  shutdown() {
    if (this.#stopping) return;
    this.#stopping = true;
    this.#server.close();
    for (const socket of this.#sockets) socket.destroy();
    this.#finish();
  }

  async #accept(socket) {
    this.#connections += 1;
    const connection = this.#connections;
    const reader = new Reader(socket);
    try {
      await this.#handle(reader, socket, connection);
    } catch {
      // Text that is not UTF-8 or a peer that left mid-request ends the connection.
    }
    close(socket);
  }

  async #handle(reader, socket, connection) {
    for (;;) {
      const line = await reader.readLine();
      if (line === null) return;
      const words = trimRust(line).split(/\p{White_Space}+/u);
      const method = words[0];
      const path = (words[1] ?? '').split('?')[0];
      const headers = new Map();
      for (;;) {
        const raw = await reader.readLine();
        if (raw === null) return;
        const text = raw.replace(/\p{White_Space}+$/u, '');
        if (text === '') break;
        const colon = text.indexOf(':');
        if (colon >= 0) {
          headers.set(asciiLower(trimRust(text.slice(0, colon))), trimRust(text.slice(colon + 1)));
        }
      }
      const graphql = GRAPHQL_PATHS.includes(path);
      const upgrade = headers.get('upgrade');
      if (upgrade !== undefined && asciiLower(upgrade) === 'websocket' && graphql) {
        await this.#websocket(reader, socket, headers, connection);
        return;
      }
      const length = parseU64(headers.get('content-length') ?? '') ?? 0n;
      if (length > BigInt(MAX_BODY)) {
        respond(socket, '413 Payload Too Large', '{}');
        return;
      }
      const body = await reader.readExact(Number(length));
      this.#route(socket, method, path, graphql, headers, body);
    }
  }

  #route(socket, method, path, graphql, headers, body) {
    if (method === 'OPTIONS') {
      const allowed = headers.get('access-control-request-headers') ?? CORS_HEADERS;
      socket.write(
        `HTTP/1.1 204 No Content\r\n${CORS}Access-Control-Allow-Headers: ${allowed}\r\nContent-Length: 0\r\n\r\n`,
      );
    } else if (method === 'GET' && ['/healthz', '/api/healthz', '/v1/healthz'].includes(path)) {
      respond(socket, '200 OK', '{"status":"ok"}');
    } else if (method === 'GET' && ['/v1/version', '/v1/version/'].includes(path)) {
      respond(socket, '200 OK', '{"version":"clink"}');
    } else if (method === 'POST' && graphql) {
      const before = this.#store.generation;
      respond(socket, '200 OK', this.#post(headers, body).write());
      if (this.#store.generation !== before) this.#refresh();
    } else respond(socket, '404 Not Found', '{"error":"resource does not exist"}');
  }

  #post(headers, body) {
    let auth;
    try {
      auth = this.#authorize(headers.get('authorization'), headers.get('x-hasura-admin-secret'));
    } catch (error) {
      return new GqlError(error.message, 'invalid-jwt', '$').toJson();
    }
    let request;
    try {
      request = Json.parse(lossy.decode(body));
    } catch (error) {
      return GqlError.validation(error.message, '$').toJson();
    }
    const [query, operation, variables] = requestParts(request);
    return execute(this.#store, this.#settings, query, operation, variables, auth)[0];
  }

  #authorize(authorization, adminSecret) {
    return this.#settings.authorize(authorization ?? null, adminSecret ?? null);
  }

  async #websocket(reader, socket, headers, connection) {
    const key = headers.get('sec-websocket-key') ?? '';
    const offered = headers.get('sec-websocket-protocol') ?? '';
    const legacy = !offered.split(',').some((name) => trimRust(name) === 'graphql-transport-ws');
    const chosen = legacy ? 'graphql-ws' : 'graphql-transport-ws';
    socket.write(
      'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n' +
        `Sec-WebSocket-Accept: ${websocketAccept(key)}\r\nSec-WebSocket-Protocol: ${chosen}\r\n\r\n`,
    );
    let auth;
    try {
      auth = this.#authorize(headers.get('authorization'), headers.get('x-hasura-admin-secret'));
    } catch {
      auth = GUEST;
    }
    try {
      for (;;) {
        const text = await readMessage(reader, socket);
        if (text === null) return;
        let incoming;
        try {
          incoming = Json.parse(text);
        } catch {
          continue;
        }
        const id = incoming.get('id')?.asStr() ?? null;
        const payload = incoming.get('payload') ?? Json.NULL;
        const kind = incoming.get('type')?.asStr() ?? '';
        if (kind === 'connection_init') {
          const found = this.#init(socket, payload, legacy);
          if (found === null) return;
          if (found !== KEEP) auth = found;
        } else if (kind === 'start' || kind === 'subscribe') {
          this.#subscribe(connection, id ?? '', payload, auth, legacy, socket);
        } else if (kind === 'stop' || kind === 'complete') {
          this.#stop(connection, id);
          if (legacy) sendFrame(socket, 1, message('complete', id, null));
        } else if (kind === 'ping') sendFrame(socket, 1, message('pong', null, null));
        else if (kind === 'connection_terminate') return;
      }
    } finally {
      this.#stop(connection, null);
    }
  }

  /**
   * `connection_init`: the subject its headers give, `KEEP` when they give
   * none, `null` when they are refused.
   */
  #init(socket, payload, legacy) {
    const holder = payload.get('headers') ?? payload;
    const found = (name, lower) => (holder.get(name) ?? holder.get(lower))?.asStr() ?? null;
    const authorization = found('Authorization', 'authorization');
    const admin = found('x-hasura-admin-secret', 'x-hasura-admin-secret');
    let auth;
    try {
      auth = this.#authorize(authorization, admin);
    } catch (problem) {
      const error = Json.object([['message', Json.str(problem.message)]]).write();
      sendFrame(socket, 1, message('connection_error', null, error));
      return null;
    }
    sendFrame(socket, 1, message('connection_ack', null, null));
    if (legacy) sendFrame(socket, 1, message('ka', null, null));
    return authorization !== null || admin !== null ? auth : KEEP;
  }

  #subscribe(connection, id, payload, auth, legacy, socket) {
    const [query, operation, variables] = requestParts(payload);
    const before = this.#store.generation;
    const [response] = execute(this.#store, this.#settings, query, operation, variables, auth);
    if (response.get('errors') !== null) sendError(socket, id, response, legacy);
    else {
      const last = response.write();
      sendData(socket, id, last, legacy);
      this.#subscriptions.push({
        connection,
        id,
        query,
        operation,
        variables,
        auth,
        legacy,
        socket,
        last,
      });
    }
    if (this.#store.generation !== before) this.#refresh();
  }

  #stop(connection, id) {
    this.#subscriptions = this.#subscriptions.filter(
      (subscription) =>
        subscription.connection !== connection || (id !== null && id !== subscription.id),
    );
  }

  /** Runs every subscription again and sends those whose answer changed. */
  #refresh() {
    this.#subscriptions = this.#subscriptions.filter((subscription) => {
      const { query, operation, variables, auth } = subscription;
      const [response] = execute(this.#store, this.#settings, query, operation, variables, auth);
      const text = response.write();
      if (text === subscription.last) return true;
      subscription.last = text;
      return sendData(subscription.socket, subscription.id, text, subscription.legacy);
    });
  }
}
