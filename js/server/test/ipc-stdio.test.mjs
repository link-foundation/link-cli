// Unit tests of ipc-stdio.js, the anonymous-pipe transports of
// rust/src/protocol/ipc/stdio.rs and `close_stdout` of
// rust/src/protocol/ipc/unix/mod.rs: the `stdio` listener over stand-in
// streams, `ExitStatus`'s text, and `exec:` over a stand-in child process
// (writes after the end, reaping, the 5 s kill, a failed start).
import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { devNull } from 'node:os';
import { PassThrough, Writable } from 'node:stream';
import { describe, it } from 'node:test';
import { Endpoint } from '../src/ipc-endpoint.js';
import {
  closeStdout,
  connectExec,
  exitStatus,
  listenStdio,
  StdioListener,
} from '../src/ipc-stdio.js';
import { setTrace } from '../src/ipc-trace.js';
import { diagnostics } from '../src/protocol-errors.js';

/** Collects the `[ipc]` traces of `body`. */
async function traced(body) {
  const lines = [];
  const original = diagnostics.stderr;
  diagnostics.stderr = (line) => lines.push(line);
  setTrace(true);
  try {
    await body();
  } finally {
    setTrace(false);
    diagnostics.stderr = original;
  }
  return lines;
}

describe('close_stdout', () => {
  it('points descriptor 1 at the null device', () => {
    for (const opened of [1, 7]) {
      const calls = [];
      closeStdout({
        closeSync: (fd) => calls.push(['close', fd]),
        openSync: (file, flags) => {
          calls.push(['open', file, flags]);
          return opened;
        },
      });
      const expected = [
        ['close', 1],
        ['open', devNull, 'w'],
      ];
      if (opened !== 1) expected.push(['close', opened]);
      assert.deepEqual(calls, expected);
    }
  });
});

describe('stdio', () => {
  it('serves one connection, then waits for its end or the close', async () => {
    const stdin = new PassThrough();
    let unrefs = 0;
    stdin.unref = () => unrefs++;
    const stdout = new PassThrough();
    const closings = [];
    const listener = await listenStdio(Endpoint.parse('stdio'), {
      stdin,
      stdout,
      closeOutput: (api) => closings.push(typeof api.openSync),
    });
    assert.deepEqual([listener.endpoints(), listener.localAddr()], [['stdio'], null]);
    let stream;
    const lines = await traced(async () => {
      stream = await listener.accept();
    });
    assert.deepEqual(lines, ['[ipc] stdio connection started']);
    const waiting = listener.accept();
    await stream.write(Buffer.from('out'));
    assert.equal(stdout.read().toString(), 'out');
    stdin.write('in');
    assert.equal((await stream.read(2)).toString(), 'in');
    await stream.closeWrite();
    await stream.closeWrite();
    assert.deepEqual(closings, ['function']);
    await assert.rejects(stream.write(Buffer.from('x')), {
      kind: 'BrokenPipe',
      message: 'standard output was closed',
    });
    stdout.emit('error', new Error('ignored'));
    const ended = await traced(async () => {
      stream.shutdown();
      stream.shutdown();
      assert.equal(await waiting, null);
    });
    assert.deepEqual(ended, ['[ipc] stdio connection ended']);
    assert.equal(unrefs, 1);
    listener.close();
    assert.equal(await listener.accept(), null);
  });

  it('reports failed writes and ignores a failed close at the end', async () => {
    const stdout = new Writable({
      write: (_chunk, _encoding, callback) => callback(new Error('the reader went away')),
    });
    const listener = new StdioListener({
      stdin: new PassThrough(),
      stdout,
      closeOutput: () => {
        throw new Error('cannot close');
      },
    });
    const stream = await listener.accept();
    await assert.rejects(stream.write(Buffer.from('x')), { message: 'the reader went away' });
    stream.shutdown();
    assert.equal(await listener.accept(), null);
  });

  it('stops waiting when the listener closes', async () => {
    const listener = new StdioListener({ stdin: new PassThrough(), stdout: new PassThrough() });
    assert.ok(await listener.accept());
    const waiting = listener.accept();
    listener.close();
    assert.equal(await waiting, null);
    // This process's own streams are the default.
    assert.equal(new StdioListener().endpoint(), 'stdio');
  });
});

