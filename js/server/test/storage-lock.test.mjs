// The advisory sidecar lock of rust/src/storage/lock.rs: `lock_file_path`,
// `FileLock` over every runtime's primitive (an injected backend reaches the
// branches of the others), and real contention inside one process, across
// processes of the same runtime, with flock(1) and with Rust std `File::lock`.
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { constants } from 'node:os';
import { join } from 'node:path';
import { afterEach, describe, it } from 'node:test';
import { pathToFileURL } from 'node:url';

import { formatErrorReport } from '../src/error-report.js';
import { LinkError, LockError } from '../src/errors.js';
import {
  errorString,
  kernel32Backend,
  loadKernel32,
  maybeVerbatim,
  overlappedLayout,
  WindowsIoError,
} from '../src/storage-lock-windows.js';
import {
  denoBackend,
  detectLockBackend,
  errnoError,
  FileLock,
  flockBackend,
  lockBackend,
  lockFilePath,
  LockMode,
  setLockBackend,
} from '../src/storage-lock.js';
import { scratchDirectory } from './helpers/storage.mjs';

const REAL = lockBackend();
const real = { skip: REAL.unavailable };
const DENO = globalThis.Deno !== undefined;
const WINDOWS = process.platform === 'win32';
const { errno } = constants;

afterEach(() => setLockBackend(null));

/** A backend that records its calls; `tryLock` answers `free`. */
function recorder({ free = true, lockError, tryError, unlockError, closeError } = {}) {
  const calls = [];
  const fail = (error) => {
    if (error) throw error;
  };
  return {
    calls,
    primitive: 'recorder',
    open: (path) => {
      calls.push(['open', path]);
      return 7;
    },
    lock: (handle, exclusive) => {
      calls.push(['lock', handle, exclusive]);
      fail(lockError);
    },
    tryLock: (handle, exclusive) => {
      calls.push(['tryLock', handle, exclusive]);
      fail(tryError);
      return free;
    },
    unlock: (handle) => {
      calls.push(['unlock', handle]);
      fail(unlockError);
    },
    close: (handle) => {
      calls.push(['close', handle]);
      fail(closeError);
    },
  };
}

describe('lockFilePath', () => {
  // The output of rust/src/storage/lock.rs `lock_file_path` on Linux.
  const RUST = {
    'db.links': 'db.links.lock',
    'dir/db.links': 'dir/db.links.lock',
    './db.links': './db.links.lock',
    '/db.links': '/db.links.lock',
    '/': '.lock',
    '': '.lock',
    '.': '.lock',
    '..': '.lock',
    'dir/': 'dir.lock',
    'dir//': 'dir.lock',
    'dir/.': 'dir.lock',
    'dir/./': 'dir.lock',
    'dir/..': 'dir/.lock',
    'a/./b': 'a/b.lock',
    'a//b': 'a/b.lock',
    '//db': '/db.lock',
    '/./db': '/db.lock',
    'a/b/': 'a/b.lock',
    '../x': '../x.lock',
    './': '.lock',
    '/.': '.lock',
    'a/b/.': 'a/b.lock',
    'a/.//.': 'a.lock',
    'C:\\x\\db': 'C:\\x\\db.lock',
  };

  it('appends .lock to the file name as Rust does', () => {
    for (const [path, expected] of Object.entries(RUST)) {
      assert.equal(lockFilePath(path, 'linux'), expected, path);
    }
  });

  it('separates components with either slash on Windows', () => {
    assert.equal(lockFilePath('dir\\db.links', 'win32'), 'dir\\db.links.lock');
    assert.equal(lockFilePath('dir/db.links', 'win32'), 'dir\\db.links.lock');
    assert.equal(lockFilePath('dir\\sub\\', 'win32'), 'dir\\sub.lock');
    assert.equal(lockFilePath('\\db', 'win32'), '\\db.lock');
  });

  it('uses the running platform by default', () => {
    assert.equal(lockFilePath('db.links'), 'db.links.lock');
    assert.equal(lockFilePath(join('dir', 'db.links')), join('dir', 'db.links.lock'));
  });
});

