// Advisory file locking for multi-process access to a links database
// (rust/src/storage/lock.rs): `LockMode`, `lockFilePath` and `FileLock`.
//
// The lock is taken on a dedicated sidecar `<database>.lock` file opened
// create + read + write without truncation, as the Rust std `File::lock`,
// `lock_shared` and `try_lock` do: `flock(2)` on Unix, `LockFileEx` on Windows.
// Locks belong to an open file description, so two `FileLock`s in one process
// contend exactly as two processes do -- and as a Rust process does.
//
// Node.js has no `flock` in `node:fs`, so the primitive is picked per runtime
// when a lock is taken:
// - Deno: `Deno.FsFile` `lockSync` / `tryLockSync` / `unlockSync`, the same
//   Rust std calls (so Windows works too);
// - Bun: `flock(2)` of the C library through `bun:ffi`;
// - Node.js >= 26: `flock(2)` through `node:ffi`, which prints Node's
//   "FFI is an experimental feature" warning once per process.
// Anything else -- an older Node.js, Bun or Node.js on Windows, an unknown
// platform -- fails with a `LockError` naming what is missing; it never
// pretends to hold a lock.
//
// Rust releases the lock when the guard is dropped; here `release()` (or
// `using`, through `Symbol.dispose`) does. The operating system still drops
// it when the process exits, so a crashed holder never leaves it locked.
import { closeSync, constants as fsConstants, existsSync, mkdirSync, openSync } from 'node:fs';
import { constants as osConstants } from 'node:os';
import { dirname } from 'node:path';

import { errorText } from './error-report.js';
import { LockError } from './errors.js';
import { ioError } from './transactions-storage.js';

/** Requested sharing mode for a `FileLock`. */
export const LockMode = Object.freeze({
  /** Multiple readers may hold the lock at the same time. */
  Shared: 'Shared',
  /** Only one holder at a time; excludes all shared holders. */
  Exclusive: 'Exclusive',
});

function isExclusive(mode) {
  if (mode === LockMode.Exclusive) return true;
  if (mode === LockMode.Shared) return false;
  throw new TypeError(`unknown lock mode: ${String(mode)}`);
}

/** The trailing `/` and `/.` that `Path::components` ignores, removed; a root stays. */
function trimTrailing(path, separator) {
  let end = path.length;
  for (;;) {
    if (end > 1 && separator.test(path[end - 1])) end--;
    else if (end >= 2 && path[end - 1] === '.' && separator.test(path[end - 2])) end--;
    else return path.slice(0, end);
  }
}

/**
 * Conventional sidecar lock filename for a links database: `Path::file_name`
 * with `.lock` appended, joined to `Path::parent` unless that is empty or none.
 */
export function lockFilePath(databaseFilename, platform = process.platform) {
  const windows = platform === 'win32';
  const separator = windows ? /[\\/]/ : /\//;
  const trimmed = trimTrailing(String(databaseFilename), separator);
  let cut = trimmed.length - 1;
  while (cut >= 0 && !separator.test(trimmed[cut])) cut--;
  const last = trimmed.slice(cut + 1);
  // `.`, `..` and a root have no file name; a root has no parent either.
  const name = `${last === '.' || last === '..' ? '' : last}.lock`;
  if (cut < 0 || last === '') return name;
  const parent = trimTrailing(trimmed.slice(0, cut + 1), separator);
  if (separator.test(parent[parent.length - 1])) return `${parent}${name}`;
  return `${parent}${windows ? '\\' : '/'}${name}`;
}

// The C library errors `flock(2)` reports, beyond those error-report.js knows.
const FLOCK_STRERROR = {
  ENOLCK: 'No locks available',
  EOPNOTSUPP: 'Operation not supported',
  ENOTSUP: 'Operation not supported',
};

const LOCK_SH = 1;
const LOCK_EX = 2;
const LOCK_NB = 4;
const LOCK_UN = 8;