describe('ExitStatus', () => {
  it('reads as Rust prints it', () => {
    assert.equal(exitStatus(3, null, 'win32'), 'exit code: 3');
    assert.equal(exitStatus(null, 'SIGKILL', 'linux'), 'signal: 9 (SIGKILL)');
    assert.equal(exitStatus(0, null, 'linux'), 'exit status: 0');
  });
});

/** A `ChildProcess` stand-in: piped streams, a scripted start and exit. */
class FakeChild extends EventEmitter {
  pid = 4242;
  exitCode = null;
  signalCode = null;
  stdin = new PassThrough();
  stdout = new PassThrough();
  killed = [];

  kill(signal) {
    this.killed.push(signal);
    this.signalCode = signal;
    this.emit('exit', null, signal);
  }

  exit(code) {
    this.exitCode = code;
    this.emit('exit', code, null);
  }
}

/** A `spawn` that records its call and starts (or fails) `child`. */
function spawning(child, error = null) {
  const calls = [];
  const spawn = (...args) => {
    calls.push(args);
    setImmediate(() => (error === null ? child.emit('spawn') : child.emit('error', error)));
    return child;
  };
  return { spawn, calls };
}

describe('exec', { timeout: 30_000 }, () => {
  it('pipes a child and traces how it exited', async () => {
    const child = new FakeChild();
    const { spawn, calls } = spawning(child);
    let stream;
    const started = await traced(async () => {
      stream = await connectExec(Endpoint.parse('exec:["cat", "-u"]'), { spawn });
    });
    assert.deepEqual(calls, [['cat', ['-u'], { stdio: ['pipe', 'pipe', 'inherit'] }]]);
    assert.deepEqual(started, ['[ipc] exec started `cat` as process 4242']);
    await stream.write(Buffer.from('to child'));
    assert.equal(child.stdin.read().toString(), 'to child');
    child.stdout.write('from child');
    assert.equal((await stream.read(10)).toString(), 'from child');
    child.emit('error', new Error('ignored after the start'));
    child.stdin.emit('error', new Error('ignored'));
    await stream.closeWrite();
    await stream.closeWrite();
    await assert.rejects(stream.write(Buffer.from('x')), {
      kind: 'BrokenPipe',
      message: 'the outgoing direction was closed',
    });
    const reaped = await traced(async () => {
      stream.shutdown();
      stream.shutdown();
      child.exit(3);
      await stream.transport.reaped;
    });
    assert.deepEqual(reaped, [`[ipc] exec \`cat\` exited: ${exitStatus(3, null)}`]);
  });

  it('kills a child that does not exit in time', async () => {
    const child = new FakeChild();
    const { spawn } = spawning(child);
    const stream = await connectExec(Endpoint.parse('exec:sleep 60'), { spawn, killAfter: 10 });
    const lines = await traced(async () => {
      stream.shutdown();
      await stream.transport.reaped;
    });
    assert.deepEqual(lines, ['[ipc] exec `sleep` killed after 5 s']);
    assert.deepEqual(child.killed, ['SIGKILL']);
  });

  it('reaps a child that already exited', async () => {
    const child = new FakeChild();
    child.exitCode = 0;
    const { spawn } = spawning(child);
    const stream = await connectExec(Endpoint.parse('exec:true'), { spawn, platform: 'win32' });
    const lines = await traced(async () => {
      stream.shutdown();
      await stream.transport.reaped;
    });
    assert.deepEqual(lines, ['[ipc] exec `true` exited: exit code: 0']);
  });

  it('reports a child that cannot start', async () => {
    const failure = Object.assign(new Error('spawn nope ENOENT'), {
      code: 'ENOENT',
      errno: -2,
      syscall: 'spawn nope',
    });
    const { spawn } = spawning(new FakeChild(), failure);
    await assert.rejects(connectExec(Endpoint.parse('exec:nope'), { spawn }), (error) => {
      assert.equal(error.kind, 'NotFound');
      assert.match(error.message, /^cannot start `nope`: /);
      return true;
    });
    // Node.js's own `spawn` is the default.
    await assert.rejects(connectExec(Endpoint.parse('exec:clink-ipc-no-such-program')), {
      kind: 'NotFound',
      message: /^cannot start `clink-ipc-no-such-program`: /,
    });
  });
});