describe('FileLock over an injected backend', () => {
  it('names the modes as the Rust enum does and rejects others', () => {
    assert.deepEqual(Object.values(LockMode), ['Shared', 'Exclusive']);
    assert.ok(Object.isFrozen(LockMode));
    setLockBackend(recorder());
    assert.throws(() => FileLock.acquire('x.lock', 'Both'), {
      name: 'TypeError',
      message: 'unknown lock mode: Both',
    });
  });

  it('acquires, reports its path and mode, and releases once', () => {
    const backend = recorder();
    assert.equal(setLockBackend(backend), null);
    const path = join(scratchDirectory(), 'nested', 'deeper', 'db.links.lock');
    const lock = FileLock.acquire(path, LockMode.Shared);
    assert.ok(existsSync(join(path, '..')), 'the parent directory is created');
    assert.equal(lock.path(), path);
    assert.equal(lock.mode(), LockMode.Shared);
    assert.equal(lock.isReleased(), false);
    lock.release();
    lock.release();
    lock[Symbol.dispose]();
    assert.equal(lock.isReleased(), true);
    assert.deepEqual(backend.calls, [
      ['open', path],
      ['lock', 7, false],
      ['unlock', 7],
      ['close', 7],
    ]);
    assert.equal(setLockBackend(null), backend);
  });

  it('tries without blocking: a lock when free, null when busy', () => {
    const free = recorder();
    setLockBackend(free);
    const lock = FileLock.tryAcquire('db.lock', LockMode.Exclusive);
    assert.equal(lock.mode(), LockMode.Exclusive);
    assert.deepEqual(free.calls, [
      ['open', 'db.lock'],
      ['tryLock', 7, true],
    ]);
    const busy = recorder({ free: false });
    setLockBackend(busy);
    assert.equal(FileLock.tryAcquire('db.lock', LockMode.Shared), null);
    assert.deepEqual(busy.calls.slice(1), [
      ['tryLock', 7, false],
      ['close', 7],
    ]);
  });

  it('reports a failed lock as LinkError::Lock and closes the file', () => {
    const lockError = errnoError(errno.EINTR);
    const backend = recorder({ lockError });
    setLockBackend(backend);
    assert.throws(
      () => FileLock.acquire('db.lock', LockMode.Exclusive),
      (error) => {
        assert.ok(error instanceof LockError);
        assert.equal(error.cause, lockError);
        assert.equal(
          error.message,
          `Lock error: failed to lock db.lock: Interrupted system call (os error ${errno.EINTR})`,
        );
        return true;
      },
    );
    assert.deepEqual(backend.calls.at(-1), ['close', 7]);
    const tried = recorder({ tryError: new Error('Bad resource ID') });
    setLockBackend(tried);
    assert.throws(() => FileLock.tryAcquire('db.lock', LockMode.Shared), {
      message: 'Lock error: failed to lock db.lock: Bad resource ID',
    });
    assert.deepEqual(tried.calls.at(-1), ['close', 7]);
  });

  it('reports a sidecar it cannot open as LinkError::Io', () => {
    const directory = scratchDirectory();
    setLockBackend(flockBackend('test', { flock: () => 0, errno: () => 0 }));
    assert.throws(
      () => FileLock.acquire(directory, LockMode.Exclusive),
      (error) => {
        assert.ok(error instanceof LinkError);
        assert.equal(
          formatErrorReport(error),
          `I/O error: Is a directory (os error ${errno.EISDIR})\n\nCaused by:\n    Is a directory (os error ${errno.EISDIR})`,
        );
        return true;
      },
    );
    const blocker = join(directory, 'file');
    writeFileSync(blocker, '');
    assert.throws(() => FileLock.tryAcquire(join(blocker, 'db.lock'), LockMode.Shared), {
      message: `I/O error: Not a directory (os error ${errno.ENOTDIR})`,
    });
    setLockBackend({ open: () => assert.fail('unused'), lock: () => {} });
    assert.throws(() => FileLock.acquire(join(directory, 'x', 'db.lock'), LockMode.Shared), {
      message: 'unused',
    });
  });

  it('never pretends to lock without a primitive', () => {
    const path = join(scratchDirectory(), 'missing', 'db.lock');
    setLockBackend({ unavailable: 'Node.js 20.0.0 has no node:ffi' });
    for (const take of [FileLock.acquire, FileLock.tryAcquire]) {
      assert.throws(() => take(path, LockMode.Exclusive), {
        name: 'LockError',
        message: `Lock error: failed to lock ${path}: Node.js 20.0.0 has no node:ffi`,
      });
    }
    assert.equal(existsSync(join(path, '..')), false);
  });

  it('ignores failures to unlock and close on release, as drop does', () => {
    const backend = recorder({ unlockError: new Error('unlock'), closeError: new Error('close') });
    setLockBackend(backend);
    const lock = FileLock.acquire('db.lock', LockMode.Exclusive);
    lock.release();
    assert.deepEqual(backend.calls.slice(2), [
      ['unlock', 7],
      ['close', 7],
    ]);
  });
});

