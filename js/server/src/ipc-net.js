// The socket transports of rust/src/protocol/ipc/net.rs (TCP and Unix domain
// sockets) and the Windows named pipes of rust/src/protocol/ipc/windows/mod.rs
// (`pipe_name`, `connect_pipe`, `listen_pipe`), over Node.js `net`. Adapted
// from js/client/src/tcp.js and the socket connectors of
// js/client/src/ipc-joined.js.
//
// Rust blocks a thread in `accept`; here a listener queues what `net.Server`
// delivers and `accept()` takes from that queue. Address resolution follows
// Rust's `ToSocketAddrs for str`, so a bad address fails with the same text.
//
// `unix:` on Windows: Rust uses `AF_UNIX` through Winsock
// (rust/src/protocol/ipc/windows/unix_socket.rs), but Node.js's `net` treats
// every Windows path as a named pipe, so the JavaScript server reports that
// Node.js has no API for it (`noNodeApi`); Rust's own text stays for the
// platforms where Rust lacks the kind too.
import * as dns from 'node:dns';
import * as fs from 'node:fs';
import * as net from 'node:net';
import { IoError } from './protocol-errors.js';
import { trace } from './ipc-trace.js';
import { IpcStream, SocketTransport } from './ipc-stream.js';
import { noNodeApi, Signal, timedOut } from './ipc-util.js';

const invalid = (message) => new IoError('InvalidInput', message);

/** `SocketAddr`'s `Display`: `ip:port`, or `[ip]:port` for IPv6. */
export const display = (address, port) =>
  net.isIPv6(address) ? `[${address}]:${port}` : `${address}:${port}`;

// `u16::from_str`: an optional `+` and decimal digits up to 65535.
const parsePort = (text) =>
  /^\+?[0-9]+$/.test(text) && Number(text) <= 65535 ? Number(text) : null;

// `SocketAddr::from_str`: `a.b.c.d:port` or `[v6]:port`, without a `+`.
function literal(text) {
  const v6 = /^\[([^\]]*)\]:([0-9]+)$/.exec(text);
  const v4 = /^([0-9.]+):([0-9]+)$/.exec(text);
  const [, host, port] = v6 ?? v4 ?? [];
  if (host === undefined || Number(port) > 65535) return null;
  if (v6 ? !net.isIPv6(host) : !net.isIPv4(host)) return null;
  return [{ address: host, port: Number(port), family: v6 ? 6 : 4 }];
}

/** `dns.lookup` with every address, in the resolver's order. */
export const lookupAll = (host) =>
  new Promise((resolve, reject) => {
    dns.lookup(host, { all: true, verbatim: true }, (error, addresses) =>
      error ? reject(error) : resolve(addresses),
    );
  });

// The `getaddrinfo` failure Rust reports, `gai_strerror` included.
function lookupFailed(error) {
  const detail =
    error.code === 'EAI_AGAIN'
      ? 'Temporary failure in name resolution'
      : 'Name or service not known';
  return new IoError('Uncategorized', `failed to lookup address information: ${detail}`);
}

/**
 * `address.to_socket_addrs()`: `[{address, port, family}]`. `lookup` is
 * injectable for tests; an empty result fails with `emptyText`.
 */
export async function resolveAll(text, emptyText, lookup = lookupAll) {
  let addresses = literal(text);
  if (addresses === null) {
    const colon = text.lastIndexOf(':');
    if (colon < 0) throw invalid('invalid socket address');
    const port = parsePort(text.slice(colon + 1));
    if (port === null) throw invalid('invalid port value');
    const host = text.slice(0, colon);
    if (host === '') throw lookupFailed({});
    let found;
    try {
      found = await lookup(host);
    } catch (error) {
      throw lookupFailed(error);
    }
    addresses = found.map(({ address, family }) => ({ address, port, family }));
  }
  if (addresses.length === 0) throw invalid(emptyText);
  return addresses;
}

/** `resolve(address)` of net.rs. */
export const resolve = (text, lookup = lookupAll) =>
  resolveAll(text, `\`${text}\` resolves to no address`, lookup);

