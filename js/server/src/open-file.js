// Opening a regular file the way every platform agrees on.
//
// On Linux and macOS `open(2)` refuses a directory for writing (`EISDIR`),
// and the Rust standard library on Windows opens without
// `FILE_FLAG_BACKUP_SEMANTICS`, so `CreateFileW` refuses a directory there
// too. libuv on Windows always passes that flag, so Node.js, Bun and Deno open
// a directory for `r+`, `a+` or `O_RDWR | O_CREAT` and fail only at the first
// read or write -- or never. `openFileSync` checks what it opened and fails as
// `open(2)` does, so a directory in place of a log or a lock file is an
// `I/O error: Is a directory` everywhere (the POSIX wording that the README
// promises on every platform).
import { closeSync, fstatSync, openSync } from 'node:fs';
import { constants } from 'node:os';

/** The system error `open(2)` gives for a directory, shaped as `node:fs` throws it. */
export function isDirectoryError(path, errnos = constants.errno) {
  return Object.assign(new Error(`EISDIR: illegal operation on a directory, open '${path}'`), {
    code: 'EISDIR',
    errno: -errnos.EISDIR,
    syscall: 'open',
    path,
  });
}

/** `openSync(path, flags, mode)` that never hands back a directory. */
export function openFileSync(path, flags, mode, fs = { openSync, fstatSync, closeSync }) {
  const fd = fs.openSync(path, flags, mode);
  let directory;
  try {
    directory = fs.fstatSync(fd).isDirectory();
  } catch (error) {
    fs.closeSync(fd);
    throw error;
  }
  if (directory) {
    fs.closeSync(fd);
    throw isDirectoryError(path);
  }
  return fd;
}