describe('lock primitives', () => {
  it('names errno values as the Rust io::Error does', () => {
    const text = (error) => formatErrorReport(new LockError('x', { cause: error }));
    assert.equal(
      text(errnoError(errno.EWOULDBLOCK)),
      `Lock error: x\n\nCaused by:\n    Resource temporarily unavailable (os error ${errno.EWOULDBLOCK})`,
    );
    assert.equal(errnoError(errno.ENOLCK).message, 'No locks available');
    assert.equal(errnoError(errno.ENOLCK).code, 'ENOLCK');
    assert.equal(errnoError(errno.EDOM).message, `Unknown error ${errno.EDOM}`);
    const unknown = errnoError(99999);
    assert.equal(unknown.code, undefined);
    assert.equal(unknown.message, 'Unknown error 99999 (os error 99999)');
  });

  it('calls flock(2) with the LOCK_* operations', () => {
    const calls = [];
    let result = 0;
    let error = 0;
    const libc = {
      flock: (fd, operation) => {
        calls.push([fd, operation]);
        return result;
      },
      errno: () => error,
    };
    const backend = flockBackend('fake', libc, { EAGAIN: 11, EINTR: 4, EBADF: 9 });
    assert.equal(backend.primitive, 'fake');
    backend.lock(3, true);
    backend.lock(3, false);
    assert.equal(backend.tryLock(3, true), true);
    assert.equal(backend.tryLock(3, false), true);
    backend.unlock(3);
    assert.deepEqual(calls, [
      [3, 2],
      [3, 1],
      [3, 6],
      [3, 5],
      [3, 8],
    ]);
    result = -1;
    error = 11;
    assert.equal(backend.tryLock(3, true), false);
    assert.throws(() => backend.lock(3, true), { code: 'EAGAIN' });
    error = 9;
    assert.throws(() => backend.tryLock(3, false), { code: 'EBADF' });
    error = 4;
    assert.throws(() => backend.lock(3, false), { code: 'EINTR' });
    const path = join(scratchDirectory(), 'kept.lock');
    writeFileSync(path, 'kept');
    backend.close(backend.open(path));
    assert.equal(readFileSync(path, 'utf8'), 'kept', 'the sidecar is not truncated');
  });

  it('maps the Deno.FsFile locking calls', () => {
    const calls = [];
    const file = {
      lockSync: (exclusive) => calls.push(['lockSync', exclusive]),
      tryLockSync: (exclusive) => calls.push(['tryLockSync', exclusive]) > 0,
      unlockSync: () => calls.push(['unlockSync']),
      close: () => calls.push(['close']),
    };
    const deno = { openSync: (path, options) => calls.push(['openSync', path, options]) && file };
    const backend = denoBackend(deno);
    assert.equal(backend.primitive, 'Deno.FsFile.lockSync');
    const handle = backend.open('db.lock');
    backend.lock(handle, true);
    assert.equal(backend.tryLock(handle, false), true);
    backend.unlock(handle);
    backend.close(handle);
    assert.deepEqual(calls, [
      ['openSync', 'db.lock', { read: true, write: true, create: true }],
      ['lockSync', true],
      ['tryLockSync', false],
      ['unlockSync'],
      ['close'],
    ]);
  });

  it('picks Deno.FsFile under Deno and names an old Deno', () => {
    const FsFile = class {
      lockSync() {}
    };
    const versions = { deno: '2.9.7' };
    assert.equal(
      detectLockBackend({ deno: { FsFile }, versions }).primitive,
      'Deno.FsFile.lockSync',
    );
    assert.deepEqual(detectLockBackend({ deno: {}, versions }), {
      unavailable: 'Deno 2.9.7 has no Deno.FsFile.lockSync',
    });
  });

  it('names what Node.js and Bun lack', () => {
    const node = { node: '20.0.0' };
    const bun = { node: '24.3.0', bun: '1.4.4' };
    const none = () => undefined;
    assert.deepEqual(detectLockBackend({ deno: null, versions: node, builtin: none }), {
      unavailable:
        'Node.js 20.0.0 has no node:ffi to call flock(2); use Node.js >= 26, Bun or Deno',
    });
    const throwing = () => {
      throw new Error('ERR_ACCESS_DENIED');
    };
    assert.match(
      detectLockBackend({ deno: null, versions: bun, platform: 'linux', builtin: throwing })
        .unavailable,
      /^Bun 1\.4\.4 has no bun:ffi/,
    );
    const windows = { deno: null, platform: 'win32', builtin: none };
    assert.deepEqual(detectLockBackend({ ...windows, versions: { node: '22.0.0' } }), {
      unavailable:
        'Node.js 22.0.0 has no node:ffi to call LockFileEx; use Node.js >= 26, Bun or Deno',
    });
    assert.deepEqual(
      detectLockBackend({ deno: null, versions: node, platform: 'sunos', builtin: () => ({}) }),
      { unavailable: 'Node.js 20.0.0 on sunos has no known C library for flock(2)' },
    );
  });

  it('binds flock and errno through node:ffi, trying each C library once', () => {
    const opened = [];
    const ffi = {
      dlopen(name, definitions) {
        opened.push(name);
        if (name === 'libc.so.6') throw new Error('libc.so.6: cannot open shared object file');
        assert.deepEqual(definitions, {
          flock: { arguments: ['int32', 'int32'], return: 'int32' },
          __errno_location: { arguments: [], return: 'pointer' },
        });
        return { functions: { flock: () => -1, __errno_location: () => 1234n } };
      },
      getInt32: (pointer) => (pointer === 1234n ? errno.EWOULDBLOCK : 0),
    };
    const environment = {
      deno: null,
      versions: { node: '26.13.0' },
      platform: 'linux',
      builtin: (id) => (id === 'node:ffi' ? ffi : undefined),
    };
    const backend = detectLockBackend(environment);
    assert.equal(backend.primitive, 'flock(2) through node:ffi');
    assert.equal(backend.tryLock(3, true), false);
    assert.equal(detectLockBackend(environment), backend);
    assert.deepEqual(opened, ['libc.so.6', 'libc.so']);
  });

  it('binds flock and errno through bun:ffi on macOS', () => {
    const ffi = {
      FFIType: { i32: 5, ptr: 12 },
      dlopen(name, definitions) {
        assert.equal(name, 'libSystem.B.dylib');
        assert.deepEqual(definitions, {
          flock: { args: [5, 5], returns: 5 },
          __error: { args: [], returns: 12 },
        });
        return { symbols: { flock: () => -1, __error: () => 4321 } };
      },
      read: { i32: (pointer, offset) => (pointer === 4321 && offset === 0 ? errno.EBADF : 0) },
    };
    const backend = detectLockBackend({
      deno: null,
      versions: { node: '24.3.0', bun: '1.4.4' },
      platform: 'darwin',
      builtin: (id) => (id === 'bun:ffi' ? ffi : undefined),
    });
    assert.equal(backend.primitive, 'flock(2) through bun:ffi');
    assert.throws(() => backend.tryLock(3, true), { code: 'EBADF' });
  });

  it('names every C library it could not load', () => {
    const ffi = {
      dlopen(name) {
        throw new Error(`${name} missing`);
      },
    };
    const environment = {
      deno: null,
      versions: { node: '26.0.0' },
      platform: 'linux',
      builtin: () => ffi,
    };
    assert.deepEqual(detectLockBackend(environment), {
      unavailable:
        'Node.js 26.0.0 cannot load the C library through node:ffi (libc.so.6 missing; libc.so missing)',
    });
  });

  it('detects the running runtime by default', () => {
    const backend = detectLockBackend();
    const call = process.platform === 'win32' ? 'LockFileEx' : 'flock(2)';
    const expected = DENO
      ? 'Deno.FsFile.lockSync'
      : `${call} through ${process.versions.bun ? 'bun' : 'node'}:ffi`;
    assert.equal(backend.primitive ?? backend.unavailable, REAL.primitive ?? REAL.unavailable);
    if (backend.unavailable === undefined) assert.equal(backend.primitive, expected);
  });
});

