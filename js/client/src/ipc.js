// The links protocol over every IPC mechanism of
// docs/protocol/ipc-transports-v1.md, for Node.js. Kinds Node.js has an API
// for are spoken natively, byte for byte like the Rust reference
// (rust/src/protocol/ipc/); the others go through a bridge child process,
// `clink --serve stdio --forward ENDPOINT`, which still uses exactly the
// requested mechanism between the bridge and the server.

import { spawn } from 'node:child_process';
import net from 'node:net';
import { LinksClient } from './client.js';
import { scopeError, scopeOf, watchScope } from './deadline.js';
import { ConnectionClosedError, LinksError } from './errors.js';
import { connectUdp, resolveAddresses } from './ipc-datagram.js';
import {
  commandLine,
  Endpoint,
  IpcError,
  parseEndpoint,
  registeredConnect,
} from './ipc-endpoint.js';
import { connectFifo } from './ipc-fifo.js';
import { JoinedStream } from './ipc-joined.js';
import { connectRing } from './ipc-ring.js';
import { trace, traceEnabled } from './ipc-trace.js';
import { TcpTransport, verifyProtocol } from './tcp.js';

export { Endpoint, IpcError, KINDS, parseEndpoint, registerTransport } from './ipc-endpoint.js';
export { setTrace, traceEnabled } from './ipc-trace.js';

const PLATFORM = process.platform;
const ARCH = process.arch;

/**
 * The kinds the Rust `clink` (the bridge) connects to on `platform` and
 * `arch` (default: this machine).
 */
export function bridgeableKinds(platform = PLATFORM, arch = ARCH) {
  const kinds = ['tcp', 'udp', 'exec', 'pipe', 'file', 'mmap', 'shm', 'signal', 'unix'];
  const windows = platform === 'win32';
  if (!windows) kinds.push('fifo');
  if (platform === 'linux' || platform === 'darwin') kinds.push('sysvmq');
  if (platform === 'linux') kinds.push('mq');
  if (windows) kinds.push('mailslot', 'copydata');
  if (windows && ['x64', 'arm64'].includes(arch)) kinds.push('alpc');
  return new Set(kinds);
}

const BRIDGEABLE = bridgeableKinds();

const lower = (kind) => String(kind).replace(/[A-Z]/g, (letter) => letter.toLowerCase());

/** Whether `kind` is connected to without a bridge on `platform` (custom kinds count). */
export function supportsNatively(kind, platform = PLATFORM) {
  return (
    registeredConnect(kind) !== undefined || nativeConnect(lower(kind), platform) !== undefined
  );
}

/** Whether `kind` can be connected to on `platform` and `arch`, natively or through the bridge. */
export function supports(kind, platform = PLATFORM, arch = ARCH) {
  return supportsNatively(kind, platform) || bridgeableKinds(platform, arch).has(lower(kind));
}

const unsupported = (kind) =>
  new IpcError(`connecting on \`${kind}:\` endpoints is not supported on this platform`, 'ENOTSUP');

// A stream that has opened, then the connect gave up: close it again.
const discard = (stream) => {
  try {
    stream?.destroy();
  } catch {
    // Nothing more to release.
  }
};

/** Waits for a `node:net` socket to connect; `signal` destroys it. */
function socketConnected(socket, signal, describe = (error) => error) {
  return new Promise((resolve, reject) => {
    const finish = (error) => {
      socket.off('connect', onConnect);
      socket.off('error', onError);
      signal?.removeEventListener('abort', onAbort);
      if (error) {
        socket.destroy();
        reject(error);
      } else {
        resolve(socket);
      }
    };
    const onConnect = () => finish();
    const onError = (error) => finish(describe(error));
    const onAbort = () => finish(signal.reason);
    socket.once('connect', onConnect);
    socket.once('error', onError);
    signal?.addEventListener('abort', onAbort, { once: true });
  });
}

async function connectTcpStream(endpoint, { signal }) {
  const addresses = await resolveAddresses(endpoint.address);
  const timeout = endpoint.option('timeout') === undefined ? undefined : endpoint.timeout();
  let last;
  for (const { host, port } of addresses) {
    const socket = net.connect({ host, port, allowHalfOpen: true, noDelay: true });
    const timer =
      timeout === undefined
        ? undefined
        : setTimeout(
            () => socket.destroy(new IpcError('connection timed out', 'ETIMEDOUT')),
            timeout,
          );
    try {
      await socketConnected(socket, signal);
      socket.setNoDelay(true);
      return socket;
    } catch (error) {
      if (signal?.aborted) throw error;
      last = error;
    } finally {
      clearTimeout(timer);
    }
  }
  throw last;
}

