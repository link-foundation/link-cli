// Store locking of `clink` invocations (issue #194), the port of
// rust/src/store_lock/: option parsing, the lock order, the owner record and
// its probes, the `--lock pid` directory lock, the `--lock flock` lock over an
// injected backend (so that every runtime reaches every branch), and
// `--lock-status`. tests/cli_store_lock_tests.rs drives the same through the
// Rust executable; main.test.mjs through `clink` processes.
import assert from 'node:assert/strict';
import {
  closeSync,
  existsSync,
  mkdirSync,
  openSync,
  readdirSync,
  readFileSync,
  renameSync,
  rmSync,
  utimesSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { afterEach, describe, it } from 'node:test';

import { InvalidFormatError, LinkError, LockError } from '../src/errors.js';
import { lockFilePath, LockMode, setLockBackend } from '../src/storage-lock.js';
import {
  breakStale,
  DEFAULT_LOCK_TIMEOUT_MS,
  hostName,
  lockDirPath,
  LockOwner,
  lockOrder,
  lockStatus,
  ownerFilePath,
  parseStoreLockOptions,
  pidNamespace,
  processAlive,
  processStartTime,
  quietFfiWarning,
  readOwner,
  staleReason,
  StoreLock,
  StoreLockKind,
  tryAcquirePid,
  writeRecord,
} from '../src/store-lock.js';
import { capture, literally, scratchDirectory } from './helpers/storage.mjs';

afterEach(() => setLockBackend(null));

/** A process id no system hands out: `processAlive` calls it gone. */
const GONE_PID = 0xffffffff;

/**
 * An in-process `flock`: shared holders or one exclusive holder per path.
 * `failTry(path, exclusive)` makes a `tryLock` throw.
 */
function fakeFlock({ failTry = () => false } = {}) {
  const held = new Map();
  const state = (path) => {
    if (!held.has(path)) held.set(path, { exclusive: 0, shared: 0 });
    return held.get(path);
  };
  return {
    primitive: 'fake',
    open: (path) => {
      closeSync(openSync(path, 'a'));
      return { path, mode: null };
    },
    lock: () => {
      throw new Error('store locks never block');
    },
    tryLock: (handle, exclusive) => {
      if (failTry(handle.path, exclusive)) throw new Error('probe failed');
      const lock = state(handle.path);
      if (lock.exclusive > 0 || (exclusive && lock.shared > 0)) return false;
      lock[exclusive ? 'exclusive' : 'shared'] += 1;
      handle.mode = exclusive ? 'exclusive' : 'shared';
      return true;
    },
    unlock: (handle) => {
      state(handle.path)[handle.mode] -= 1;
      handle.mode = null;
    },
    close: () => {},
  };
}

const options = (kind, timeout, extra = {}) => ({
  ...parseStoreLockOptions(kind, timeout),
  ...extra,
});

/**
 * A pid lock planted in `dir` for `owner`, as another process leaves it;
 * without an owner, a directory some other tool made. (A rename onto an empty
 * directory replaces it on POSIX systems, so that one holds a file.)
 */
function plantPidLock(dir, owner) {
  mkdirSync(dir);
  if (owner === undefined) writeFileSync(join(dir, 'note'), '');
  else writeFileSync(join(dir, 'owner'), owner.toRecord());
  return dir;
}

describe('parseStoreLockOptions', () => {
  it('defaults to a flock waited for 30 s', () => {
    assert.deepEqual(parseStoreLockOptions(null, undefined), {
      kind: StoreLockKind.Flock,
      explicit: false,
      timeout: DEFAULT_LOCK_TIMEOUT_MS,
      timeoutText: '30000',
    });
    assert.equal(parseStoreLockOptions(' ', null).explicit, false);
  });

  it('reads kinds in any case and timeouts as u64 milliseconds', () => {
    assert.deepEqual(parseStoreLockOptions(' PID ', ' +007 '), {
      kind: StoreLockKind.Pid,
      explicit: true,
      timeout: 7,
      timeoutText: '7',
    });
    assert.equal(parseStoreLockOptions('None', '0').kind, StoreLockKind.None);
    const longest = parseStoreLockOptions('flock', '18446744073709551615');
    assert.equal(longest.timeoutText, '18446744073709551615');
    assert.equal(longest.explicit, true);
  });

  it('rejects other values with the messages of the Rust clink', () => {
    assert.throws(() => parseStoreLockOptions('mutex', null), {
      name: InvalidFormatError.name,
      message: "Invalid link format: Invalid --lock value 'mutex'. Use 'flock', 'pid' or 'none'.",
    });
    for (const timeout of ['soon', '-1', '1.5', '18446744073709551616']) {
      assert.throws(() => parseStoreLockOptions(null, ` ${timeout} `), {
        message: `Invalid link format: Invalid --lock-timeout value '${timeout}'. Use a whole number of milliseconds.`,
      });
    }
  });
});

describe('lockOrder', () => {
  it('sorts by absolute path and locks a store spelled twice once', () => {
    const dir = scratchDirectory();
    const a = join(dir, 'a.links');
    const b = join(dir, 'b.links');
    const again = `${dir}/./a.links`;
    assert.deepEqual(lockOrder([b, a, again]), [a, b]);
    assert.deepEqual(lockOrder([again, b]), [again, b]);
  });

  it('puts a directory before what it holds, and keeps missing parents', () => {
    const dir = scratchDirectory();
    const inner = join(dir, 'missing', 'x.links');
    const outer = join(dir, 'missing');
    assert.deepEqual(lockOrder([inner, outer]), [outer, inner]);
    assert.deepEqual(lockOrder([]), []);
  });
});

describe('owner probes', () => {
  it('names the host', () => {
    assert.equal(hostName('win32', { COMPUTERNAME: 'BUILD-7' }), 'BUILD-7');
    assert.equal(hostName('win32', {}), '');
    assert.equal(
      hostName('linux', {}, () => 'build-7'),
      'build-7',
    );
    assert.equal(
      hostName('linux', {}, () => {
        throw new Error('no name');
      }),
      '',
    );
    assert.equal(typeof hostName(), 'string');
  });

  it('reads the PID namespace on Linux only', () => {
    assert.equal(pidNamespace('darwin'), '');
    assert.equal(
      pidNamespace('linux', () => 'pid:[4026531836]'),
      'pid:[4026531836]',
    );
    assert.equal(
      pidNamespace('linux', () => {
        throw new Error('no /proc');
      }),
      '',
    );
  });

  it('reads the start time past a command name with spaces and parentheses', () => {
    const fields = ['S', ...Array.from({ length: 18 }, (_, index) => String(index)), '4242', '9'];
    const stat = `7 (we) ird) ${fields.join(' ')}\n`;
    assert.equal(
      processStartTime(7, 'linux', () => stat),
      '4242',
    );
    assert.equal(
      processStartTime(7, 'linux', () => '7 (short) S 1 2'),
      null,
    );
    assert.equal(
      processStartTime(7, 'linux', () => 'garbled'),
      null,
    );
    assert.equal(
      processStartTime(7, 'linux', () => {
        throw new Error('ENOENT');
      }),
      null,
    );
    assert.equal(processStartTime(7, 'win32'), null);
  });

  it('tells running, gone and unknown processes apart', () => {
    const failing = (code) => () => {
      throw Object.assign(new Error(code), { code });
    };
    assert.equal(processAlive(process.pid), true);
    assert.equal(processAlive(0), false);
    assert.equal(processAlive(GONE_PID), false);
    assert.equal(
      processAlive(5, () => true),
      true,
    );
    assert.equal(processAlive(5, failing('ESRCH')), false);
    assert.equal(processAlive(5, failing('EPERM')), true);
    assert.equal(processAlive(5, failing('EINVAL')), null);
  });
});

describe('LockOwner', () => {
  it('describes this process and reads its own record back', () => {
    const owner = LockOwner.current('t1');
    assert.equal(owner.pid, process.pid);
    assert.match(owner.program, /^clink \(js\) \d/);
    assert.equal(LockOwner.current('t2').program, owner.program);
    assert.deepEqual(LockOwner.parse(owner.toRecord()), owner);
    assert.deepEqual(owner.liveness(), { state: 'alive' });
    const described = owner.describe();
    assert.ok(described.startsWith(`pid=${process.pid} host=${owner.host}`), described);
    assert.ok(described.includes(` acquired_at_ms=${owner.acquiredAtMs}`), described);
    assert.ok(described.endsWith(' (running)'), described);
  });

  it('leaves what a platform cannot report empty', () => {
    const owner = LockOwner.current('t', 'darwin');
    assert.deepEqual([owner.pidNamespace, owner.startTime], ['', '']);
    assert.equal(owner.pid, process.pid);
  });

  it('parses records leniently but needs a process id', () => {
    assert.equal(LockOwner.parse(''), null);
    assert.equal(LockOwner.parse('pid=0\n'), null);
    assert.equal(LockOwner.parse('pid=x\nhost=h\n'), null);
    assert.equal(LockOwner.parse('pid=4294967296\n'), null);
    const owner = LockOwner.parse(
      'comment\n pid = +12 \nacquired_at_ms=soon\nfuture=1\nhost=far\n',
    );
    assert.deepEqual(
      { ...owner },
      {
        pid: 12,
        host: 'far',
        pidNamespace: '',
        startTime: '',
        acquiredAtMs: 0,
        token: '',
        program: '',
      },
    );
    assert.equal(owner.describe(), 'pid=12 host=far (held on another host, far)');
  });

  it('judges only owners on this host and in this PID namespace', () => {
    const owner = LockOwner.current('t');
    assert.deepEqual(new LockOwner({ ...owner, pidNamespace: 'pid:[1]' }).liveness(), {
      state: 'unknown',
      reason: 'held in another PID namespace',
    });
    assert.deepEqual(
      owner.liveness(() => false),
      { state: 'gone', reason: `owner process ${process.pid} is gone` },
    );
    assert.deepEqual(
      owner.liveness(() => null),
      { state: 'unknown', reason: 'cannot tell whether the owner runs' },
    );
    const reused = new LockOwner({ ...owner, startTime: 'never' }).liveness();
    if (processStartTime(process.pid) === null) assert.deepEqual(reused, { state: 'alive' });
    else {
      assert.deepEqual(reused, {
        state: 'gone',
        reason: `owner process ${process.pid} is gone; its id now belongs to another process`,
      });
    }
  });
});

describe('--lock pid', () => {
  it('publishes, reports and releases a lock directory', () => {
    const dir = lockDirPath(join(scratchDirectory(), 'nested', 's.links'));
    assert.ok(dir.endsWith('s.links.lock.d'));
    const first = tryAcquirePid(dir);
    const lock = first.acquired;
    assert.equal(lock.path(), dir);
    assert.equal(readOwner(dir).token, lock.token);

    const second = tryAcquirePid(dir);
    assert.equal(second.acquired, undefined);
    assert.equal(second.held.pid, process.pid);
    assert.equal(staleReason(dir, second.held), null);

    lock.release();
    assert.equal(existsSync(dir), false);
    // Releasing what is no longer there does nothing.
    lock.release();
    assert.deepEqual(readdirSync(join(dir, '..')), []);
  });

  it('leaves a lock alone once it is no longer its own', () => {
    const dir = lockDirPath(join(scratchDirectory(), 's.links'));
    const lock = tryAcquirePid(dir).acquired;
    writeFileSync(join(dir, 'owner'), LockOwner.current('someone else').toRecord());
    lock.release();
    assert.equal(readOwner(dir).token, 'someone else');
  });

  it('keeps a lock it cannot move aside on release', () => {
    const dir = lockDirPath(join(scratchDirectory(), 's.links'));
    const lock = tryAcquirePid(dir).acquired;
    mkdirSync(join(`${dir}.${lock.token}.released`, 'occupied'), { recursive: true });
    lock.release();
    assert.equal(readOwner(dir).token, lock.token);
  });

  it('breaks the lock of a gone owner, tracing it when asked', () => {
    const root = scratchDirectory();
    const gone = new LockOwner({ ...LockOwner.current('crashed'), pid: GONE_PID });
    for (const trace of [false, true]) {
      const dir = plantPidLock(lockDirPath(join(root, `${trace}.links`)), gone);
      assert.equal(staleReason(dir, readOwner(dir)), `owner process ${GONE_PID} is gone`);
      const stderr = capture();
      const lock = tryAcquirePid(dir, trace, stderr).acquired;
      assert.equal(readOwner(dir).token, lock.token);
      assert.equal(
        stderr.text,
        trace ? `[TRACE] Breaking stale lock ${dir}: owner process ${GONE_PID} is gone\n` : '',
      );
      lock.release();
    }
    assert.deepEqual(readdirSync(root), []);
  });

  it('breaks a directory without a record only once it is old', () => {
    const dir = lockDirPath(join(scratchDirectory(), 's.links'));
    assert.equal(staleReason(dir, null), null, 'no directory, nothing to break');
    plantPidLock(dir);
    assert.equal(staleReason(dir, null), null);
    assert.deepEqual(tryAcquirePid(dir), { held: null });
    const old = new Date(Date.now() - 60_000);
    utimesSync(dir, old, old);
    assert.equal(staleReason(dir, null), 'no owner record for 10 s');
    const lock = tryAcquirePid(dir).acquired;
    assert.equal(readOwner(dir).token, lock.token);
  });

  it('moves back a lock that changed hands while being broken', () => {
    const root = scratchDirectory();
    const gone = new LockOwner({ ...LockOwner.current('crashed'), pid: GONE_PID });
    const dir = plantPidLock(lockDirPath(join(root, 's.links')), LockOwner.current('new owner'));
    breakStale(dir, gone);
    assert.equal(readOwner(dir).token, 'new owner');
    // Another waiter moved it first: nothing to do.
    breakStale(lockDirPath(join(root, 'none.links')), gone);
    // A third process published meanwhile: the moved directory stays aside.
    breakStale(dir, gone, (from, to) => {
      renameSync(from, to);
      if (from === dir) plantPidLock(dir, LockOwner.current('third'));
    });
    assert.equal(readOwner(dir).token, 'third');
    assert.equal(readdirSync(root).filter((name) => name.endsWith('.stale')).length, 1);
  });

  it('gives up after four attempts', () => {
    const root = scratchDirectory();
    const denied = () => {
      throw Object.assign(new Error('EACCES: permission denied'), { code: 'EACCES' });
    };
    assert.throws(() => tryAcquirePid(lockDirPath(join(root, 'a.links')), false, capture(), denied), {
      message: 'EACCES: permission denied',
    });
    const gone = new LockOwner({ ...LockOwner.current('crashed'), pid: GONE_PID });
    const dir = plantPidLock(lockDirPath(join(root, 'b.links')), gone);
    const stuck = (from, to) => {
      if (to.endsWith('.stale')) denied();
      renameSync(from, to);
    };
    const stderr = capture();
    assert.deepEqual(tryAcquirePid(dir, true, stderr, stuck), { held: gone });
    assert.equal(stderr.text.split('Breaking stale lock').length - 1, 4);
    assert.deepEqual(readdirSync(root).sort(), ['b.links.lock.d']);
  });
});

describe('StoreLock with --lock flock', () => {
  it('holds a store exclusively, naming its owner to waiters', async () => {
    setLockBackend(fakeFlock());
    const db = join(scratchDirectory(), 's.links');
    const path = lockFilePath(db);
    const stderr = capture();
    const writer = await StoreLock.acquire([db], LockMode.Exclusive, options(null, '0', { trace: true, stderr }));
    assert.deepEqual(writer.paths(), [path]);
    assert.equal(stderr.text, `[TRACE] Acquired exclusive flock lock ${path}\n`);
    assert.equal(LockOwner.parse(readFileSync(ownerFilePath(db), 'utf8')).pid, process.pid);

    const waiting = capture();
    await assert.rejects(
      StoreLock.acquire([db], LockMode.Shared, options(null, '0', { trace: true, stderr: waiting })),
      (error) => {
        assert.ok(error instanceof LockError);
        assert.match(
          error.message,
          new RegExp(
            `^Lock error: store is locked: ${literally(path)} is held by an exclusive writer: pid=${process.pid} .*\\(running\\); gave up waiting for the shared flock lock after 0 ms \\(--lock-timeout\\)$`,
          ),
        );
        return true;
      },
    );
    assert.match(waiting.text, /^\[TRACE\] Waiting for shared flock lock .*: held by an exclusive writer: pid=/);
    assert.match(lockStatus([db]), /^flock .*: locked by an exclusive writer: pid=\d+ .*\npid .*\.lock\.d: unlocked\n$/);

    rmSync(ownerFilePath(db));
    await assert.rejects(StoreLock.acquire([db], LockMode.Exclusive, options(null, '0')), {
      message: /is held by an exclusive writer that left no owner record;/,
    });

    writer.release();
    writer.release();
    assert.equal(existsSync(ownerFilePath(db)), false);
    assert.equal(lockStatus([db]), `flock ${path}: unlocked\npid ${db}.lock.d: unlocked\n`);
  });

  it('shares a store among readers, who clear a crashed writer record', async () => {
    setLockBackend(fakeFlock());
    const db = join(scratchDirectory(), 's.links');
    writeRecord(ownerFilePath(db), LockOwner.current('crashed').toRecord());
    const first = await StoreLock.acquire([db], LockMode.Shared, options(null, '0'));
    assert.equal(existsSync(ownerFilePath(db)), false);
    const second = await StoreLock.acquire([db], LockMode.Shared, options(null, '0'));
    await assert.rejects(StoreLock.acquire([db], LockMode.Exclusive, options(null, '20')), {
      message: /is held by shared readers; gave up waiting for the exclusive flock lock after 20 ms/,
    });
    assert.match(lockStatus([db]), /: locked by shared readers\n/);
    first.release();
    second[Symbol.dispose]();
    (await StoreLock.acquire([db], LockMode.Exclusive, options(null, '0'))).release();
  });

  it('waits for the holder to finish', async () => {
    setLockBackend(fakeFlock());
    const db = join(scratchDirectory(), 's.links');
    const holder = await StoreLock.acquire([db], LockMode.Exclusive, options(null, '0'));
    setTimeout(() => holder.release(), 30);
    const waiter = await StoreLock.acquire([db], LockMode.Exclusive, options(null, '5000'));
    waiter.release();
  });

  it('names the record when probing for readers fails', async () => {
    setLockBackend(fakeFlock({ failTry: (path, exclusive) => !exclusive }));
    const db = join(scratchDirectory(), 's.links');
    const holder = await StoreLock.acquire([db], LockMode.Exclusive, options(null, '0'));
    await assert.rejects(StoreLock.acquire([db], LockMode.Exclusive, options(null, '0')), {
      message: new RegExp(`held by an exclusive writer: pid=${process.pid} `),
    });
    holder.release();
    setLockBackend(fakeFlock({ failTry: () => true }));
    assert.match(lockStatus([db]), /^flock .*: unknown \(.*probe failed\)\n/);
  });

  it('releases the stores it took when a later one is held', async () => {
    setLockBackend(fakeFlock());
    const root = scratchDirectory();
    const [a, b] = [join(root, 'a.links'), join(root, 'b.links')];
    const held = await StoreLock.acquire([b], LockMode.Exclusive, options(null, '0'));
    await assert.rejects(StoreLock.acquire([a, b], LockMode.Exclusive, options(null, '0')), {
      message: /b\.links\.lock is held/,
    });
    const free = await StoreLock.acquire([a], LockMode.Exclusive, options(null, '0'));
    free.release();
    held.release();
  });

  it('reports stores that were never locked without creating anything', () => {
    const root = scratchDirectory();
    const db = join(root, 's.links');
    assert.equal(lockStatus([db]), `flock ${lockFilePath(db)}: unlocked\npid ${db}.lock.d: unlocked\n`);
    assert.deepEqual(readdirSync(root), []);
  });

  it('keeps a record it cannot write to itself', () => {
    const missing = join(scratchDirectory(), 'missing', 's.links.lock.owner');
    writeRecord(missing, 'pid=1\n');
    assert.equal(existsSync(join(missing, '..')), false);
  });
});

describe('StoreLock with --lock pid and none', () => {
  it('takes no lock with --lock none', async () => {
    const db = join(scratchDirectory(), 's.links');
    const lock = await StoreLock.acquire([db], LockMode.Exclusive, options('none', null));
    assert.deepEqual(lock.paths(), []);
  });

  it('falls back to the pid lock where this runtime has no flock', async () => {
    setLockBackend({ unavailable: 'no flock here' });
    const db = join(scratchDirectory(), 's.links');
    const stderr = capture();
    const traced = await StoreLock.acquire([db], LockMode.Shared, options(null, null, { trace: true, stderr }));
    assert.deepEqual(traced.paths(), [lockDirPath(db)]);
    assert.equal(
      stderr.text,
      `[TRACE] Locking with --lock pid: no flock here\n[TRACE] Acquired pid lock ${db}.lock.d\n`,
    );
    traced.release();
    const quiet = await StoreLock.acquire([db], LockMode.Shared, options(null, null));
    assert.deepEqual(quiet.paths(), [lockDirPath(db)]);
    quiet.release();
    assert.deepEqual((await StoreLock.acquire([], LockMode.Shared, options(null, null))).paths(), []);
    await assert.rejects(StoreLock.acquire([db], LockMode.Shared, options('flock', null)), {
      message: /no flock here/,
    });
  });

  it('waits for a live owner and names it', async () => {
    const db = join(scratchDirectory(), 's.links');
    const dir = plantPidLock(lockDirPath(db), LockOwner.current('alive'));
    const stderr = capture();
    await assert.rejects(StoreLock.acquire([db], LockMode.Shared, options('pid', '10', { trace: true, stderr })), {
      message: new RegExp(
        `^Lock error: store is locked: ${literally(dir)} is held by pid=${process.pid} .*\\(running\\); gave up waiting for the pid lock after 10 ms`,
      ),
    });
    assert.match(stderr.text, /^\[TRACE\] Waiting for pid lock .*: held by pid=/);
    assert.match(lockStatus([db]), /\npid .*: locked by pid=\d+ .*\(running\)\n$/);

    renameSync(join(dir, 'owner'), join(dir, 'note'));
    await assert.rejects(StoreLock.acquire([db], LockMode.Shared, options('pid', '0')), {
      message: /held by a directory without an owner record \(removed once .* is 10 s old\);/,
    });
    rmSync(join(dir, 'note'));
    writeFileSync(join(dir, 'owner'), new LockOwner({ ...LockOwner.current('x'), pid: GONE_PID }).toRecord());
    assert.match(
      lockStatus([db]),
      new RegExp(`; stale, the next writer breaks it: owner process ${GONE_PID} is gone\\n$`),
    );
  });

  it('reports what keeps it from making the lock directory', async () => {
    const file = join(scratchDirectory(), 'file');
    writeFileSync(file, '');
    await assert.rejects(StoreLock.acquire([join(file, 's.links')], LockMode.Shared, options('pid', '0')), (error) => {
      assert.ok(error instanceof LinkError);
      assert.match(error.message, /^I\/O error: /);
      return true;
    });
  });
});

describe('quietFfiWarning', () => {
  it('drops only the node:ffi experimental warning', () => {
    const emitted = [];
    const target = {
      emitWarning(...args) {
        emitted.push(args);
        return 'emitted';
      },
    };
    quietFfiWarning(target);
    assert.equal(target.emitWarning('FFI is an experimental feature and might change at any time', 'ExperimentalWarning'), undefined);
    assert.equal(target.emitWarning(new Error('FFI is an experimental feature')), undefined);
    assert.equal(target.emitWarning('Something else', 'Warning'), 'emitted');
    assert.equal(target.emitWarning(undefined), 'emitted');
    assert.deepEqual(emitted, [['Something else', 'Warning'], [undefined]]);
  });
});