/** UTF-16 with a NUL, as kernel32 receives it, back to a string. */
function fromWide(buffer) {
  assert.equal(buffer.readUInt16LE(buffer.length - 2), 0, 'NUL-terminated');
  return buffer.toString('utf16le', 0, buffer.length - 2);
}

const MESSAGES = {
  5: 'Access is denied.',
  6: 'The handle is invalid.',
  8: 'Not enough memory resources are available to process this command.',
  33: 'The process cannot access the file because another process has locked a portion of the file.',
};

/**
 * kernel32 as a model: handles name files, `LockFileEx` grants whole-file
 * locks that do not conflict, `FormatMessageW` knows `MESSAGES`. `overrides`
 * replace functions; each gets the model's own, with `fail(code, result)`
 * setting the last error, as its first argument.
 */
function fakeKernel32(pointerSize = 8, overrides = {}) {
  const calls = [];
  const held = [];
  const files = new Map();
  let lastError = 0;
  let next = 0x40;
  const handle = () => (pointerSize === 8 ? BigInt(next++) : next++);
  const fail = (code, result = 0) => {
    lastError = code;
    return result;
  };
  const base = {
    CreateFileW(name, ...rest) {
      calls.push(['CreateFileW', fromWide(name), ...rest]);
      const created = handle();
      files.set(created, fromWide(name));
      return created;
    },
    CloseHandle(closed) {
      calls.push(['CloseHandle', closed]);
      for (let index = held.length - 1; index >= 0; index--) {
        if (held[index].handle === closed) held.splice(index, 1);
      }
      files.delete(closed);
      return 1;
    },
    LockFileEx(locked, flags, ...rest) {
      calls.push(['LockFileEx', locked, flags, ...rest.slice(0, 3), Buffer.from(rest[3])]);
      const exclusive = (flags & 2) !== 0;
      const file = files.get(locked);
      const conflict = held.some(
        (lock) => lock.file === file && lock.handle !== locked && (exclusive || lock.exclusive),
      );
      if (conflict) return fail(33);
      held.push({ file, handle: locked, exclusive });
      return 1;
    },
    UnlockFile(unlocked, ...rest) {
      calls.push(['UnlockFile', unlocked, ...rest]);
      const index = held.findIndex((lock) => lock.handle === unlocked);
      if (index === -1) return fail(158);
      held.splice(index, 1);
      return 1;
    },
    CreateEventW(...parameters) {
      calls.push(['CreateEventW', ...parameters]);
      return handle();
    },
    GetOverlappedResult: () => assert.fail('unused'),
    GetLastError: () => lastError,
    GetFullPathNameW(name, size, buffer, part) {
      assert.equal(part, null);
      const absolute = `C:\\work\\${fromWide(name)}`;
      if (absolute.length >= size) return absolute.length + 1;
      buffer.write(`${absolute}\0`, 'utf16le');
      return absolute.length;
    },
    FormatMessageW(flags, source, code, language, buffer, size, parameters) {
      assert.deepEqual([flags, source, language, size, parameters], [0x1200, null, 0, 2048, null]);
      if (MESSAGES[code] === undefined) return fail(317);
      buffer.write(`${MESSAGES[code]}\r\n`, 'utf16le');
      return MESSAGES[code].length + 2;
    },
  };
  const model = { ...base, fail };
  const kernel32 = { calls, held };
  for (const [name, call] of Object.entries(model)) {
    kernel32[name] = overrides[name]
      ? (...parameters) => overrides[name](model, ...parameters)
      : call;
  }
  return kernel32;
}

