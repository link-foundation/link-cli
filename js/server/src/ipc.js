// The IPC transports of the server: a port of rust/src/protocol/ipc/mod.rs
// (`connect`, `listen`, `listen_all`, `supports`, `register`,
// `MultiListener`, `forward`, `relay`, the tracing switch), dispatching to the
// ports of its submodules (ipc-net.js, ipc-udp.js, ipc-stdio.js, ipc-ring.js,
// ipc-fifo.js). The wire formats are docs/protocol/ipc-transports-v1.md.
//
// What Node.js cannot do fails with `Unsupported`, never with a fake:
// - where Rust supports the kind on this platform (`mq` on Linux, `sysvmq`
//   on Linux and macOS, `mailslot`, `copydata` and `alpc` on Windows, `unix`
//   on Windows, `shm` outside Linux, `signal` on Windows), the text says that
//   Node.js has no API for the primitive (`noNodeApi`);
// - otherwise it is Rust's own "{what} on `{kind}:` endpoints is not
//   supported on this platform".
// `supports` therefore answers what this port can do, not what the Rust
// build of the same platform can.
//
// Rust's threads are promises here: `accept`, `read`, `write`, `forward` and
// `relay` are asynchronous, and `shutdown` and `close` stay synchronous.
import { errorText } from './error-report.js';
import { diagnostics, IoError } from './protocol-errors.js';
import { setTrace, trace, traceEnabled } from './ipc-trace.js';
import { asciiLower, Endpoint, KINDS, register, registered } from './ipc-endpoint.js';
import { IpcStream } from './ipc-stream.js';
import { connectPipe, connectTcp, connectUnix, fromTcpSocket } from './ipc-net.js';
import { listenPipe, listenTcp, listenUnix } from './ipc-net.js';
import { connectUdp, listenUdp } from './ipc-udp.js';
import { connectExec, listenStdio } from './ipc-stdio.js';
import { connectRing, listenRing } from './ipc-ring.js';
import { connectFifo, listenFifo } from './ipc-fifo.js';
import { noNodeApi, sleep, unsupported } from './ipc-util.js';

export { Endpoint, IpcStream, KINDS, fromTcpSocket, register, setTrace, trace, traceEnabled };

const ON_64_BITS = new Set(['x64', 'arm64', 'ppc64', 's390x', 'loong64', 'riscv64', 'mips64el']);

/**
 * The kinds Rust has on some platform but Node.js cannot reach on any, with
 * the primitive it lacks where Rust has the kind (null elsewhere); `unix`,
 * `shm` and `signal` are checked by their ports.
 */
const MISSING_API = {
  mq: (platform) => (platform === 'linux' ? 'POSIX message queues' : null),
  sysvmq: (platform) =>
    ['linux', 'darwin', 'ios'].includes(platform) ? 'System V message queues' : null,
  mailslot: (platform) => (platform === 'win32' ? 'mailslots' : null),
  copydata: (platform) => (platform === 'win32' ? '`WM_COPYDATA` window messages' : null),
  alpc: (platform, arch) => (platform === 'win32' && ON_64_BITS.has(arch) ? 'ALPC ports' : null),
};

const elsewhere = (kind, what, { platform, arch }) => {
  const primitive = MISSING_API[kind](platform, arch);
  return primitive === null ? unsupported(kind, what) : noNodeApi(kind, primitive);
};

/** The defaults of the injectable `{platform, arch}` (`net` stands for `node:net`). */
const host = (options) => ({
  ...options,
  platform: options.platform ?? process.platform,
  arch: options.arch ?? process.arch,
});

/** `builtin_connect`. */
function builtinConnect(endpoint, options) {
  const { platform } = options;
  switch (endpoint.kind) {
    case 'tcp':
      return connectTcp(endpoint);
    case 'udp':
      return connectUdp(endpoint);
    case 'exec':
      return connectExec(endpoint);
    case 'stdio':
      throw new IoError(
        'Unsupported',
        '`stdio` can only be listened on; connect with `exec:COMMAND` instead',
      );
    case 'mmap':
    case 'file':
    case 'shm':
    case 'signal':
      return connectRing(endpoint, options);
    case 'unix':
      return connectUnix(endpoint, platform);
    case 'pipe':
      return platform === 'win32'
        ? connectPipe(endpoint, options.net)
        : connectFifo(endpoint, options);
    case 'fifo':
      return connectFifo(endpoint, options);
    default:
      throw elsewhere(endpoint.kind, 'connecting', options);
  }
}

