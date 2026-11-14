// Unit tests of the small IPC modules: the `[ipc]` trace switch, the shared
// helpers of rust/src/protocol/ipc/mod.rs (ipc-util.js), the flat JSON of
// rust/src/protocol/ipc/json.rs (its `round_trips_and_skips_other_members`
// test and every error) and the endpoints of
// rust/src/protocol/ipc/endpoint.rs.
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { after, describe, it } from 'node:test';
import { resetTrace, setTrace, trace, traceEnabled } from '../src/ipc-trace.js';
import {
  brokenPipe,
  ByteQueue,
  hungUp,
  outgoingClosed,
  timedOut,
  unsupported,
  noNodeApi,
  PeerWatch,
  processAlive,
  randomId,
  randomToken,
  Signal,
  sleep,
  systemError,
} from '../src/ipc-util.js';
import { JsonObject, parseStrings, readObject } from '../src/ipc-json.js';
import {
  asciiLower,
  commandLine,
  Endpoint,
  optionsOf,
  register,
  registered,
  trimRust,
  unregisterAll,
} from '../src/ipc-endpoint.js';
import { errorText } from '../src/error-report.js';
import { diagnostics } from '../src/protocol-errors.js';

const directory = mkdtempSync(join(tmpdir(), 'clink-ipc-basics-'));
after(() => {
  unregisterAll();
  resetTrace();
  rmSync(directory, { recursive: true, force: true });
});

describe('ipc trace', () => {
  it('follows CLINK_IPC_TRACE until switched', () => {
    const lines = [];
    const original = diagnostics.stderr;
    const saved = process.env.CLINK_IPC_TRACE;
    diagnostics.stderr = (line) => lines.push(line);
    try {
      process.env.CLINK_IPC_TRACE = '1';
      resetTrace();
      assert.equal(traceEnabled(), true);
      trace('one');
      setTrace(false);
      trace('two');
      assert.equal(traceEnabled(), false);
      delete process.env.CLINK_IPC_TRACE;
      resetTrace();
      assert.equal(traceEnabled(), false);
      setTrace(1);
      trace('three');
    } finally {
      diagnostics.stderr = original;
      if (saved === undefined) delete process.env.CLINK_IPC_TRACE;
      else process.env.CLINK_IPC_TRACE = saved;
      resetTrace();
    }
    assert.deepEqual(lines, ['[ipc] one', '[ipc] three']);
  });
});