const fakeBackend = (kernel32, pointerSize = 8) =>
  kernel32Backend('LockFileEx (fake)', kernel32, pointerSize);
const ALL = 0xffffffff;

describe('the kernel32 lock of Bun and Node.js on Windows', () => {
  it('lays OVERLAPPED out for 64-bit and 32-bit pointers', () => {
    assert.deepEqual(overlappedLayout(8), { size: 32, hEvent: 24 });
    assert.deepEqual(overlappedLayout(4), { size: 20, hEvent: 16 });
  });

  it('makes paths verbatim as Rust maybe_verbatim does', () => {
    const kernel32 = fakeKernel32();
    const verbatim = (path) => fromWide(maybeVerbatim(kernel32, path, () => new Error('x')));
    for (const path of ['', '\\\\?\\C:\\db.lock', '\\??\\C:\\db.lock', 'C:', 'C:\\db.lock']) {
      assert.equal(verbatim(path), path);
    }
    for (const path of ['D:/db.lock', '\\\\server\\share\\db.lock', '//server/share/db.lock']) {
      assert.equal(verbatim(path), path);
    }
    assert.equal(verbatim('db.lock'), '\\\\?\\C:\\work\\db.lock');
    assert.equal(verbatim('C:db.lock'), '\\\\?\\C:\\work\\C:db.lock');
    const long = `C:\\${'d\\'.repeat(300)}db.lock`;
    assert.equal(verbatim(long), `\\\\?\\C:\\work\\${long}`, 'grows the buffer past 512');
    assert.throws(() => verbatim('db\0.lock'), {
      name: 'WindowsIoError',
      message: 'strings passed to WinAPI cannot contain NULs',
    });
  });

  it('prefixes every kind of absolute path GetFullPathNameW makes', () => {
    let absolute;
    let sizes = [];
    const kernel32 = fakeKernel32(8, {
      GetFullPathNameW(model, name, size, buffer) {
        sizes.push(size);
        if (absolute === null) return 0;
        if (size === 512) return 512; // `k == n`: Rust doubles the buffer
        buffer.write(absolute, 'utf16le');
        return absolute.length;
      },
    });
    const verbatim = (path) => fromWide(maybeVerbatim(kernel32, path, () => new Error('failed')));
    const cases = {
      '\\\\.\\COM1': '\\\\?\\COM1',
      '\\\\?\\C:\\a': '\\\\?\\C:\\a',
      '\\??\\C:\\a': '\\??\\C:\\a',
      '\\\\server\\share\\a': '\\\\?\\UNC\\server\\share\\a',
      '\\a': '\\a',
    };
    for (const [full, expected] of Object.entries(cases)) {
      absolute = full;
      assert.equal(verbatim('relative'), expected);
    }
    assert.deepEqual(sizes.slice(0, 2), [512, 1024]);
    absolute = null;
    assert.throws(() => verbatim('relative'), { message: 'failed' });
  });

  it('formats Windows errors as the Rust std error_string does', () => {
    const kernel32 = fakeKernel32();
    assert.equal(errorString(kernel32, 5), 'Access is denied.');
    assert.equal(
      errorString(kernel32, 1234),
      'OS Error 1234 (FormatMessageW() returned error 317)',
    );
    const lone = fakeKernel32(8, {
      FormatMessageW: (model, flags, source, code, language, buffer) => {
        buffer.writeUInt16LE(0xd800, 0);
        return 1;
      },
    });
    assert.equal(errorString(lone, 5), 'OS Error 5 (FormatMessageW() returned invalid UTF-16)');
  });

  it('opens the sidecar with CreateFileW as OpenOptions does', () => {
    const kernel32 = fakeKernel32();
    const backend = fakeBackend(kernel32);
    assert.equal(backend.primitive, 'LockFileEx (fake)');
    assert.equal(backend.open('C:\\db.lock'), 0x40n);
    backend.close(0x40n);
    assert.deepEqual(kernel32.calls, [
      ['CreateFileW', 'C:\\db.lock', 0xc0000000, 7, null, 4, 0, null],
      ['CloseHandle', 0x40n],
    ]);
    const denied = fakeBackend(fakeKernel32(8, { CreateFileW: (model) => model.fail(5, -1n) }));
    assert.throws(
      () => denied.open('C:\\db.lock'),
      (error) => {
        assert.ok(error instanceof WindowsIoError);
        assert.equal(error.message, 'Access is denied. (os error 5)');
        assert.equal(error.rawOsError, 5);
        return true;
      },
    );
  });

  it('locks the whole file with LockFileEx as File::lock and try_lock do', () => {
    const kernel32 = fakeKernel32();
    const backend = fakeBackend(kernel32);
    const first = backend.open('C:\\db.lock');
    const second = backend.open('C:\\db.lock');
    backend.lock(first, true);
    const [, , , , event] = kernel32.calls.slice(2);
    assert.deepEqual(kernel32.calls.slice(2, 5), [
      ['CreateEventW', null, 0, 0, null],
      ['LockFileEx', first, 2, 0, ALL, ALL, kernel32.calls[3][6]],
      ['CloseHandle', 0x42n],
    ]);
    assert.equal(event, undefined);
    const overlapped = kernel32.calls[3][6];
    assert.equal(overlapped.length, 32);
    assert.equal(overlapped.readBigInt64LE(24), 0x42n, 'hEvent');
    assert.equal(overlapped.readBigInt64LE(0) | overlapped.readBigInt64LE(16), 0n);
    assert.equal(backend.tryLock(second, true), false);
    assert.equal(backend.tryLock(second, false), false);
    assert.deepEqual(
      kernel32.calls.slice(5).map(([name, , flags]) => [name, flags]),
      [
        ['LockFileEx', 3],
        ['LockFileEx', 1],
      ],
    );
    assert.deepEqual(kernel32.calls[5][6], Buffer.alloc(32), 'a zeroed OVERLAPPED');
    backend.unlock(first);
    assert.equal(backend.tryLock(second, false), true);
    backend.lock(first, false);
    assert.equal(kernel32.calls.at(-2)[2], 0, 'a shared lock has no flags');
  });

  it('unlocks twice, as a handle may hold an exclusive and a shared lock', () => {
    const kernel32 = fakeKernel32();
    const backend = fakeBackend(kernel32);
    const handle = backend.open('C:\\db.lock');
    backend.lock(handle, true);
    backend.lock(handle, false);
    backend.unlock(handle);
    assert.deepEqual(kernel32.held, []);
    backend.lock(handle, true);
    backend.unlock(handle);
    const unlocks = kernel32.calls.filter(([name]) => name === 'UnlockFile');
    assert.equal(unlocks.length, 4);
    assert.deepEqual(unlocks[0], ['UnlockFile', handle, 0, 0, ALL, ALL]);
    assert.throws(() => backend.unlock(handle), { message: /^OS Error 158 .* \(os error 158\)$/ });
    let calls = 0;
    const second = fakeBackend(
      fakeKernel32(8, { UnlockFile: (model) => (calls++ === 0 ? 1 : model.fail(6)) }),
    );
    assert.throws(() => second.unlock(1n), { message: 'The handle is invalid. (os error 6)' });
  });

  it('reports failed locks with GetLastError, closing the event', () => {
    const noEvent = fakeKernel32(8, { CreateEventW: (model) => model.fail(8, 0n) });
    assert.throws(() => fakeBackend(noEvent).lock(1n, true), {
      message: 'Not enough memory resources are available to process this command. (os error 8)',
    });
    const kernel32 = fakeKernel32(8, { LockFileEx: (model) => model.fail(6) });
    const backend = fakeBackend(kernel32);
    assert.throws(() => backend.lock(1n, true), { rawOsError: 6 });
    assert.deepEqual(kernel32.calls.at(-1), ['CloseHandle', 0x40n]);
    assert.throws(() => backend.tryLock(1n, true), {
      message: 'The handle is invalid. (os error 6)',
    });
  });

  it('waits for a lock LockFileEx completes asynchronously', () => {
    const results = [1, 0];
    const waits = [];
    const kernel32 = fakeKernel32(4, {
      LockFileEx: (model) => model.fail(997),
      GetOverlappedResult(model, handle, overlapped, transferred, wait) {
        waits.push([
          handle,
          overlapped.length,
          overlapped.readInt32LE(16),
          transferred.length,
          wait,
        ]);
        return results.shift() || model.fail(6);
      },
    });
    const backend = fakeBackend(kernel32, 4);
    backend.lock(9, true);
    assert.throws(() => backend.lock(9, false), { rawOsError: 6 });
    assert.deepEqual(waits, [
      [9, 20, 0x40, 4, 1],
      [9, 20, 0x41, 4, 1],
    ]);
    assert.deepEqual(
      kernel32.calls.filter(([name]) => name === 'CloseHandle'),
      [
        ['CloseHandle', 0x40],
        ['CloseHandle', 0x41],
      ],
    );
  });

  it('contends between two FileLocks and reports an unopenable sidecar as I/O', () => {
    const kernel32 = fakeKernel32();
    setLockBackend(fakeBackend(kernel32));
    const path = join(scratchDirectory(), 'db.links.lock');
    const first = FileLock.acquire(path, LockMode.Exclusive);
    assert.equal(FileLock.tryAcquire(path, LockMode.Shared), null);
    first.release();
    FileLock.tryAcquire(path, LockMode.Shared).release();
    assert.deepEqual(kernel32.held, []);
    setLockBackend(fakeBackend(fakeKernel32(8, { CreateFileW: (model) => model.fail(5, -1n) })));
    assert.throws(
      () => FileLock.acquire(path, LockMode.Exclusive),
      (error) => {
        assert.ok(error instanceof LinkError);
        assert.equal(
          formatErrorReport(error),
          'I/O error: Access is denied. (os error 5)\n\nCaused by:\n    Access is denied. (os error 5)',
        );
        return true;
      },
    );
    assert.throws(() => FileLock.acquire(`${path}\0`, LockMode.Shared), {
      message: 'I/O error: strings passed to WinAPI cannot contain NULs',
    });
  });

  it('declares the kernel32 functions to node:ffi and bun:ffi', () => {
    const declared = {};
    const node = {
      dlopen: (name, definitions) => {
        declared[name] = definitions;
        return { functions: { GetLastError: () => 0 } };
      },
    };
    assert.equal(loadKernel32(node, 'node:ffi', 8).GetLastError(), 0);
    const definitions = declared['kernel32.dll'];
    assert.deepEqual(definitions.CreateFileW, {
      arguments: ['pointer', 'uint32', 'uint32', 'pointer', 'uint32', 'uint32', 'pointer'],
      return: 'int64',
    });
    assert.deepEqual(definitions.LockFileEx, {
      arguments: ['int64', 'uint32', 'uint32', 'uint32', 'uint32', 'pointer'],
      return: 'int32',
    });
    assert.deepEqual(Object.keys(definitions), [
      'CreateFileW',
      'CloseHandle',
      'LockFileEx',
      'UnlockFile',
      'CreateEventW',
      'GetOverlappedResult',
      'GetLastError',
      'GetFullPathNameW',
      'FormatMessageW',
    ]);
    const bun = {
      FFIType: { i64: 'i64', i32: 'i32', ptr: 'ptr', u32: 'u32' },
      dlopen: (name, bunDefinitions) => ({ symbols: bunDefinitions }),
    };
    const symbols = loadKernel32(bun, 'bun:ffi', 4);
    assert.deepEqual(symbols.CloseHandle, { args: ['i32'], returns: 'i32' });
    assert.deepEqual(symbols.CreateEventW, { args: ['ptr', 'i32', 'i32', 'ptr'], returns: 'i32' });
    assert.deepEqual(loadKernel32(bun, 'bun:ffi', 8).CloseHandle, {
      args: ['i64'],
      returns: 'i32',
    });
    assert.deepEqual(loadKernel32(node, 'node:ffi', 4).GetLastError(), 0);
    assert.deepEqual(declared['kernel32.dll'].CloseHandle, {
      arguments: ['int32'],
      return: 'int32',
    });
  });

  it('binds kernel32 on Windows once per pointer size and names a failure', () => {
    const loads = [];
    const ffi = {
      dlopen(name, definitions) {
        loads.push(definitions.CloseHandle.arguments[0]);
        return { functions: {} };
      },
    };
    const environment = {
      deno: null,
      versions: { node: '26.13.0' },
      platform: 'win32',
      builtin: (id) => (id === 'node:ffi' ? ffi : undefined),
    };
    const backend = detectLockBackend({ ...environment, arch: 'x64' });
    assert.equal(backend.primitive, 'LockFileEx through node:ffi');
    assert.equal(detectLockBackend({ ...environment, arch: 'x64' }), backend);
    detectLockBackend({ ...environment, arch: 'arm64' });
    detectLockBackend({ ...environment, arch: 'ia32' });
    detectLockBackend({ ...environment, arch: 'arm' });
    assert.deepEqual(loads, ['int64', 'int64', 'int32', 'int32']);
    const missing = {
      dlopen() {
        throw new Error('kernel32.dll: not found');
      },
    };
    assert.deepEqual(
      detectLockBackend({ ...environment, versions: { node: '26.0.0' }, builtin: () => missing }),
      {
        unavailable:
          'Node.js 26.0.0 cannot load kernel32.dll through node:ffi (kernel32.dll: not found)',
      },
    );
  });
});