/** `builtin_listen`. */
function builtinListen(endpoint, options) {
  const { platform } = options;
  switch (endpoint.kind) {
    case 'tcp':
      return listenTcp(endpoint);
    case 'udp':
      return listenUdp(endpoint);
    case 'stdio':
      return listenStdio(endpoint, options.io);
    case 'exec':
      throw new IoError(
        'Unsupported',
        '`exec` can only be connected to; listen on `stdio` in the started command',
      );
    case 'mmap':
    case 'file':
    case 'shm':
    case 'signal':
      return listenRing(endpoint, options);
    case 'unix':
      return listenUnix(endpoint, platform);
    case 'pipe':
      return platform === 'win32'
        ? listenPipe(endpoint, options.net)
        : listenFifo(endpoint, options);
    case 'fifo':
      return listenFifo(endpoint, options);
    default:
      throw elsewhere(endpoint.kind, 'listening', options);
  }
}

/**
 * `connect_endpoint`. `options` (`{platform, arch, alive, ...}`) exist for
 * the tests of other platforms.
 */
export async function connectEndpoint(endpoint, options = {}) {
  trace(`connect ${endpoint}`);
  try {
    const custom = registered(endpoint.kind);
    let stream;
    if (custom === undefined) stream = await builtinConnect(endpoint, host(options));
    else if (custom.connect === undefined) throw unsupported(endpoint.kind, 'connecting');
    else stream = await custom.connect(endpoint);
    trace(`connected ${endpoint}`);
    return stream;
  } catch (error) {
    trace(`connect ${endpoint} failed: ${errorText(error)}`);
    throw error;
  }
}

/** `connect`: parses `endpoint` (see `Endpoint`) and connects to it. */
export async function connect(endpoint, options = {}) {
  return connectEndpoint(Endpoint.parse(endpoint, options.platform), options);
}

/** `listen_endpoint`. */
export async function listenEndpoint(endpoint, options = {}) {
  const custom = registered(endpoint.kind);
  let listener;
  if (custom === undefined) listener = await builtinListen(endpoint, host(options));
  else if (custom.listen === undefined) throw unsupported(endpoint.kind, 'listening');
  else listener = await custom.listen(endpoint);
  trace(`listening on ${listener.endpoint()}`);
  return listener;
}

/** `listen`: parses `endpoint` and listens on it. */
export async function listen(endpoint, options = {}) {
  return listenEndpoint(Endpoint.parse(endpoint, options.platform), options);
}

/**
 * `listen_all`: one listener is returned as it is, several are merged. A
 * failure closes the listeners opened before it (Rust drops them).
 */
export async function listenAll(endpoints, options = {}) {
  const listeners = [];
  try {
    for (const endpoint of endpoints) listeners.push(await listen(endpoint, options));
  } catch (error) {
    for (const listener of listeners) listener.close();
    throw error;
  }
  if (listeners.length === 0) throw new IoError('InvalidInput', 'no endpoint to listen on');
  return listeners.length === 1 ? listeners[0] : new MultiListener(listeners);
}

/**
 * `supports`: whether this port can connect to (`listen` false) or listen on
 * `kind` on `platform`. Registered kinds answer for themselves.
 */
export function supports(kind, listen, platform = process.platform, _arch = process.arch) {
  const custom = registered(kind);
  if (custom !== undefined) return (listen ? custom.listen : custom.connect) !== undefined;
  switch (asciiLower(String(kind))) {
    case 'tcp':
    case 'udp':
    case 'mmap':
    case 'file':
    case 'pipe':
      return true;
    case 'stdio':
      return listen;
    case 'exec':
      return !listen;
    case 'unix':
    case 'fifo':
    case 'signal':
      return platform !== 'win32';
    case 'shm':
      return platform === 'linux';
    default:
      return false;
  }
}

/** `MultiListener`: accepts on every listener at once. */
export class MultiListener {
  #listeners;
  #queue = [];
  #waiters = [];
  #open;
  #closed = false;