async function connectUnix(endpoint, { signal }, platform) {
  let path = endpoint.address;
  if (path.startsWith('@')) {
    if (platform !== 'linux') {
      throw new IpcError('abstract `unix:@NAME` sockets exist only on Linux', 'ENOTSUP');
    }
    path = `\0${path.slice(1)}`;
  }
  return socketConnected(net.connect({ path, allowHalfOpen: true }), signal);
}

/** `\\.\pipe\NAME` for a bare name, a full `\\server\pipe\NAME` as it is. */
export function pipeName(address) {
  if (address === '') throw new IpcError('`pipe:` needs a name', 'EINVAL');
  return address.startsWith('\\\\') ? address : `\\\\.\\pipe\\${address}`;
}

async function connectWindowsPipe(endpoint, { signal }) {
  const name = pipeName(endpoint.address);
  const socket = await socketConnected(
    net.connect({ path: name, allowHalfOpen: true }),
    signal,
    (error) =>
      error.code === 'ENOENT'
        ? new IpcError(`nobody listens on \`${name}\``, 'ECONNREFUSED', { cause: error })
        : error,
  );
  // A named pipe cannot half-close: ending the outgoing direction closes it.
  socket.once('finish', () => socket.destroy());
  trace(`pipe connected to ${name}`);
  return socket;
}

/** Waits up to 5 s for a child to exit, then kills it (as the Rust `exec` does). */
function reap(child, program) {
  if (child.exitCode !== null || child.signalCode !== null) return Promise.resolve();
  return new Promise((resolve) => {
    const timer = setTimeout(() => {
      trace(`exec \`${program}\` killed after 5 s`);
      child.kill('SIGKILL');
    }, 5000);
    child.once('exit', (code, signal) => {
      clearTimeout(timer);
      trace(`exec \`${program}\` exited: ${signal ? `signal: ${signal}` : `exit status: ${code}`}`);
      resolve();
    });
  });
}

/** Spawns `argv` and resolves once it runs; `error` describes a start failure. */
function started(argv, options, signal, describe) {
  const child = spawn(argv[0], argv.slice(1), { ...options, shell: false, windowsHide: true });
  return new Promise((resolve, reject) => {
    const finish = (error) => {
      child.off('spawn', onSpawn);
      child.off('error', onError);
      signal?.removeEventListener('abort', onAbort);
      if (error) reject(error);
      else resolve(child);
    };
    const onSpawn = () => finish();
    const onError = (error) => finish(describe(error));
    const onAbort = () => {
      child.kill('SIGKILL');
      finish(signal.reason);
    };
    child.once('spawn', onSpawn);
    child.once('error', onError);
    signal?.addEventListener('abort', onAbort, { once: true });
  });
}

async function connectExec(endpoint, { signal }) {
  const argv = commandLine(endpoint.address);
  const child = await started(argv, { stdio: ['pipe', 'pipe', 'inherit'] }, signal, (error) =>
    Object.assign(new IpcError(`cannot start \`${argv[0]}\`: ${error.message}`, error.code), {
      cause: error,
    }),
  );
  trace(`exec started \`${argv[0]}\` as process ${child.pid}`);
  const stream = new JoinedStream(child.stdout, child.stdin, {
    release: () => reap(child, argv[0]),
  });
  stream.pid = child.pid;
  return stream;
}

/** The `clink` command the bridge runs. */
export const bridgeCommand = (options = {}) => options.clink ?? process.env.CLINK_BIN ?? 'clink';

const TAIL = 4096;

/**
 * Connects through `clink --serve stdio --forward ENDPOINT`. The bridge runs
 * with its `[ipc]` log on, which tells when its own connection is up; the
 * log is passed on when this process traces too.
 */
