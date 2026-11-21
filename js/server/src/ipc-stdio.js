// `stdio` and `exec:COMMAND`, the anonymous-pipe transports of
// rust/src/protocol/ipc/stdio.rs (`StdioTransport`, `StdioListener`,
// `ExecTransport`, `connect_exec`) and `close_stdout` of
// rust/src/protocol/ipc/unix/mod.rs, adapted from js/client/src/process.js.
//
// Rust's `ThreadedReader` is a `ReadQueue` (ipc-stream.js) over the Node.js
// stream. Rust closes standard output by duplicating the null device onto
// descriptor 1; Node.js has no `dup2`, so `closeStdout` closes descriptor 1
// and opens the null device, which takes the lowest free descriptor: 1.
// Rust's `shutdown` of `exec` blocks until the child exits (at most 5 s, then
// kills it); here `shutdown` stays synchronous and the reaping runs on, its
// outcome traced as in Rust.
import { spawn } from 'node:child_process';
import * as fs from 'node:fs';
import { constants, devNull } from 'node:os';
import { errorText } from './error-report.js';
import { IoError, ioErrorKind } from './protocol-errors.js';
import { trace } from './ipc-trace.js';
import { IpcStream, ReadQueue } from './ipc-stream.js';
import { commandLine } from './ipc-endpoint.js';
import { outgoingClosed, Signal } from './ipc-util.js';

/** `close_stdout`: points descriptor 1 at the null device. */
export function closeStdout(api = fs) {
  api.closeSync(1);
  const fd = api.openSync(devNull, 'w');
  if (fd !== 1) api.closeSync(fd);
}

/** Writes `buffer` to a Node.js `Writable`; resolves to its length. */
const writeTo = (writable, buffer) =>
  new Promise((resolve, reject) => {
    writable.write(buffer, (error) => (error ? reject(error) : resolve(buffer.length)));
  });

/** `StdioTransport`: standard input and output of this process. */
class StdioTransport {
  kind = 'stdio';
  #input;
  #output;
  #closeOutput;
  #closed = false;

  constructor({ stdin, stdout, closeOutput }, ended) {
    this.#input = new ReadQueue(stdin, { keepError: true });
    this.#output = stdout;
    this.#closeOutput = closeOutput;
    this.ended = ended;
    stdout.on('error', () => {});
  }

  read(max, timeoutMs) {
    return this.#input.read(max, timeoutMs);
  }

  write(buffer) {
    if (this.#output === null) {
      return Promise.reject(new IoError('BrokenPipe', 'standard output was closed'));
    }
    return writeTo(this.#output, buffer);
  }

  async closeWrite() {
    const output = this.#output;
    if (output === null) return;
    this.#output = null;
    await new Promise((resolve) => output.write('', resolve));
    this.#closeOutput(fs);
  }

  shutdown() {
    if (this.#closed) return;
    this.#closed = true;
    trace('stdio connection ended');
    this.closeWrite().catch(() => {});
    this.#input.discard();
    // A discarding reader must not keep the process alive.
    this.#input.readable.unref?.();
    this.ended();
  }
}

/**
 * `StdioListener`: the first `accept()` returns the connection over standard
 * input and output; later ones wait until it ends or the listener closes.
 * `io`: `{stdin, stdout, closeOutput}`, this process's by default.
 */
export class StdioListener {
  #io;
  #taken = false;
  #ended = false;
  #closed = false;
  #changed = new Signal();

  constructor(io = {}) {
    this.#io = {
      stdin: io.stdin ?? process.stdin,
      stdout: io.stdout ?? process.stdout,
      closeOutput: io.closeOutput ?? closeStdout,
    };
  }

  async accept() {
    if (this.#closed) return null;
    if (!this.#taken) {
      this.#taken = true;
      const transport = new StdioTransport(this.#io, () => {
        this.#ended = true;
        this.#changed.notify();
      });
      trace('stdio connection started');
      return new IpcStream(transport);
    }
    while (!this.#ended && !this.#closed) await this.#changed.wait();
    return null;
  }

  endpoint() {
    return 'stdio';
  }

  endpoints() {
    return [this.endpoint()];
  }

  localAddr() {
    return null;
  }

  close() {
    this.#closed = true;
    this.#changed.notify();
  }
}

/** `listen_stdio`. */
export const listenStdio = async (_endpoint, io = {}) => new StdioListener(io);

/**
 * `ExitStatus`'s `Display`: `exit status: N` or `signal: N (NAME)` on Unix,
 * `exit code: N` on Windows.
 */
export function exitStatus(code, signal, platform = process.platform) {
  if (platform === 'win32') return `exit code: ${code}`;
  if (signal !== null) return `signal: ${constants.signals[signal]} (${signal})`;
  return `exit status: ${code}`;
}

const KILL_AFTER = 5000;

/** `ExecTransport`: a child's standard input and output. */
class ExecTransport {
  kind = 'exec';
  #child;
  #input;
  #output;
  #closed = false;

  constructor(child, program, { killAfter = KILL_AFTER, platform = process.platform } = {}) {
    this.#child = child;
    this.#input = child.stdin;
    this.#output = new ReadQueue(child.stdout);
    this.program = program;
    this.killAfter = killAfter;
    this.platform = platform;
    child.stdin.on('error', () => {});
    this.exited = new Promise((resolve) => {
      if (child.exitCode !== null || child.signalCode !== null) resolve();
      else child.once('exit', () => resolve());
    });
  }

  read(max, timeoutMs) {
    return this.#output.read(max, timeoutMs);
  }

  write(buffer) {
    if (this.#input === null) return Promise.reject(outgoingClosed());
    return writeTo(this.#input, buffer);
  }

  async closeWrite() {
    this.#input?.end();
    this.#input = null;
  }

  shutdown() {
    if (this.#closed) return;
    this.#closed = true;
    this.closeWrite();
    this.#output.discard();
    const child = this.#child;
    let timer;
    const killed = new Promise((resolve) => {
      timer = setTimeout(() => resolve(true), this.killAfter);
    });
    this.reaped = Promise.race([this.exited.then(() => false), killed]).then((kill) => {
      clearTimeout(timer);
      if (kill) {
        trace(`exec \`${this.program}\` killed after 5 s`);
        child.kill('SIGKILL');
        return;
      }
      const status = exitStatus(child.exitCode, child.signalCode, this.platform);
      trace(`exec \`${this.program}\` exited: ${status}`);
    });
  }
}

/** Resolves once `child` started; a failed start rejects with its error. */
const started = (child) =>
  new Promise((resolve, reject) => {
    child.once('spawn', resolve);
    child.once('error', reject);
  });

/** `connect_exec`: runs the command with its standard input and output piped. */
export async function connectExec(endpoint, options = {}) {
  const argv = commandLine(endpoint.address);
  const child = (options.spawn ?? spawn)(argv[0], argv.slice(1), {
    stdio: ['pipe', 'pipe', 'inherit'],
  });
  try {
    await started(child);
  } catch (error) {
    throw new IoError(ioErrorKind(error), `cannot start \`${argv[0]}\`: ${errorText(error)}`);
  }
  child.on('error', () => {});
  trace(`exec started \`${argv[0]}\` as process ${child.pid}`);
  return new IpcStream(new ExecTransport(child, argv[0], options));
}