  constructor(listeners) {
    this.#listeners = listeners;
    this.#open = listeners.length;
    this.endpointList = listeners.flatMap((listener) => listener.endpoints());
    this.address = listeners.map((listener) => listener.localAddr()).find((a) => a != null) ?? null;
    for (const listener of listeners) this.#pump(listener);
  }

  async #pump(listener) {
    for (;;) {
      let item;
      try {
        const stream = await listener.accept();
        item = stream === null ? null : { stream };
      } catch (error) {
        item = { error };
      }
      this.#push(item);
      if (item === null) return;
    }
  }

  #push(item) {
    const waiter = this.#waiters.shift();
    if (waiter === undefined) this.#queue.push(item);
    else waiter(item);
  }

  #next() {
    if (this.#queue.length > 0) return Promise.resolve(this.#queue.shift());
    return new Promise((resolve) => this.#waiters.push(resolve));
  }

  async accept() {
    while (this.#open > 0 && !this.#closed) {
      const item = await this.#next();
      if (item === undefined) break;
      if (item === null) this.#open -= 1;
      else if (item.error !== undefined) throw item.error;
      else return item.stream;
    }
    return null;
  }

  endpoint() {
    return this.endpointList.join(' ');
  }

  endpoints() {
    return [...this.endpointList];
  }

  localAddr() {
    return this.address;
  }

  /** The closer and `Drop`: closes every listener and wakes `accept`. */
  close() {
    this.#closed = true;
    for (const listener of this.#listeners) listener.close();
    for (const waiter of this.#waiters.splice(0)) waiter(undefined);
  }
}

/** `pump`: copies `from` to `to` until the end, then closes `to`'s writing. */
async function pump(from, to) {
  let total = 0;
  for (;;) {
    let chunk;
    try {
      chunk = await from.read(64 * 1024);
    } catch (error) {
      trace(`relay ${from.kind} -> ${to.kind}: ${errorText(error)}`);
      // The other direction cannot go on either.
      to.shutdown();
      from.shutdown();
      throw error;
    }
    if (chunk.length === 0) break;
    try {
      await to.write(chunk);
      await to.flush();
    } catch (error) {
      trace(`relay ${from.kind} -> ${to.kind}: ${errorText(error)}`);
      from.shutdown();
      to.shutdown();
      throw error;
    }
    total += chunk.length;
  }
  trace(`relay ${from.kind} -> ${to.kind}: end after ${total} bytes`);
  try {
    await to.closeWrite();
  } catch {
    // As Rust's `let _ = to.close_write()`.
  }
  return total;
}

/**
 * `forward`: copies both directions until both ended; resolves to the bytes
 * copied `[a to b, b to a]`.
 */
export async function forward(a, b) {
  const [ahead, back] = await Promise.allSettled([pump(a, b), pump(b, a)]);
  a.shutdown();
  b.shutdown();
  if (ahead.status === 'rejected') throw ahead.reason;
  if (back.status === 'rejected') throw back.reason;
  return [ahead.value, back.value];
}

/** One relayed connection of `relay`. */
async function relayOne(stream, target, options) {
  let outgoing;
  try {
    outgoing = await connectEndpoint(target, options);
  } catch (error) {
    diagnostics.stderr(`clink: cannot forward to ${target}: ${errorText(error)}`);
    stream.shutdown();
    return;
  }
  try {
    await forward(stream, outgoing);
  } catch (error) {
    trace(`relay to ${target} ended: ${errorText(error)}`);
  }
}

/**
 * `relay`: forwards every connection accepted on `listener` to a new
 * connection to `target` until the listener is closed or exhausted, then
 * waits for the forwarded connections to end.
 */
export async function relay(listener, target, options = {}) {
  const connections = new Set();
  for (;;) {
    let stream;
    try {
      stream = await listener.accept();
    } catch (error) {
      trace(`relay accept on ${listener.endpoint()} failed: ${errorText(error)}`);
      // A failing listener must not spin.
      await sleep(10);
      continue;
    }
    if (stream === null) break;
    const connection = relayOne(stream, target, options).finally(() =>
      connections.delete(connection),
    );
    connections.add(connection);
  }
  await Promise.all(connections);
}
