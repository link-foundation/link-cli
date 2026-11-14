// The kernel32 lock of rust/src/storage/lock.rs, as Bun and Node.js take it on
// Windows (src/storage-lock-windows.js): over a model of kernel32 on every
// platform, and over the real kernel32 on Windows.
import assert from 'node:assert/strict';
import { closeSync, mkdirSync, openSync, readSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { afterEach, describe, it } from 'node:test';
import { formatErrorReport } from '../src/error-report.js';
import { LinkError } from '../src/errors.js';
import {
  errorString,
  kernel32Backend,
  loadKernel32,
  maybeVerbatim,
  overlappedLayout,
  WindowsIoError,
} from '../src/storage-lock-windows.js';
import {
  detectLockBackend,
  FileLock,
  lockBackend,
  LockMode,
  setLockBackend,
} from '../src/storage-lock.js';
import { scratchDirectory } from './helpers/storage.mjs';

const REAL = lockBackend();
const WINDOWS = process.platform === 'win32';

afterEach(() => setLockBackend(null));

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
    // Every FILE_SHARE_* mode: another handle opens the sidecar. The range
    // lock of LockFileEx is mandatory, as under Rust's File::lock, so that
    // handle cannot read it until the lock is released.
    const fd = openSync(path, 'r');
    try {
      assert.throws(() => readSync(fd, Buffer.alloc(4), 0, 4, 0), { code: 'EBUSY' });
      lock.release();
      const bytes = Buffer.alloc(8);
      assert.equal(bytes.toString('utf8', 0, readSync(fd, bytes, 0, 8, 0)), 'kept');
    } finally {
      closeSync(fd);
    }
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