describe('ipc helpers', () => {
  it('builds system errors that print as io::Error', () => {
    const error = systemError('EEXIST', 'mkfifo', { EEXIST: 17 });
    assert.equal(error.code, 'EEXIST');
    assert.equal(error.errno, -17);
    assert.equal(error.syscall, 'mkfifo');
    assert.equal(
      errorText(systemError('ENOENT', 'mkfifo')),
      'No such file or directory (os error 2)',
    );
    assert.equal(
      noNodeApi('mq', 'POSIX message queues').message,
      '`mq:` endpoints are not supported by the JavaScript server: Node.js has no API for POSIX message queues',
    );
  });

  it('makes the errors of Rust', () => {
    const errors = [hungUp(), brokenPipe(), timedOut(), outgoingClosed(), unsupported('mq', 'x')];
    assert.deepEqual(
      errors.map((error) => [error.kind, error.message]),
      [
        ['ConnectionReset', 'the peer hung up'],
        ['BrokenPipe', 'the peer no longer reads'],
        ['TimedOut', 'timed out'],
        ['BrokenPipe', 'the outgoing direction was closed'],
        ['Unsupported', 'x on `mq:` endpoints is not supported on this platform'],
      ],
    );
  });

  it('signals wake waiters and time out', async () => {
    const signal = new Signal();
    let woken = 0;
    const forever = signal.wait().then(() => woken++);
    const bounded = signal.wait(1000).then(() => woken++);
    signal.notify();
    await Promise.all([forever, bounded]);
    assert.equal(woken, 2);
    const started = performance.now();
    await signal.wait(-5);
    await signal.wait(20);
    assert.ok(performance.now() - started >= 15);
    signal.notify();
  });

  it('queues bytes', () => {
    const queue = new ByteQueue();
    queue.push(Buffer.alloc(0));
    assert.equal(queue.length, 0);
    queue.push(Buffer.from('abc'));
    queue.push(Buffer.from('defg'));
    assert.equal(queue.take(2).toString(), 'ab');
    assert.equal(queue.take(3).toString(), 'cde');
    assert.equal(queue.length, 2);
    assert.equal(queue.take(10).toString(), 'fg');
    assert.equal(queue.take(10).length, 0);
    queue.push(Buffer.from('x'));
    queue.clear();
    assert.equal(queue.length, 0);
    assert.equal(queue.take(1).length, 0);
  });

  it('makes random names and tokens', () => {
    assert.match(randomId(), /^[0-9a-f]{20}$/);
    assert.equal(randomToken().length, 16);
  });

  it('tells living processes', () => {
    assert.equal(processAlive(process.pid), true);
    for (const pid of [0, -1, 1.5, 0x80000000, '1']) assert.equal(processAlive(pid), false);
    const fail = (code) => () => {
      throw Object.assign(new Error(code), { code });
    };
    assert.equal(processAlive(7, fail('EPERM')), true);
    assert.equal(processAlive(7, fail('ESRCH')), false);
  });

  it('watches a peer at most every 100 ms', async () => {
    let calls = 0;
    const watch = new PeerWatch(7, () => {
      calls += 1;
      return false;
    });
    assert.equal(watch.alive(), true);
    assert.equal(calls, 0);
    await sleep(110);
    assert.equal(watch.alive(), false);
    assert.equal(watch.alive(), false);
    assert.equal(calls, 1);
    assert.equal(new PeerWatch(process.pid).pid, process.pid);
  });
});

describe('ipc json', () => {
  it('round trips and skips other members', () => {
    const object = new JsonObject()
      .with('kind', 'shm')
      .with('pid', 42)
      .with('path', 'C:\\dir "x"\n\r\t\u0001é');
    const json = object.toJson();
    assert.equal(json, '{"kind":"shm","path":"C:\\\\dir \\"x\\"\\n\\r\\t\\u0001é","pid":42}');
    assert.deepEqual(JsonObject.parse(json).entries(), object.entries());
    const other = JsonObject.parse(
      ' {"v":1, "x":[1,{"a":null}], "b":true, "f":-1.5e3, "s":"\\u00e9\\ud83d\\ude00"} ',
    );
    assert.equal(other.number('v'), 1n);
    assert.equal(other.text('s'), 'é😀');
    assert.equal(other.number('f'), undefined);
    assert.equal(other.text('v'), undefined);
    assert.equal(other.requireNumber('v'), 1n);
    assert.throws(() => other.requireNumber('s'), {
      kind: 'InvalidData',
      message: 'the handshake lacks the number `s`',
    });
    assert.deepEqual(parseStrings(' ["clink", "--serve", "a b"] '), ['clink', '--serve', 'a b']);
    assert.deepEqual(parseStrings('[ ]'), []);
  });

  it('skips every other kind of value', () => {
    const parsed = JsonObject.parse(
      '{"a":1.5,"b":2e3,"c":3E1,"d":18446744073709551616,"e":18446744073709551615,' +
        '"f":false,"g":null,"h":[],"i":{},"j":{"k":1,"l":[2,3]},"\\"\\\\\\/\\b\\f\\n\\r\\t":"x"}',
    );
    assert.deepEqual(parsed.entries(), [
      ['"\\/\b\f\n\r\t', 'x'],
      ['e', 18446744073709551615n],
    ]);
    assert.equal(JsonObject.parse('{ }').entries().length, 0);
  });

  it('decodes \\u escapes as Rust does', () => {
    const text = (escape) => JsonObject.parse(`{"s":"${escape}"}`).text('s');
    assert.equal(text('\\ud800'), '\ufffd');
    assert.equal(text('\\udc00'), '\ufffd');
    assert.equal(text('\\ud800\\u0041'), String.fromCodePoint(0x10041));
    assert.equal(text('\\u+041'), 'A');
    assert.equal(text('\\u0041'), 'A');
  });

  it('reports malformed JSON', () => {
    const cases = [
      ['{"a":1} x', 'trailing characters after the JSON object'],
      ['[', 'expected `{` at offset 0 of the JSON object'],
      ['{"a" 1}', 'expected `:` at offset 5 of the JSON object'],
      ['{"a":1 "b":2}', 'expected `,` or `}` in the JSON object'],
      ['{"a":{"b":1 x}}', 'expected `,` or `}` in a JSON object'],
      ['{"a":[1 2]}', 'expected `,` or `]` in a JSON array'],
      ['{"a":x}', 'unexpected JSON at offset 5'],
      ['{"a":tru}', 'unexpected JSON at offset 5'],
      ['{"a":-', 'expected `,` or `}` in the JSON object'],
      ['{"a', 'unended JSON string'],
      ['{"a\\', 'unended escape'],
      ['{"\\x":1}', 'unknown JSON escape'],
      ['{"\\u12":1}', 'bad `\\u` escape'],
      ['{"\\uzzzz":1}', 'bad `\\u` escape'],
      [`{"a":${'['.repeat(40)}}`, 'the JSON object nests too deeply'],
    ];
    for (const [text, message] of cases) {
      assert.throws(() => JsonObject.parse(text), { kind: 'InvalidData', message }, text);
    }
    const strings = [
      ['["a" "b"]', 'expected `,` or `]` in a JSON array'],
      ['[] x', 'trailing characters after the JSON array'],
      ['[1]', 'expected `"` at offset 1 of the JSON object'],
      ['x', 'expected `[` at offset 0 of the JSON object'],
    ];
    for (const [text, message] of strings) {
      assert.throws(() => parseStrings(text), { message }, text);
    }
  });

  it('reads JSON files that are UTF-8', () => {
    const good = join(directory, 'good.json');
    writeFileSync(good, '{"pid":7}');
    assert.equal(readObject(good).number('pid'), 7n);
    const bad = join(directory, 'bad.json');
    writeFileSync(bad, Buffer.of(0x7b, 0xff, 0x7d));
    assert.throws(() => readObject(bad), {
      kind: 'InvalidData',
      message: 'stream did not contain valid UTF-8',
    });
  });
});

