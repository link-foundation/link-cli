// The CLI adapter: starts a `clink --serve` process (Rust or C#) and connects
// to it. The executable and its arguments are separate argv entries, never a
// shell line, so paths with spaces or quotes need no escaping. Closing stops
// only the process this adapter started.

import { spawn } from 'node:child_process';
import { scopeError, scopeOf, watchScope } from './deadline.js';
import { ConnectionClosedError, LinksError } from './errors.js';
import { LinksClient } from './client.js';
import { connectTcp, parseAddress } from './tcp.js';

const BANNER = 'clink server listening on ';
const TAIL_BYTES = 8192;

/**
 * A `clink --serve` child process. `address` is where it listens.
 */
export class ServerProcess {
  #child;
  #exited;
  #stderr = '';

  constructor(child, address, exited, stderr) {
    this.#child = child;
    this.address = address;
    this.#exited = exited;
    this.#stderr = stderr;
    child.stderr?.on('data', (chunk) => {
      this.#stderr = (this.#stderr + chunk).slice(-TAIL_BYTES);
    });
  }

  get pid() {
    return this.#child.pid;
  }

  /** The last few kilobytes the server wrote to stderr. */
  get stderr() {
    return this.#stderr;
  }

  /** Resolves to `{ code, signal }` once the process exited. */
  get exited() {
    return this.#exited;
  }

  /**
   * Stops the process: SIGINT (the C# server shuts down cleanly; the Rust
   * server saved every write already), then SIGKILL after `timeout` ms.
   */
  async close({ timeout = 5000 } = {}) {
    if (this.#child.exitCode === null && this.#child.signalCode === null) {
      this.#child.kill('SIGINT');
      const timer = setTimeout(() => this.#child.kill('SIGKILL'), timeout);
      try {
        return await this.#exited;
      } finally {
        clearTimeout(timer);
      }
    }
    return this.#exited;
  }

  async [Symbol.asyncDispose]() {
    await this.close();
  }
}

/**
 * Starts `command [...args] --db <db> --serve <host:port>` and resolves once
 * it listens.
 *
 * Options: `command` (path to `clink`, or e.g. `dotnet` with `args:
 * ['clink.dll']`), `args`, `db` (required), `host` (default 127.0.0.1),
 * `port` (default 0: any free port), `env`, `cwd`, `signal`, `deadline`,
 * `timeout` (default 30 s: the C# server compiles on first start).
 */
export async function startServer({ command, args = [], db, host = '127.0.0.1', port = 0, env, cwd, signal, deadline, timeout = 30000 } = {}) {
  if (typeof command !== 'string' || command === '') throw new TypeError('command is the path of the clink executable');
  if (typeof db !== 'string' || db === '') throw new TypeError('db is the path of the links database file');
  if (!Array.isArray(args) || args.some((arg) => typeof arg !== 'string')) throw new TypeError('args is an array of strings');
  const scope = scopeOf({ signal, deadline, timeout });
  const early = scopeError(scope, 'not-sent');
  if (early) throw early;
  const listen = host.includes(':') ? `[${host}]:${port}` : `${host}:${port}`;
  const child = spawn(command, [...args, '--db', db, '--serve', listen], {
    cwd,
    env: env ?? process.env,
    shell: false,
    windowsHide: true,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let stderr = '';
  const onStderr = (chunk) => {
    stderr = (stderr + chunk).slice(-TAIL_BYTES);
  };
  child.stderr.setEncoding('utf8');
  child.stderr.on('data', onStderr);
  const exited = new Promise((resolve) => child.once('exit', (code, signal) => resolve({ code, signal })));
  let address;
  try {
    address = await new Promise((resolve, reject) => {
      let stdout = '';
      const finish = (error, value) => {
        stop();
        child.stdout.off('data', onStdout);
        child.off('error', onError);
        if (error) reject(error);
        else resolve(value);
      };
      const onStdout = (chunk) => {
        const lines = (stdout + chunk).split('\n');
        stdout = lines.pop().slice(-TAIL_BYTES);
        const line = lines.find((candidate) => candidate.startsWith(BANNER));
        if (line !== undefined) finish(null, line.slice(BANNER.length).trim());
      };
      const onError = (error) => finish(new ConnectionClosedError(`cannot start ${command}: ${error.message}`, { outcome: 'not-sent', cause: error }));
      const stop = watchScope(scope, (errorOf) => finish(errorOf('not-sent')));
      child.stdout.setEncoding('utf8');
      child.stdout.on('data', onStdout);
      child.once('error', onError);
      exited.then(({ code, signal: exitSignal }) =>
        finish(new ConnectionClosedError(`${command} exited (${exitSignal ?? `code ${code}`}) before listening${stderr ? `: ${stderr.trim()}` : ''}`, { outcome: 'not-sent' })),
      );
    });
    // The banner belongs to an untrusted child. Validate it while startup
    // still owns failure cleanup, so a malformed address cannot leak a process.
    parseAddress(address);
  } catch (error) {
    if (child.exitCode === null && child.signalCode === null && child.pid !== undefined) {
      child.kill('SIGKILL');
      await exited;
    }
    throw error;
  }
  // Keep draining stdout so a chatty server never blocks on a full pipe.
  child.stdout.resume();
  child.stderr.off('data', onStderr);
  return new ServerProcess(child, address, exited, stderr);
}

/**
 * Starts a server process and connects a {@link LinksClient} to it; closing
 * the client closes the connection, then stops the process.
 */
export async function connectProcess(options = {}) {
  const server = await startServer(options);
  let transport;
  try {
    const client = await connectTcp(server.address, options);
    transport = client.transport;
  } catch (error) {
    await server.close();
    throw error;
  }
  return new LinksClient(new ProcessTransport(transport, server));
}

/** A TCP transport that also owns the server process behind it. */
export class ProcessTransport {
  constructor(transport, server) {
    this.transport = transport;
    this.server = server;
    this.capabilities = transport.capabilities;
    this.stats = transport.stats;
  }

  send(text, scope) {
    return this.transport.send(text, scope);
  }

  async close() {
    let failure = null;
    try {
      await this.transport.close();
    } catch (error) {
      failure = error;
    }
    await this.server.close();
    if (failure) throw failure instanceof LinksError ? failure : new ConnectionClosedError(failure.message, { cause: failure });
  }
}
