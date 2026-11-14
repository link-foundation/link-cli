// How the Rust `clink` reports a failure: `main` returns an `anyhow::Error`,
// so the standard library prints `Error: {error:?}` on standard error and
// exits with status 1. anyhow's `Debug` writes the error, then every cause
// under `Caused by:` -- indented by four spaces, or numbered `    0: ` when
// there is more than one -- (anyhow 1.0, src/fmt.rs).
//
// A JavaScript error chains its causes through `cause`, as `with_context`
// chains them in Rust. Node.js system errors (`code`, `errno`, `syscall`)
// print as Rust's `std::io::Error` does: the C library's text and the
// platform error number, `No such file or directory (os error 2)`.
import { constants } from 'node:os';

// `strerror` of the GNU C library for the errors file and socket calls meet.
const STRERROR = {
  E2BIG: 'Argument list too long',
  EACCES: 'Permission denied',
  EADDRINUSE: 'Address already in use',
  EADDRNOTAVAIL: 'Cannot assign requested address',
  EAFNOSUPPORT: 'Address family not supported by protocol',
  EAGAIN: 'Resource temporarily unavailable',
  EBADF: 'Bad file descriptor',
  EBUSY: 'Device or resource busy',
  ECONNABORTED: 'Software caused connection abort',
  ECONNREFUSED: 'Connection refused',
  ECONNRESET: 'Connection reset by peer',
  EEXIST: 'File exists',
  EFBIG: 'File too large',
  EHOSTUNREACH: 'No route to host',
  EINTR: 'Interrupted system call',
  EINVAL: 'Invalid argument',
  EIO: 'Input/output error',
  EISDIR: 'Is a directory',
  ELOOP: 'Too many levels of symbolic links',
  EMFILE: 'Too many open files',
  ENAMETOOLONG: 'File name too long',
  ENETUNREACH: 'Network is unreachable',
  ENOENT: 'No such file or directory',
  ENOMEM: 'Cannot allocate memory',
  ENOSPC: 'No space left on device',
  ENOTCONN: 'Transport endpoint is not connected',
  ENOTDIR: 'Not a directory',
  ENOTEMPTY: 'Directory not empty',
  ENOTSOCK: 'Socket operation on non-socket',
  ENXIO: 'No such device or address',
  EPERM: 'Operation not permitted',
  EPIPE: 'Broken pipe',
  EROFS: 'Read-only file system',
  ETIMEDOUT: 'Connection timed out',
  EXDEV: 'Invalid cross-device link',
};

/** Whether `error` is a Node.js system error (it has an `E…` code and an errno). */
export function isSystemError(error) {
  return (
    error instanceof Error &&
    typeof error.code === 'string' &&
    /^E[A-Z0-9]+$/.test(error.code) &&
    typeof error.errno === 'number'
  );
}

/**
 * The `Display` text of the `std::io::Error` Rust would hold for a Node.js
 * system error: `{strerror} (os error {errno})`. Unknown codes keep the
 * Node.js text after its `CODE: ` prefix.
 */
export function ioErrorText(error, errnos = constants.errno) {
  const number = errnos[error.code] ?? Math.abs(error.errno);
  const description =
    STRERROR[error.code] ??
    error.message.replace(/^[A-Z0-9]+: /, '').replace(/, [a-z]+(?: '.*')?$/, '');
  return `${description} (os error ${number})`;
}

/** The text one error of a chain prints. */
export function errorText(error) {
  if (isSystemError(error)) return ioErrorText(error);
  if (error instanceof Error) return error.message;
  return String(error);
}

/** The errors of a chain: `error`, its `cause`, the cause's `cause`, … */
export function errorChain(error) {
  const chain = [];
  const seen = new Set();
  for (let current = error; current != null && !seen.has(current); current = current.cause) {
    seen.add(current);
    chain.push(current);
  }
  return chain;
}

function indented(text, number) {
  return text
    .split('\n')
    .map((line, index) => {
      if (index > 0) return `${number == null ? '    ' : '       '}${line}`;
      return `${number == null ? '    ' : `${String(number).padStart(5)}: `}${line}`;
    })
    .join('\n');
}

/** anyhow's `{:?}` of `error`: the message, then `Caused by:` and every cause. */
export function formatErrorReport(error) {
  const [head, ...causes] = errorChain(error);
  let report = errorText(head);
  if (causes.length > 0) {
    report += '\n\nCaused by:';
    const multiple = causes.length > 1;
    causes.forEach((cause, number) => {
      report += `\n${indented(errorText(cause), multiple ? number : null)}`;
    });
  }
  return report;
}

/** What the Rust `main` prints when it returns `Err`: `Error: …` and a newline. */
export function reportError(error, err = process.stderr) {
  err.write(`Error: ${formatErrorReport(error)}\n`);
}