describe('FileLock on this runtime', () => {
  const sidecar = () => join(scratchDirectory(), 'db.links.lock');

  it('excludes a second exclusive holder in the same process', real, () => {
    const path = sidecar();
    const first = FileLock.acquire(path, LockMode.Exclusive);
    assert.equal(FileLock.tryAcquire(path, LockMode.Exclusive), null);
    assert.equal(FileLock.tryAcquire(path, LockMode.Shared), null);
    first.release();
    const again = FileLock.tryAcquire(path, LockMode.Exclusive);
    assert.notEqual(again, null);
    again.release();
  });

  it('lets shared holders share and keeps an exclusive one out', real, () => {
    const path = sidecar();
    const first = FileLock.acquire(path, LockMode.Shared);
    const second = FileLock.tryAcquire(path, LockMode.Shared);
    assert.notEqual(second, null);
    assert.equal(FileLock.tryAcquire(path, LockMode.Exclusive), null);
    first.release();
    assert.equal(FileLock.tryAcquire(path, LockMode.Exclusive), null);
    second.release();
    FileLock.acquire(path, LockMode.Exclusive).release();
  });

  it('creates missing parent directories and keeps the sidecar', real, () => {
    const path = join(scratchDirectory(), 'a', 'b', 'db.links.lock');
    const lock = FileLock.acquire(path, LockMode.Exclusive);
    assert.ok(existsSync(path));
    lock.release();
    assert.ok(existsSync(path), 'releasing leaves the sidecar in place');
  });

  it('contends with another process, which the OS releases on exit', real, async () => {
    const directory = scratchDirectory();
    const path = join(directory, 'db.links.lock');
    const child = join(directory, 'holder.mjs');
    const module = pathToFileURL(join(import.meta.dirname, '..', 'src', 'storage-lock.js'));
    writeFileSync(
      child,
      `import { FileLock, LockMode } from ${JSON.stringify(module.href)};
FileLock.acquire(process.env.LOCK_PATH, LockMode.Exclusive);
console.log('locked');
setTimeout(() => process.exit(0), 300);
`,
    );
    const args = DENO ? ['run', '--allow-all', child] : [child];
    const holder = spawn(process.execPath, args, {
      env: { ...process.env, LOCK_PATH: path },
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const exited = new Promise((resolve) => holder.on('exit', resolve));
    await new Promise((resolve, reject) => {
      holder.stdout.on('data', (chunk) => String(chunk).includes('locked') && resolve());
      holder.on('exit', (code) => reject(new Error(`holder exited with ${code}`)));
    });
    assert.equal(FileLock.tryAcquire(path, LockMode.Exclusive), null);
    assert.equal(FileLock.tryAcquire(path, LockMode.Shared), null);
    // Blocks until the holder exits without releasing.
    const lock = FileLock.acquire(path, LockMode.Exclusive);
    lock.release();
    assert.equal(await exited, 0);
  });

  const flock = spawnSync('flock', ['--version']).status === 0;
  it('contends with flock(1)', { skip: REAL.unavailable ?? (!flock && 'no flock(1)') }, () => {
    const path = sidecar();
    const lock = FileLock.acquire(path, LockMode.Shared);
    assert.equal(spawnSync('flock', ['-n', '-s', path, 'true']).status, 0);
    assert.equal(spawnSync('flock', ['-n', path, 'true']).status, 1);
    lock.release();
    assert.equal(spawnSync('flock', ['-n', path, 'true']).status, 0);
  });

  const rustc = spawnSync('rustc', ['--version']).status === 0;
  it(
    'contends with the Rust std File::try_lock of rust/src/storage/lock.rs',
    { skip: REAL.unavailable ?? (!rustc && 'no rustc'), timeout: 120000 },
    () => {
      const directory = scratchDirectory();
      const path = join(directory, 'db.links.lock');
      mkdirSync(join(directory, 'rust'));
      writeFileSync(
        join(directory, 'rust', 'main.rs'),
        `use std::fs::{OpenOptions, TryLockError};
fn main() {
    let path = std::env::args().nth(1).unwrap();
    let file = OpenOptions::new().create(true).truncate(false).read(true).write(true)
        .open(path).unwrap();
    match file.try_lock() {
        Ok(()) => print!("locked"),
        Err(TryLockError::WouldBlock) => print!("busy"),
        Err(TryLockError::Error(error)) => print!("{error}"),
    }
}
`,
      );
      const program = join(directory, 'rust', WINDOWS ? 'try-lock.exe' : 'try-lock');
      const build = spawnSync('rustc', ['-o', program, join(directory, 'rust', 'main.rs')]);
      assert.equal(build.status, 0, String(build.stderr));
      const tryLock = () => spawnSync(program, [path], { encoding: 'utf8' }).stdout;
      const lock = FileLock.acquire(path, LockMode.Shared);
      assert.equal(tryLock(), 'busy');
      lock.release();
      assert.equal(tryLock(), 'locked');
    },
  );
});

// Where Bun and Node.js >= 26 lock with LockFileEx; the tests above run there
// too (two holders in one process, a child process, Rust's File::try_lock).
const kernel32Skip = !WINDOWS
  ? 'not Windows'
  : (REAL.unavailable ??
    (!REAL.primitive.startsWith('LockFileEx') && `${REAL.primitive} locks here`));

describe('FileLock on Windows through kernel32', { skip: kernel32Skip }, () => {
  it('names LockFileEx and the foreign function interface', () => {
    const id = process.versions.bun === undefined ? 'node:ffi' : 'bun:ffi';
    assert.equal(REAL.primitive, `LockFileEx through ${id}`);
  });

  it('keeps what the sidecar holds and opens it for every sharing mode', () => {
    const path = join(scratchDirectory(), 'db.links.lock');
    writeFileSync(path, 'kept');
    const lock = FileLock.acquire(path, LockMode.Exclusive);
    assert.equal(readFileSync(path, 'utf8'), 'kept', 'readable by others, not truncated');
    lock.release();
  });

  it('locks a sidecar past MAX_PATH through the \\\\?\\ prefix', () => {
    const path = join(scratchDirectory(), 'd'.repeat(120), 'e'.repeat(120), 'db.links.lock');
    assert.ok(path.length > 260);
    const lock = FileLock.acquire(path, LockMode.Exclusive);
    assert.equal(FileLock.tryAcquire(path, LockMode.Shared), null);
    lock.release();
    FileLock.tryAcquire(path, LockMode.Shared).release();
  });

  it('reports a directory in place of the sidecar as Rust does', () => {
    const path = join(scratchDirectory(), 'db.links.lock');
    mkdirSync(path);
    assert.throws(() => FileLock.acquire(path, LockMode.Exclusive), {
      name: 'LinkError',
      message: /^I\/O error: .+ \(os error 5\)$/,
    });
  });
});