/** The system error `errno` stands for, shaped as `node:fs` throws one. */
export function errnoError(errno, errnos = osConstants.errno) {
  const code = Object.keys(errnos).find((name) => errnos[name] === errno);
  if (code === undefined) {
    return Object.assign(new Error(`Unknown error ${errno} (os error ${errno})`), { errno });
  }
  const message = FLOCK_STRERROR[code] ?? `Unknown error ${errno}`;
  return Object.assign(new Error(message), { code, errno: -errno });
}

/** `flock(2)` over a C library binding `{ flock(fd, operation), errno() }`. */
export function flockBackend(primitive, libc, errnos = osConstants.errno) {
  const wouldBlock = errnos.EWOULDBLOCK ?? errnos.EAGAIN;
  const call = (fd, operation) => {
    if (libc.flock(fd, operation) === 0) return true;
    const errno = libc.errno();
    if (operation & LOCK_NB && errno === wouldBlock) return false;
    throw errnoError(errno, errnos);
  };
  return {
    primitive,
    open: (path) => openSync(path, fsConstants.O_RDWR | fsConstants.O_CREAT, 0o666),
    lock: (fd, exclusive) => {
      call(fd, exclusive ? LOCK_EX : LOCK_SH);
    },
    tryLock: (fd, exclusive) => call(fd, (exclusive ? LOCK_EX : LOCK_SH) | LOCK_NB),
    unlock: (fd) => libc.flock(fd, LOCK_UN),
    close: (fd) => closeSync(fd),
  };
}

/** `Deno.FsFile` locking: the Rust std `File::lock` family itself. */
export function denoBackend(deno) {
  return {
    primitive: 'Deno.FsFile.lockSync',
    open: (path) => deno.openSync(path, { read: true, write: true, create: true }),
    lock: (file, exclusive) => file.lockSync(exclusive),
    tryLock: (file, exclusive) => file.tryLockSync(exclusive),
    unlock: (file) => file.unlockSync(),
    close: (file) => file.close(),
  };
}

// The C library and its `errno` accessor per platform.
const LIBC = {
  linux: { names: ['libc.so.6', 'libc.so'], errno: '__errno_location' },
  darwin: { names: ['libSystem.B.dylib'], errno: '__error' },
  freebsd: { names: ['libc.so.7'], errno: '__error' },
};

// How each foreign function interface declares and calls `flock` and `errno`.
const FFI = {
  'node:ffi': (ffi, name, errno) => {
    const { functions } = ffi.dlopen(name, {
      flock: { arguments: ['int32', 'int32'], return: 'int32' },
      [errno]: { arguments: [], return: 'pointer' },
    });
    return { flock: functions.flock, errno: () => ffi.getInt32(functions[errno]()) };
  },
  'bun:ffi': (ffi, name, errno) => {
    const { i32, ptr } = ffi.FFIType;
    const { symbols } = ffi.dlopen(name, {
      flock: { args: [i32, i32], returns: i32 },
      [errno]: { args: [], returns: ptr },
    });
    return { flock: symbols.flock, errno: () => ffi.read.i32(symbols[errno](), 0) };
  },
};

// Loaded C libraries: ffi module -> platform -> backend or unavailable reason.
const loaded = new WeakMap();

function libcBackend(ffi, id, runtime, platform) {
  const libc = LIBC[platform];
  if (libc === undefined) {
    return { unavailable: `${runtime} on ${platform} has no known C library for flock(2)` };
  }
  let byPlatform = loaded.get(ffi);
  if (byPlatform === undefined) loaded.set(ffi, (byPlatform = new Map()));
  if (!byPlatform.has(platform)) {
    let result;
    const failures = [];
    for (const name of libc.names) {
      try {
        result = flockBackend(`flock(2) through ${id}`, FFI[id](ffi, name, libc.errno));
        break;
      } catch (error) {
        failures.push(error.message);
      }
    }
    result ??= {
      unavailable: `${runtime} cannot load the C library through ${id} (${failures.join('; ')})`,
    };
    byPlatform.set(platform, result);
  }
  return byPlatform.get(platform);
}

/**
 * The advisory lock primitive of the running runtime: a backend
 * `{ primitive, open, lock, tryLock, unlock, close }`, or `{ unavailable }`
 * with the reason there is none. The arguments stand in for the runtime's
 * globals.
 */
