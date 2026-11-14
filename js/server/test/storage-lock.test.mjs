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
    assert.deepEqual(detectLockBackend({ deno: null, versions: bun, platform: 'win32' }), {
      unavailable:
        'Bun 1.4.4 on Windows has no LockFileEx binding in this port; use Deno or the Rust clink',
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
    const expected = DENO
      ? 'Deno.FsFile.lockSync'
      : `flock(2) through ${process.versions.bun ? 'bun' : 'node'}:ffi`;
    assert.equal(backend.primitive ?? backend.unavailable, REAL.primitive ?? REAL.unavailable);
    if (backend.unavailable === undefined) assert.equal(backend.primitive, expected);
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
      const program = join(directory, 'rust', 'try-lock');
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
