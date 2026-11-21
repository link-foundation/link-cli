// Ports rust/src/protocol/error.rs (`ProtocolError`), the custom
// `std::io::Error`s of rust/src/protocol/ipc (`IoError`, with the
// `io::ErrorKind` names) and rust/src/diagnostics.rs (best-effort lines on
// standard error and standard output).
//
// A Rust `ProtocolError::Io` prints `I/O error: {io error}` and keeps the I/O
// error as its source, so anyhow reports it under `Caused by:`; the JavaScript
// error keeps it as `cause` (see error-report.js). A Node.js system error
// prints as Rust's `io::Error` text, `Connection refused (os error 111)`.
import { errorChain, errorText } from './error-report.js';
import { LinkError } from './errors.js';

/** A custom `std::io::Error`: an `io::ErrorKind` name and its message. */
export class IoError extends Error {
  constructor(kind, message, options) {
    super(message, options);
    this.name = 'IoError';
    this.kind = kind;
  }
}

/** The `io::ErrorKind` of any error: `IoError.kind`, or a Node.js error code. */
export function ioErrorKind(error) {
  if (error instanceof IoError) return error.kind;
  const code = error?.code;
  return Object.hasOwn(SYSTEM_KINDS, code) ? SYSTEM_KINDS[code] : 'Other';
}

const SYSTEM_KINDS = {
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

const PREFIXES = {
  io: 'I/O error: ',
  malformed: 'malformed message: ',
  invalidLino: 'invalid LiNo: ',
  limitExceeded: 'limit exceeded: ',
  unencodable: 'cannot encode: ',
  remote: 'server error: ',
};

/** `ProtocolError`: one of the kinds above and its detail text. */
export class ProtocolError extends LinkError {
  constructor(kind, detail, options) {
    super(`${PREFIXES[kind]}${detail}`, options);
    this.kind = kind;
    this.detail = detail;
  }

  /** `ProtocolError::Io(error)`: prints the I/O error and keeps it as the cause. */
  static io(error) {
    return new ProtocolError('io', errorText(error), { cause: error });
  }

  static malformed(detail) {
    return new ProtocolError('malformed', detail);
  }

  static invalidLino(detail) {
    return new ProtocolError('invalidLino', detail);
  }

  static limitExceeded(detail) {
    return new ProtocolError('limitExceeded', detail);
  }

  static unencodable(detail) {
    return new ProtocolError('unencodable', detail);
  }

  static remote(detail) {
    return new ProtocolError('remote', detail);
  }
}

/** Converts any error into a `ProtocolError`; others become `Io`. */
export function protocolError(error) {
  return error instanceof ProtocolError ? error : ProtocolError.io(error);
}

/** anyhow's `{:#}`: every error of the chain, joined with `: `. */
export function alternateText(error) {
  return errorChain(error).map(errorText).join(': ');
}

/**
 * The best-effort writers of rust/src/diagnostics.rs: one line each, to
 * standard error or standard output. Tests replace `write`.
 */
export const diagnostics = {
  write(stream, text) {
    try {
      (stream === 'stdout' ? process.stdout : process.stderr).write(text);
    } catch {
      // Diagnostics never fail the operation they describe.
    }
  },
  stderr(line) {
    diagnostics.write('stderr', `${line}\n`);
  },
  stdout(line) {
    diagnostics.write('stdout', `${line}\n`);
  },
};