export function detectLockBackend({
  deno = globalThis.Deno,
  versions = process.versions,
  platform = process.platform,
  builtin = (id) => process.getBuiltinModule?.(id),
} = {}) {
  if (deno) {
    return typeof deno.FsFile?.prototype?.lockSync === 'function'
      ? denoBackend(deno)
      : { unavailable: `Deno ${versions.deno} has no Deno.FsFile.lockSync` };
  }
  const bun = versions.bun !== undefined;
  const runtime = bun ? `Bun ${versions.bun}` : `Node.js ${versions.node}`;
  const id = bun ? 'bun:ffi' : 'node:ffi';
  if (platform === 'win32') {
    return {
      unavailable: `${runtime} on Windows has no LockFileEx binding in this port; use Deno or the Rust clink`,
    };
  }
  let ffi;
  try {
    ffi = builtin(id);
  } catch {
    ffi = undefined;
  }
  if (ffi === undefined) {
    return {
      unavailable: `${runtime} has no ${id} to call flock(2); use Node.js >= 26, Bun or Deno`,
    };
  }
  return libcBackend(ffi, id, runtime, platform);
}

let override = null;

/**
 * Replaces the detected primitive with `backend` (`null` restores detection)
 * and returns the previous replacement. Meant for tests.
 */
export function setLockBackend(backend) {
  const previous = override;
  override = backend;
  return previous;
}

/** The primitive locks are taken with now. */
export const lockBackend = () => override ?? detectLockBackend();

/** A guard around an advisory lock on a sidecar lock file. */
export class FileLock {
  constructor(backend, handle, path, mode) {
    this.backend = backend;
    this.handle = handle;
    this.lockPath = path;
    this.lockMode = mode;
    this.released = false;
  }

  /** Acquires the lock, blocking until it becomes available. */
  static acquire(lockPath, mode) {
    const exclusive = isExclusive(mode);
    const [backend, handle, path] = FileLock.open(lockPath);
    try {
      backend.lock(handle, exclusive);
    } catch (error) {
      backend.close(handle);
      throw lockFailure(path, error);
    }
    return new FileLock(backend, handle, path, mode);
  }

  /**
   * Tries to acquire the lock, returning `null` when another holder
   * currently owns a conflicting lock.
   */
  static tryAcquire(lockPath, mode) {
    const exclusive = isExclusive(mode);
    const [backend, handle, path] = FileLock.open(lockPath);
    let locked;
    try {
      locked = backend.tryLock(handle, exclusive);
    } catch (error) {
      backend.close(handle);
      throw lockFailure(path, error);
    }
    if (locked) return new FileLock(backend, handle, path, mode);
    backend.close(handle);
    return null;
  }

  static open(lockPath) {
    const path = String(lockPath);
    const backend = lockBackend();
    if (backend.unavailable !== undefined) throw lockFailure(path, backend.unavailable);
    try {
      const parent = dirname(path);
      if (!existsSync(parent)) mkdirSync(parent, { recursive: true });
      return [backend, backend.open(path), path];
    } catch (error) {
      throw ioError(error);
    }
  }

  /** The sidecar lock file this guard holds. */
  path() {
    return this.lockPath;
  }

  /** The sharing mode this guard was acquired with. */
  mode() {
    return this.lockMode;
  }

  /** Whether `release` has run. */
  isReleased() {
    return this.released;
  }

  /** Unlocks and closes the lock file, as dropping the Rust guard does. */
  release() {
    if (this.released) return;
    this.released = true;
    try {
      this.backend.unlock(this.handle);
    } catch {
      // `let _ = self.file.unlock();`
    }
    try {
      this.backend.close(this.handle);
    } catch {
      // Dropping a `File` ignores a failed close.
    }
  }

  [Symbol.dispose]() {
    this.release();
  }
}

/** `LinkError::Lock("failed to lock {path}: {error}")`. */
function lockFailure(path, error) {
  const text = typeof error === 'string' ? error : errorText(error);
  return new LockError(`failed to lock ${path}: ${text}`, {
    cause: typeof error === 'string' ? undefined : error,
  });
}