describe('ipc endpoints', () => {
  it('lists the options of every kind', () => {
    const common = ['timeout'];
    const rendezvous = [...common, 'poll', 'accept-poll'];
    const ring = [...rendezvous, 'capacity'];
    const cases = [
      ['tcp', common],
      ['unix', common],
      ['stdio', common],
      ['exec', common],
      ['fifo', rendezvous],
      ['shm', ring],
      ['mmap', ring],
      ['file', ring],
      ['signal', [...ring, 'signo']],
      ['udp', [...common, 'chunk', 'rto', 'heartbeat', 'dead']],
      ['mq', [...rendezvous, 'chunk', 'rto', 'maxmsg']],
      ['alpc', [...rendezvous, 'chunk', 'rto']],
    ];
    for (const [kind, options] of cases) assert.deepEqual(optionsOf(kind, 'linux'), options, kind);
    assert.deepEqual(optionsOf('pipe', 'win32'), common);
    assert.deepEqual(optionsOf('pipe', 'linux'), rendezvous);
    assert.deepEqual(optionsOf('pipe'), process.platform === 'win32' ? common : rendezvous);
  });

  it('trims and lowercases as Rust', () => {
    assert.equal(trimRust('\u0085 a b\u3000\t'), 'a b');
    assert.equal(trimRust('\ufeffa'), '\ufeffa');
    assert.equal(asciiLower('TCP-É'), 'tcp-É');
  });

  it('parses endpoints', () => {
    const parsed = Endpoint.parse('  MMAP:/tmp/x?Capacity=128&&timeout=5  ');
    assert.equal(parsed.kind, 'mmap');
    assert.equal(parsed.address, '/tmp/x');
    assert.deepEqual(parsed.options, { capacity: '128', timeout: '5' });
    assert.equal(parsed.option('capacity'), '128');
    assert.equal(parsed.option('poll'), undefined);
    assert.equal(parsed.timeout(), 5);
    assert.equal(String(parsed), 'mmap:/tmp/x?capacity=128&timeout=5');
    assert.equal(Endpoint.parse(parsed), parsed);
    assert.equal(String(Endpoint.parse('127.0.0.1:80')), 'tcp:127.0.0.1:80');
    assert.equal(String(Endpoint.parse('stdio')), 'stdio');
    assert.equal(String(Endpoint.parse('localhost')), 'tcp:localhost');
    assert.equal(String(Endpoint.parse('stdio:')), 'stdio');
    assert.equal(Endpoint.parse('exec: sh -c "a?b=c"').address, ' sh -c "a?b=c"');
    assert.equal(Endpoint.parse('tcp:h:1?timeout=0').timeout(), 1);
    assert.equal(Endpoint.parse('tcp:h:1').timeout(), 10_000);
    assert.equal(new Endpoint('tcp', 'h:1', [['timeout', 'x']]).timeout(), 10_000);
    assert.equal(Endpoint.parse('pipe:x?poll=5', 'linux').number('poll', 0, 0, 10), 5);
    assert.equal(Endpoint.parse('udp:h:1').number('chunk', 99, 16, 65000), 99);
    assert.equal(Endpoint.parse('tcp:h:1?timeout=+7').millis('timeout', 0), 7);
  });

  it('refuses malformed endpoints', () => {
    const cases = [
      ['  ', 'an endpoint must not be empty'],
      ['exec:  ', '`exec:` needs a command'],
      ['tcp:h:1?timeout', 'endpoint option `timeout` needs a `=value`'],
      ['tcp:h:1?poll=1', '`tcp:` endpoints have no option `poll` (options: timeout)'],
      ['tcp:', '`tcp:` endpoints need an address'],
      ['udp', '`udp:` endpoints need an address'],
      ['stdio:x', '`stdio` takes no address'],
      [
        'tcp:h:1?timeout=x',
        'endpoint option `timeout` must be a whole number from 0 to 86400000, found `x`',
      ],
      [
        'mmap:x?capacity=8',
        'endpoint option `capacity` must be a whole number from 64 to 1073741824, found `8`',
      ],
      [
        'tcp:h:1?timeout=99999999999999999999',
        'endpoint option `timeout` must be a whole number from 0 to 86400000, found `99999999999999999999`',
      ],
    ];
    for (const [text, message] of cases) {
      assert.throws(() => Endpoint.parse(text), { kind: 'InvalidInput', message }, text);
    }
    assert.throws(() => Endpoint.parse('pipe:x?poll=1', 'win32'), {
      message: '`pipe:` endpoints have no option `poll` (options: timeout)',
    });
  });

  it('registers custom kinds', () => {
    assert.throws(() => register('a b'), {
      kind: 'InvalidInput',
      message: 'invalid endpoint kind `a b`',
    });
    assert.throws(() => register('TCP'), {
      kind: 'AlreadyExists',
      message: '`tcp` is a built-in endpoint kind',
    });
    const connect = () => null;
    register('My-Kind', connect, null);
    assert.deepEqual(registered('my-kind'), { connect, listen: undefined });
    register('other', null, connect);
    assert.deepEqual(registered('OTHER'), { connect: undefined, listen: connect });
    const custom = Endpoint.parse('MY-KIND:?anything=1');
    assert.equal(custom.kind, 'my-kind');
    assert.equal(custom.address, '');
    assert.equal(String(custom), 'my-kind?anything=1');
    assert.equal(Endpoint.parse('my-kind:addr').address, 'addr');
    unregisterAll();
    assert.equal(registered('my-kind'), undefined);
    assert.equal(Endpoint.parse('my-kind:1').kind, 'tcp');
  });

  it('splits command lines', () => {
    assert.deepEqual(commandLine(' cat  -u\u3000x '), ['cat', '-u', 'x']);
    assert.deepEqual(commandLine('["a b", "c"]'), ['a b', 'c']);
    for (const text of ['  ', '[]', '[""]']) {
      assert.throws(() => commandLine(text), { message: '`exec:` needs a command' }, text);
    }
  });
});