async function connectBridge(endpoint, options) {
  const { signal } = options;
  const clink = bridgeCommand(options);
  const text = endpoint.toString();
  const argv = [clink, '--serve', 'stdio', '--forward', text];
  const child = await started(
    argv,
    { stdio: ['pipe', 'pipe', 'pipe'], env: { ...process.env, CLINK_IPC_TRACE: '1' } },
    signal,
    (error) =>
      Object.assign(
        new IpcError(
          `cannot start the bridge \`${clink}\` for \`${endpoint.kind}:\` endpoints (install clink, or set CLINK_BIN): ${error.message}`,
          error.code,
        ),
        { cause: error },
      ),
  );
  trace(`bridge started \`${clink}\` as process ${child.pid} for ${text}`);
  let tail = '';
  let pending = '';
  let onLine = () => {};
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', (chunk) => {
    pending += chunk;
    let newline;
    while ((newline = pending.indexOf('\n')) >= 0) {
      const line = pending.slice(0, newline).replace(/\r$/, '');
      pending = pending.slice(newline + 1);
      if (!line.startsWith('[ipc] ')) tail = `${tail}${line}\n`.slice(-TAIL);
      if (traceEnabled()) trace(`bridge: ${line.replace(/^\[ipc\] /, '')}`);
      onLine(line);
    }
  });
  const failure = (detail) => {
    const lines = tail
      .split('\n')
      .filter((line) => line !== '' && !line.startsWith('clink server listening on'));
    const reason = detail ?? lines.at(-1) ?? 'it exited';
    return new IpcError(`the bridge to ${text} failed: ${reason}`, 'ECONNREFUSED');
  };
  try {
    await new Promise((resolve, reject) => {
      // The bridge connects within the endpoint's own timeout.
      const timer = setTimeout(
        () => finish(new IpcError(`the bridge to ${text} did not connect in time`, 'ETIMEDOUT')),
        endpoint.timeout() + 10_000,
      );
      const finish = (error) => {
        clearTimeout(timer);
        onLine = () => {};
        child.off('exit', onExit);
        signal?.removeEventListener('abort', onAbort);
        if (error) reject(error);
        else resolve();
      };
      onLine = (line) => {
        if (line.startsWith('[ipc] connected ')) finish();
        else if (line.startsWith('clink: cannot forward to ')) {
          const reason = line.slice('clink: cannot forward to '.length);
          finish(failure(reason.slice(reason.indexOf(': ') + 2) || reason));
        }
      };
      const onExit = () => setImmediate(() => finish(failure()));
      const onAbort = () => finish(signal.reason);
      child.once('exit', onExit);
      signal?.addEventListener('abort', onAbort, { once: true });
    });
  } catch (error) {
    child.stdin.destroy();
    child.kill('SIGKILL');
    throw error;
  }
  const stream = new JoinedStream(child.stdout, child.stdin, {
    release: () => reap(child, clink),
  });
  stream.pid = child.pid;
  stream.bridged = true;
  return stream;
}

// A connect function that also learns the platform it connects on.
const on = (connect, platform) => (endpoint, options) => connect(endpoint, options, platform);

/**
 * The function this client connects to `kind` with on `platform` (default:
 * this one) without a bridge, if any.
 */
export function nativeConnect(kind, platform = PLATFORM) {
  const windows = platform === 'win32';
  switch (kind) {
    case 'tcp':
      return connectTcpStream;
    case 'udp':
      return connectUdp;
    case 'exec':
      return connectExec;
    case 'unix':
      return windows ? undefined : on(connectUnix, platform);
    case 'pipe':
      return windows ? connectWindowsPipe : connectFifo;
    case 'fifo':
      return windows ? undefined : connectFifo;
    case 'file':
    case 'mmap':
      return on(connectRing, platform);
    case 'signal':
      return windows ? undefined : on(connectRing, platform);
    case 'shm':
      return platform === 'linux' ? on(connectRing, platform) : undefined;
    default:
      return undefined;
  }
}

function open(endpoint, options) {
  const custom = registeredConnect(endpoint.kind);
  if (custom) return Promise.resolve(custom(endpoint, options));
  if (endpoint.kind === 'stdio') {
    throw new IpcError(
      '`stdio` can only be listened on; connect with `exec:COMMAND` instead',
      'ENOTSUP',
    );
  }
  const native = nativeConnect(endpoint.kind);
  if (native) return native(endpoint, options);
  if (BRIDGEABLE.has(endpoint.kind)) return connectBridge(endpoint, options);
  throw unsupported(endpoint.kind);
}