/** `tcp_stream`: a connected TCP socket, with Nagle's algorithm off. */
export function tcpStream(socket) {
  try {
    socket.setNoDelay(true);
  } catch (error) {
    trace(`could not disable Nagle's algorithm: ${error.message}`);
  }
  return new IpcStream(new SocketTransport('tcp', socket));
}

/** Wraps a TCP `net.Socket` that is already connected. */
export const fromTcpSocket = async (socket) => tcpStream(socket);

/**
 * Connects a `net.Socket`; with `timeoutMs` it fails with `onTimeout()`
 * when the connection takes longer.
 */
export function connectSocket(options, timeoutMs = null, onTimeout = timedOut, api = net) {
  return new Promise((resolve, reject) => {
    const socket = api.connect({ allowHalfOpen: true, ...options });
    let timer;
    const settle = (error) => {
      clearTimeout(timer);
      socket.off('connect', connected);
      socket.off('error', settle);
      if (error === undefined) return resolve(socket);
      socket.destroy();
      reject(error);
    };
    const connected = () => settle(undefined);
    socket.once('connect', connected);
    socket.once('error', settle);
    if (timeoutMs !== null) timer = setTimeout(() => settle(onTimeout()), timeoutMs);
  });
}

/**
 * `connect_tcp`: tries every resolved address and reports the last failure.
 * `lookup` and the socket `api` are injectable for tests.
 */
export async function connectTcp(endpoint, lookup = lookupAll, api = net) {
  const timeout = endpoint.option('timeout') === undefined ? null : endpoint.timeout();
  let last;
  for (const { address, port } of await resolve(endpoint.address, lookup)) {
    try {
      const socket = await connectSocket(
        { host: address, port },
        timeout,
        () => new IoError('TimedOut', 'connection timed out'),
        api,
      );
      return tcpStream(socket);
    } catch (error) {
      last = error;
    }
  }
  throw last;
}

/** Starts `server` listening with `options`; resolves once it listens. */
export const serve = (server, options) =>
  new Promise((resolve, reject) => {
    const failed = (error) => reject(error);
    server.once('error', failed);
    server.listen(options, () => {
      server.off('error', failed);
      resolve(server);
    });
  });

/**
 * A listener over a listening `net.Server`: connections and server errors
 * queue up until `accept()` takes them; `close()` wakes a waiting `accept()`.
 * `describe` names the listener, `accepted(socket)` makes the stream.
 */
export class SocketListener {
  #server;
  #queue = [];
  #signal = new Signal();
  #closed = false;

