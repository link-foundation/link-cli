// Tests of src/protocol-errors.js. The texts come from rust/src/protocol/error.rs
// (the thiserror `#[error(...)]` of every `ProtocolError` variant, with
// `Io(#[from] io::Error)` as its source), the `io::ErrorKind` names of the
// Rust standard library, and rust/src/diagnostics.rs (one best-effort line on
// standard error or standard output, a failed write ignored).
import { describe, test } from 'node:test';
import assert from 'node:assert/strict';
import { constants } from 'node:os';

import { LinkError } from '../src/errors.js';
import {
  IoError,
  ProtocolError,
  alternateText,
  diagnostics,
  ioErrorKind,
  protocolError,
} from '../src/protocol-errors.js';

/** A Node.js system error, as `net` raises it. */
function systemError(code, message) {
  const error = new Error(`${message ?? code}`);
  error.code = code;
  error.errno = -constants.errno[code];
  error.syscall = 'connect';
  return error;
}

describe('IoError and ioErrorKind', () => {
  test('IoError keeps its kind, message and cause', () => {
    const cause = new Error('inner');
    const error = new IoError('InvalidData', 'frame too long', { cause });
    assert.ok(error instanceof Error);
    assert.equal(error.name, 'IoError');
    assert.equal(error.kind, 'InvalidData');
    assert.equal(error.message, 'frame too long');
    assert.equal(error.cause, cause);
    assert.equal(ioErrorKind(error), 'InvalidData');
    assert.equal(new IoError('Other', 'x').cause, undefined);
  });

  test('Node.js codes map to io::ErrorKind names', () => {
    const kinds = {
      EADDRINUSE: 'AddrInUse',
      EADDRNOTAVAIL: 'AddrNotAvailable',
      ECONNABORTED: 'ConnectionAborted',
      ECONNREFUSED: 'ConnectionRefused',
      ECONNRESET: 'ConnectionReset',
      EEXIST: 'AlreadyExists',
      EINTR: 'Interrupted',
      EINVAL: 'InvalidInput',
      ENOENT: 'NotFound',
      ENOTCONN: 'NotConnected',
      EPERM: 'PermissionDenied',
      EACCES: 'PermissionDenied',
      EPIPE: 'BrokenPipe',
      ETIMEDOUT: 'TimedOut',
      EAGAIN: 'WouldBlock',
    };
    for (const [code, kind] of Object.entries(kinds)) {
      assert.equal(ioErrorKind(systemError(code)), kind, code);
    }
  });

  test('anything else is Other', () => {
    assert.equal(ioErrorKind(systemError('EISDIR')), 'Other');
    assert.equal(ioErrorKind(new Error('plain')), 'Other');
    assert.equal(ioErrorKind({ code: 'toString' }), 'Other');
    assert.equal(ioErrorKind(null), 'Other');
    assert.equal(ioErrorKind(undefined), 'Other');
    assert.equal(ioErrorKind('text'), 'Other');
  });
});

describe('ProtocolError', () => {
  test('every variant prints its thiserror prefix', () => {
    const cases = [
      ['malformed', 'malformed message: '],
      ['invalidLino', 'invalid LiNo: '],
      ['limitExceeded', 'limit exceeded: '],
      ['unencodable', 'cannot encode: '],
      ['remote', 'server error: '],
    ];
    for (const [kind, prefix] of cases) {
      const error = ProtocolError[kind]('detail text');
      assert.ok(error instanceof ProtocolError);
      assert.ok(error instanceof LinkError);
      assert.equal(error.name, 'ProtocolError');
      assert.equal(error.kind, kind);
      assert.equal(error.detail, 'detail text');
      assert.equal(error.message, `${prefix}detail text`);
      assert.equal(error.cause, undefined);
    }
  });

  test('io prints the io::Error text and keeps it as its source', () => {
    const refused = systemError('ECONNREFUSED', 'connect ECONNREFUSED 127.0.0.1:1');
    const error = ProtocolError.io(refused);
    assert.equal(error.kind, 'io');
    const text = `Connection refused (os error ${constants.errno.ECONNREFUSED})`;
    assert.equal(error.detail, text);
    assert.equal(error.message, `I/O error: ${text}`);
    assert.equal(error.cause, refused);

    const custom = ProtocolError.io(new IoError('UnexpectedEof', 'failed to fill whole buffer'));
    assert.equal(custom.message, 'I/O error: failed to fill whole buffer');
    assert.equal(ProtocolError.io('not an error').message, 'I/O error: not an error');
  });

  test('the constructor takes a kind, a detail and options', () => {
    const cause = new Error('why');
    const error = new ProtocolError('remote', 'no such link', { cause });
    assert.equal(error.message, 'server error: no such link');
    assert.equal(error.cause, cause);
  });
});

describe('protocolError and alternateText', () => {
  test('protocolError passes a ProtocolError through and wraps the rest as Io', () => {
    const malformed = ProtocolError.malformed('bad frame');
    assert.equal(protocolError(malformed), malformed);
    const reset = systemError('ECONNRESET');
    const wrapped = protocolError(reset);
    assert.equal(wrapped.kind, 'io');
    assert.equal(
      wrapped.message,
      `I/O error: Connection reset by peer (os error ${constants.errno.ECONNRESET})`,
    );
    assert.equal(wrapped.cause, reset);
    assert.equal(protocolError(new LinkError('plain')).message, 'I/O error: plain');
  });

  test("alternateText is anyhow's {:#}: the chain joined with ': '", () => {
    assert.equal(alternateText(ProtocolError.remote('boom')), 'server error: boom');
    const broken = new IoError('BrokenPipe', 'pipe closed');
    assert.equal(alternateText(ProtocolError.io(broken)), 'I/O error: pipe closed: pipe closed');
    const outer = new LinkError('Failed to connect', { cause: ProtocolError.io(broken) });
    assert.equal(alternateText(outer), 'Failed to connect: I/O error: pipe closed: pipe closed');
    assert.equal(
      alternateText(systemError('ENOENT')),
      `No such file or directory (os error ${constants.errno.ENOENT})`,
    );
  });
});

describe('diagnostics', () => {
  /** Runs `body` with the writes of both process streams captured. */
  function capture(body, { failing = false } = {}) {
    const written = [];
    const original = { stdout: process.stdout.write, stderr: process.stderr.write };
    for (const name of ['stdout', 'stderr']) {
      process[name].write = (text) => {
        if (failing) throw new Error(`${name} closed`);
        written.push([name, text]);
        return true;
      };
    }
    try {
      body();
    } finally {
      process.stdout.write = original.stdout;
      process.stderr.write = original.stderr;
    }
    return written;
  }

  test('stderr and stdout write one line each to their stream', () => {
    const written = capture(() => {
      diagnostics.stderr('to error');
      diagnostics.stdout('to output');
      diagnostics.write('stdout', 'raw');
      diagnostics.write('anything', 'else');
    });
    assert.deepEqual(written, [
      ['stderr', 'to error\n'],
      ['stdout', 'to output\n'],
      ['stdout', 'raw'],
      ['stderr', 'else'],
    ]);
  });

  test('a failed write is ignored', () => {
    const written = capture(
      () => {
        assert.equal(diagnostics.stderr('lost'), undefined);
        assert.equal(diagnostics.stdout('lost'), undefined);
      },
      { failing: true },
    );
    assert.deepEqual(written, []);
  });
});