/**
 * Connects to an IPC endpoint and resolves to a duplex byte stream with
 * half-close: `end()` ends the outgoing direction only, and the stream ends
 * when the server ends its side.
 *
 * Options: `signal`, `deadline` and `timeout` (milliseconds) bound the
 * connect; `clink` is the bridge command (default `CLINK_BIN`, else `clink`).
 */
export async function connectStream(endpoint, options = {}) {
  const parsed = parseEndpoint(endpoint);
  const text = parsed.toString();
  const scope = scopeOf(options);
  const early = scopeError(scope, 'not-sent');
  if (early) throw early;
  trace(`connect ${text}`);
  const controller = new AbortController();
  let stop = () => {};
  const ended = new Promise((_resolve, reject) => {
    stop = watchScope(scope, (errorOf) => {
      const error = errorOf('not-sent');
      controller.abort(error);
      reject(error);
    });
  });
  ended.catch(() => {});
  let opening;
  try {
    opening = Promise.resolve().then(() => open(parsed, { ...options, signal: controller.signal }));
    const stream = await Promise.race([opening, ended]);
    trace(`connected ${text}`);
    return stream;
  } catch (error) {
    if (controller.signal.aborted) opening?.then(discard, () => {});
    trace(`connect ${text} failed: ${error.message}`);
    throw error;
  } finally {
    stop();
  }
}

// A TCP `{ host, port }`, as TcpTransport.connect takes, becomes `tcp:HOST:PORT`.
function fromAddress(endpoint) {
  if (typeof endpoint !== 'object' || endpoint === null || endpoint instanceof Endpoint) {
    return endpoint;
  }
  const host = endpoint.host ?? '127.0.0.1';
  return `tcp:${net.isIPv6(host) ? `[${host}]` : host}:${Number(endpoint.port)}`;
}

/**
 * The links protocol over an IPC stream: a {@link TcpTransport} (pipelining,
 * pushes, limits) whose capabilities name the endpoint kind.
 */
export class IpcTransport extends TcpTransport {
  #stream;
  #onEnd;

  /** Wraps a connected duplex stream; `owned: false` leaves it open on close. */
  constructor(stream, options = {}) {
    const { kind = 'ipc', endpoint, owned = true } = options;
    super(stream, {
      ...options,
      capabilities: { transport: kind, rawTcp: kind === 'tcp', ...options.capabilities },
    });
    this.kind = kind;
    this.endpoint = endpoint;
    this.#stream = stream;
    // The streams allow half-open connections: the server ending its side
    // ends this one too (or, for a stream left open, just this transport).
    this.#onEnd = () => {
      if (owned) stream.end();
      else this.onClose();
    };
    stream.on('end', this.#onEnd);
  }

  /**
   * Connects to `endpoint` (also a TCP `{ host, port }`), honouring `signal`,
   * `deadline` and `timeout`.
   */
  static async connect(endpoint, options = {}) {
    const parsed = parseEndpoint(fromAddress(endpoint));
    let stream;
    try {
      stream = await connectStream(parsed, options);
    } catch (error) {
      if (error instanceof LinksError) throw error;
      throw new ConnectionClosedError(`cannot connect to ${parsed}: ${error.message}`, {
        outcome: 'not-sent',
        cause: error,
      });
    }
    return new IpcTransport(stream, {
      ...options,
      owned: true,
      kind: parsed.kind,
      endpoint: parsed.toString(),
    });
  }

  async close() {
    try {
      await super.close();
    } finally {
      this.#stream.off('end', this.#onEnd);
    }
  }

  destroy() {
    this.#stream.off('end', this.#onEnd);
    super.destroy();
  }
}

/**
 * Connects a {@link LinksClient} to a `clink --serve ENDPOINT` server over
 * any IPC endpoint (a bare `host:port` is TCP).
 *
 * Options: those of `connectTcp` (`signal`, `deadline`, `timeout`, `verify`,
 * limits, `capabilities`) and `clink`, the bridge command.
 */
export async function connectIpc(endpoint, options = {}) {
  const transport = await IpcTransport.connect(endpoint, options);
  if (options.verify !== false) {
    try {
      await verifyProtocol(transport, scopeOf(options));
    } catch (error) {
      transport.destroy();
      await transport.close();
      throw error;
    }
  }
  return new LinksClient(transport);
}