  constructor(server, { endpoint, localAddr = null, accepted, cleanup = () => {} }) {
    this.#server = server;
    this.endpointText = endpoint;
    this.address = localAddr;
    this.accepted = accepted;
    this.cleanup = cleanup;
    server.on('connection', (socket) => this.#push({ socket }));
    server.on('error', (error) => this.#push({ error }));
  }

  #push(item) {
    if (this.#closed) {
      item.socket?.destroy();
      return;
    }
    this.#queue.push(item);
    this.#signal.notify();
  }

  get closed() {
    return this.#closed;
  }

  async accept() {
    for (;;) {
      if (this.#closed) return null;
      const item = this.#queue.shift();
      if (item === undefined) {
        await this.#signal.wait();
      } else if (item.error !== undefined) {
        throw item.error;
      } else {
        return this.accepted(item.socket);
      }
    }
  }

  endpoint() {
    return this.endpointText;
  }

  endpoints() {
    return [this.endpoint()];
  }

  localAddr() {
    return this.address;
  }

  close() {
    if (this.#closed) return;
    this.#closed = true;
    this.#server.close(() => {});
    for (const { socket } of this.#queue.splice(0)) socket?.destroy();
    this.#signal.notify();
    this.cleanup();
  }
}

/** `listen_tcp`: binds the first resolved address that accepts the bind. */
export async function listenTcp(endpoint, lookup = lookupAll) {
  let last;
  const addresses = await resolveAll(
    endpoint.address,
    'could not resolve to any addresses',
    lookup,
  );
  for (const { address, port } of addresses) {
    const server = net.createServer({ allowHalfOpen: true, pauseOnConnect: true });
    try {
      await serve(server, { host: address, port });
    } catch (error) {
      last = error;
      continue;
    }
    const bound = server.address();
    return new SocketListener(server, {
      endpoint: display(bound.address, bound.port),
      localAddr: { address: bound.address, port: bound.port, family: bound.family },
      accepted: (socket) => {
        trace(`tcp accepted ${display(socket.remoteAddress, socket.remotePort)}`);
        socket.resume();
        return tcpStream(socket);
      },
    });
  }
  throw last;
}

// ---------------------------------------------------------------- unix

// The socket path of `unix:NAME`; `@NAME` is abstract on Linux and Android.
function unixPath(name, platform) {
  if (!name.startsWith('@')) return name;
  if (platform === 'linux' || platform === 'android') return `\0${name.slice(1)}`;
  throw new IoError('Unsupported', 'abstract `unix:@NAME` sockets exist only on Linux');
}

const unixOnWindows = () => noNodeApi('unix', '`AF_UNIX` sockets on Windows');

const unixStream = (socket) => new IpcStream(new SocketTransport('unix', socket));

/** `connect_unix`. */
export async function connectUnix(endpoint, platform = process.platform) {
  if (platform === 'win32') throw unixOnWindows();
  const path = unixPath(endpoint.address, platform);
  return unixStream(await connectSocket({ path }));
}

/** `listen_unix`: a live socket file refuses, a stale one is removed. */
export async function listenUnix(endpoint, platform = process.platform) {
  if (platform === 'win32') throw unixOnWindows();
  const name = endpoint.address;
  const path = unixPath(name, platform);
  const file = !name.startsWith('@');
  if (file && fs.existsSync(path)) {
    let live = false;
    try {
      (await connectSocket({ path })).destroy();
      live = true;
    } catch {
      trace(`unix removes the stale socket \`${name}\``);
      fs.unlinkSync(path);
    }
    if (live) throw new IoError('AddrInUse', `a server already listens on \`${name}\``);
  }
  const server = net.createServer({ allowHalfOpen: true, pauseOnConnect: true });
  await serve(server, { path });
  return new SocketListener(server, {
    endpoint: `unix:${name}`,
    accepted: (socket) => {
      trace(`unix accepted on ${name}`);
      socket.resume();
      return unixStream(socket);
    },
    cleanup: () => {
      if (file) fs.rmSync(path, { force: true });
    },
  });
}

// ---------------------------------------------------------------- Windows named pipes

/** `pipe_name`: `\\.\pipe\NAME`, or a full `\\server\pipe\NAME` as given. */
export function pipeName(address) {
  if (address === '') throw invalid('`pipe:` needs a name');
  return address.startsWith('\\\\') ? address : `\\\\.\\pipe\\${address}`;
}

const pipeStream = (socket) =>
  new IpcStream(new SocketTransport('pipe', socket, { halfClose: false }));

/**
 * `connect_pipe`: libuv waits while every instance is busy; the endpoint
 * timeout bounds that wait, as Rust's `WaitNamedPipeW` loop does.
 */
export async function connectPipe(endpoint, api = net) {
  const name = pipeName(endpoint.address);
  let socket;
  try {
    socket = await connectSocket({ path: name }, endpoint.timeout(), timedOut, api);
  } catch (error) {
    if (error.code === 'ENOENT') {
      throw new IoError('ConnectionRefused', `nobody listens on \`${name}\``);
    }
    throw error;
  }
  trace(`pipe connected to ${name}`);
  return pipeStream(socket);
}

/**
 * `listen_pipe`: libuv creates the first instance with
 * `FILE_FLAG_FIRST_PIPE_INSTANCE` and reports `ERROR_ACCESS_DENIED` as
 * `EADDRINUSE`.
 */
export async function listenPipe(endpoint, api = net) {
  const full = pipeName(endpoint.address);
  const server = api.createServer({ allowHalfOpen: true, pauseOnConnect: true });
  try {
    await serve(server, { path: full });
  } catch (error) {
    if (error.code === 'EADDRINUSE') {
      throw new IoError('AddrInUse', `a server already listens on \`${full}\``);
    }
    throw error;
  }
  return new SocketListener(server, {
    endpoint: `pipe:${endpoint.address}`,
    accepted: (socket) => {
      trace(`pipe accepted on ${endpoint.address}`);
      socket.resume();
      return pipeStream(socket);
    },
  });
}
