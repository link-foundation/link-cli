// Store locking for `clink` invocations (issue #194), the port of
// rust/src/store_lock/.
//
// Every invocation loads the whole store, changes it in memory and saves it
// back. Two invocations that overlap would each save their own copy, and the
// later save would drop the writes of the earlier one. A `StoreLock` makes
// that read-modify-write exclusive: writers take it exclusively, readers
// share it, and a waiter gives up with a "store is locked" error naming the
// owner once `--lock-timeout` passes.
//
// | `--lock` | Lock                                  | Released by                                  |
// | -------- | ------------------------------------- | -------------------------------------------- |
// | `flock`  | `flock`/`LockFileEx` on `<db>.lock`   | the operating system when the holder exits   |
// | `pid`    | the directory `<db>.lock.d`           | the holder; a waiter breaks it once the owner process is gone |
// | `none`   | nothing                               | -                                            |
//
// An exclusive `flock` holder writes its owner record to `<db>.lock.owner`
// and removes it on release, so that waiters and `--lock-status` can name
// it. The record is a file of its own because on Windows the lock keeps
// every other process from reading `<db>.lock` itself. Where this runtime cannot
// take a `flock` (Node.js before 26) and `--lock` was not given, the `pid`
// lock is taken instead; an explicit `--lock flock` fails there.
//
// The owner record is a few `key=value` lines, the same in every
// implementation:
//
//   pid=4242
//   host=build-7
//   pid_namespace=pid:[4026531836]
//   start_time=1234567
//   acquired_at_ms=1764028800000
//   token=5f0c2a9d1e3b4c6a
//   program=clink (rust) 3.1.2
//
// `pid_namespace` and `start_time` are only known on Linux; elsewhere they
// are empty and the owner is judged by host and process id alone.
import {
  closeSync,
  existsSync,
  fsyncSync,
  lstatSync,
  mkdirSync,
  openSync,
  readFileSync,
  readlinkSync,
  realpathSync,
  renameSync,
  rmSync,
  statSync,
  unlinkSync,
  writeFileSync,
  writeSync,
} from 'node:fs';
import { hostname } from 'node:os';
import { basename, dirname, join, resolve, sep } from 'node:path';
import { setTimeout as sleep } from 'node:timers/promises';

import { InvalidFormatError, LockError } from './errors.js';
import { rustTrim } from './query-parser.js';
import { FileLock, lockBackend, lockFilePath, LockMode, uniqueToken } from './storage-lock.js';
import { ioError } from './transactions-storage.js';

/** How long a waiter waits by default, in milliseconds. */
export const DEFAULT_LOCK_TIMEOUT_MS = 30000;

/** The `--lock` mechanisms. */
export const StoreLockKind = Object.freeze({ Flock: 'flock', Pid: 'pid', None: 'none' });

/**
 * Reads the raw values of `--lock` and `--lock-timeout` into
 * `{ kind, explicit, timeout, timeoutText }`: `explicit` tells whether
 * `--lock` named a kind, and `timeoutText` is the timeout as given, exactly.
 */
export function parseStoreLockOptions(kind, timeout) {
  const options = {
    kind: StoreLockKind.Flock,
    explicit: false,
    timeout: DEFAULT_LOCK_TIMEOUT_MS,
    timeoutText: String(DEFAULT_LOCK_TIMEOUT_MS),
  };
  const value = kind == null ? '' : rustTrim(kind);
  if (value !== '') {
    const lower = value.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
    if (!Object.values(StoreLockKind).includes(lower)) {
      throw new InvalidFormatError(
        `Invalid --lock value '${value}'. Use 'flock', 'pid' or 'none'.`,
      );
    }
    options.kind = lower;
    options.explicit = true;
  }
  if (timeout != null) {
    const text = rustTrim(timeout);
    // `u64::from_str`: an optional `+`, then decimal digits that fit.
    const digits = /^\+?(\d+)$/.exec(text);
    if (digits === null || BigInt(digits[1]) > 0xffffffffffffffffn) {
      throw new InvalidFormatError(
        `Invalid --lock-timeout value '${text}'. Use a whole number of milliseconds.`,
      );
    }
    options.timeoutText = BigInt(digits[1]).toString();
    options.timeout = Number(digits[1]);
  }
  return options;
}

/** The absolute path of `store`, with its parent directory resolved where it exists. */
function normalize(store) {
  const absolute = resolve(store);
  try {
    return join(realpathSync(dirname(absolute)), basename(absolute));
  } catch {
    return absolute;
  }
}

/** `Path` ordering: component by component, each by its UTF-8 bytes. */
function comparePaths(left, right) {
  const a = left.split(sep).filter(Boolean);
  const b = right.split(sep).filter(Boolean);
  for (let index = 0; index < Math.min(a.length, b.length); index++) {
    const order = Buffer.compare(Buffer.from(a[index]), Buffer.from(b[index]));
    if (order !== 0) return order;
  }
  return a.length - b.length;
}

/**
 * The stores of one invocation without duplicates, in the order every
 * process locks them: sorted by absolute path. Each keeps the spelling it
 * was given, which is what messages show.
 */
export function lockOrder(stores) {
  const paths = [...stores]
    .map((store) => [normalize(String(store)), String(store)])
    .sort((left, right) => comparePaths(left[0], right[0]));
  return paths
    .filter((path, index) => index === 0 || comparePaths(paths[index - 1][0], path[0]) !== 0)
    .map(([, store]) => store);
}

/** The name of this host, or an empty string when it cannot be read. */
export function hostName(platform = process.platform, env = process.env, name = hostname) {
  if (platform === 'win32') return env.COMPUTERNAME ?? '';
  try {
    return name();
  } catch {
    return '';
  }
}

/**
 * The PID namespace of this process (Linux), so that a container does not
 * judge the process ids of another; empty elsewhere.
 */
export function pidNamespace(platform = process.platform, readlink = readlinkSync) {
  if (platform !== 'linux') return '';
  try {
    return readlink('/proc/self/ns/pid');
  } catch {
    return '';
  }
}

/**
 * When `pid` started, in clock ticks since boot (Linux); `null` elsewhere or
 * when the process does not exist.
 */
export function processStartTime(pid, platform = process.platform, read = readFileSync) {
  if (platform !== 'linux') return null;
  let stat;
  try {
    stat = read(`/proc/${pid}/stat`, 'utf8');
  } catch {
    return null;
  }
  // The command name in parentheses may hold spaces; the fields after it
  // start with the state (field 3), so `starttime` (field 22) is the 20th.
  const close = stat.lastIndexOf(')');
  if (close < 0) return null;
  return stat.slice(close + 1).trim().split(/\s+/)[19] ?? null;
}

/** Whether process `pid` exists; `null` when the system cannot tell. */
export function processAlive(pid, kill = process.kill.bind(process)) {
  if (pid === 0 || pid > 0x7fffffff) return false;
  try {
    kill(pid, 0);
    return true;
  } catch (error) {
    if (error.code === 'ESRCH') return false;
    if (error.code === 'EPERM') return true;
    return null;
  }
}

let programVersion;
const program = () => {
  programVersion ??= JSON.parse(
    readFileSync(new URL('../package.json', import.meta.url), 'utf8'),
  ).version;
  return `clink (js) ${programVersion}`;
};

/** Who holds a store lock. */
export class LockOwner {
  constructor({
    pid = 0,
    host = '',
    pidNamespace = '',
    startTime = '',
    acquiredAtMs = 0,
    token = '',
    program = '',
  } = {}) {
    Object.assign(this, { pid, host, pidNamespace, startTime, acquiredAtMs, token, program });
  }

  /** The record this process writes when it takes a lock (on `platform`). */
  static current(token, platform = process.platform) {
    return new LockOwner({
      pid: process.pid,
      host: hostName(platform),
      pidNamespace: pidNamespace(platform),
      startTime: processStartTime(process.pid, platform) ?? '',
      acquiredAtMs: Date.now(),
      token,
      program: program(),
    });
  }

  /** The `key=value` lines of the record. */
  toRecord() {
    return (
      `pid=${this.pid}\nhost=${this.host}\npid_namespace=${this.pidNamespace}\n` +
      `start_time=${this.startTime}\nacquired_at_ms=${this.acquiredAtMs}\n` +
      `token=${this.token}\nprogram=${this.program}\n`
    );
  }

  /**
   * Reads a record; `null` unless it names a process id. Unknown keys are
   * ignored so that later versions can add fields.
   */
  static parse(text) {
    const owner = new LockOwner();
    for (const line of text.split('\n')) {
      const split = line.indexOf('=');
      if (split < 0) continue;
      const value = rustTrim(line.slice(split + 1));
      switch (rustTrim(line.slice(0, split))) {
        case 'pid': {
          if (!/^\+?\d+$/.test(value) || Number(value) > 0xffffffff) return null;
          owner.pid = Number(value);
          break;
        }
        case 'host':
          owner.host = value;
          break;
        case 'pid_namespace':
          owner.pidNamespace = value;
          break;
        case 'start_time':
          owner.startTime = value;
          break;
        case 'acquired_at_ms':
          owner.acquiredAtMs = /^\+?\d+$/.test(value) ? Number(value) : 0;
          break;
        case 'token':
          owner.token = value;
          break;
        case 'program':
          owner.program = value;
          break;
        default:
      }
    }
    return owner.pid === 0 ? null : owner;
  }

  /** One line for messages: every known field, then whether it still runs. */
  describe() {
    let text = `pid=${this.pid} host=${this.host}`;
    for (const [key, value] of [
      ['pid_namespace', this.pidNamespace],
      ['start_time', this.startTime],
      ['program', this.program],
    ]) {
      if (value !== '') text += ` ${key}=${value}`;
    }
    if (this.acquiredAtMs !== 0) text += ` acquired_at_ms=${this.acquiredAtMs}`;
    const { state, reason } = this.liveness();
    return `${text} (${state === 'alive' ? 'running' : reason})`;
  }

  /**
   * Whether the owner still runs: `{ state: 'alive' | 'gone' | 'unknown',
   * reason }`. Only an owner on this host and in this PID namespace can be
   * judged; a reused process id is told apart by its start time where the
   * system reports one. `alive` stands in for `processAlive`.
   */
  liveness(alive = processAlive) {
    if (this.host !== hostName()) {
      return { state: 'unknown', reason: `held on another host, ${this.host}` };
    }
    if (this.pidNamespace !== pidNamespace()) {
      return { state: 'unknown', reason: 'held in another PID namespace' };
    }
    const running = alive(this.pid);
    if (running === false) return { state: 'gone', reason: `owner process ${this.pid} is gone` };
    if (running === null) return { state: 'unknown', reason: 'cannot tell whether the owner runs' };
    const started = processStartTime(this.pid);
    if (started !== null && this.startTime !== '' && started !== this.startTime) {
      return {
        state: 'gone',
        reason: `owner process ${this.pid} is gone; its id now belongs to another process`,
      };
    }
    return { state: 'alive' };
  }
}

// --lock pid: a lock directory, `<db>.lock.d`, holding an `owner` record.
//
// The directory is built under a private name with its record inside, then
// renamed to `<db>.lock.d`. A rename onto an existing directory fails, so
// exactly one process publishes the lock, and a published lock always has
// its record. The operating system does not release it when the owner exits,
// so a waiter breaks the lock when the record names a process on this host,
// in this PID namespace, that is gone. Works where `flock` does not, such as
// on some network file systems.

/**
 * A directory without a readable record is treated as abandoned after this
 * long (an owner crashed between creating and renaming it, or another tool
 * made it).
 */
const UNOWNED_GRACE_MS = 10000;
const OWNER_FILE = 'owner';

/** `<db>.lock.d`. */
export const lockDirPath = (store) => `${store}.lock.d`;

/** The owner record of `dir`, if it has a readable one. */
export function readOwner(dir) {
  try {
    return LockOwner.parse(readFileSync(join(dir, OWNER_FILE), 'utf8'));
  } catch {
    return null;
  }
}

/** Why the lock in `dir` may be broken, if it may; `null` otherwise. */
export function staleReason(dir, owner, now = Date.now()) {
  if (owner != null) {
    const { state, reason } = owner.liveness();
    return state === 'gone' ? reason : null;
  }
  let modified;
  try {
    modified = statSync(dir).mtimeMs;
  } catch {
    return null;
  }
  return now - modified >= UNOWNED_GRACE_MS
    ? `no owner record for ${UNOWNED_GRACE_MS / 1000} s`
    : null;
}

function writeOwner(dir, owner) {
  const fd = openSync(join(dir, OWNER_FILE), 'w');
  try {
    writeSync(fd, owner.toRecord());
    fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
}

const exists = (path) => {
  try {
    lstatSync(path);
    return true;
  } catch {
    return false;
  }
};

/**
 * Moves a stale lock aside and removes it. Should a live owner have published
 * a new lock in between, its directory is moved back. A lock that cannot be
 * moved (another waiter moved it first, or the directory is not writable) is
 * left alone: the next attempt sees what became of it. `rename` stands in for
 * `renameSync`.
 */
export function breakStale(dir, expected, rename = renameSync) {
  const tombstone = `${dir}.${process.pid}.${uniqueToken()}.stale`;
  try {
    rename(dir, tombstone);
  } catch {
    return;
  }
  if (readOwner(tombstone)?.token === expected?.token) {
    rmSync(tombstone, { recursive: true, force: true });
    return;
  }
  try {
    rename(tombstone, dir);
  } catch {
    // A third process published meanwhile; the moved owner has lost its
    // lock. Keep its directory for inspection.
  }
}

/** A held `--lock pid` lock; `release()` removes the directory. */
export class PidLock {
  constructor(dir, token) {
    this.dir = dir;
    this.token = token;
  }

  path() {
    return this.dir;
  }

  release() {
    // Only remove the lock while it is still ours.
    if (readOwner(this.dir)?.token !== this.token) return;
    const tombstone = `${this.dir}.${this.token}.released`;
    try {
      renameSync(this.dir, tombstone);
    } catch {
      return;
    }
    rmSync(tombstone, { recursive: true, force: true });
  }
}

/**
 * Publishes the lock once, breaking a stale one first:
 * `{ acquired: PidLock }` or `{ held: LockOwner | null }`. `rename` stands in
 * for `renameSync`.
 */
export function tryAcquirePid(dir, trace = false, stderr = process.stderr, rename = renameSync) {
  const parent = dirname(dir);
  if (!existsSync(parent)) mkdirSync(parent, { recursive: true });
  const token = uniqueToken();
  const owner = LockOwner.current(token);
  let lastError = null;
  for (let attempt = 0; attempt < 4; attempt++) {
    const staging = `${dir}.${owner.pid}.${uniqueToken()}.tmp`;
    mkdirSync(staging);
    try {
      writeOwner(staging, owner);
      rename(staging, dir);
      return { acquired: new PidLock(dir, token) };
    } catch (error) {
      rmSync(staging, { recursive: true, force: true });
      if (!exists(dir)) {
        // Released between the rename and this check, or a real error.
        lastError = error;
        continue;
      }
    }
    const holder = readOwner(dir);
    const reason = staleReason(dir, holder);
    if (reason === null) return { held: holder };
    if (trace) stderr.write(`[TRACE] Breaking stale lock ${dir}: ${reason}\n`);
    breakStale(dir, holder, rename);
  }
  if (lastError !== null) throw lastError;
  return { held: readOwner(dir) };
}

function describePidHolder(dir, owner) {
  return (
    owner?.describe() ??
    `a directory without an owner record (removed once ${dir} is ${UNOWNED_GRACE_MS / 1000} s old)`
  );
}

/** Waits from 1 ms, doubling up to 50 ms, until `deadline` (a `performance.now()`). */
class Backoff {
  constructor(deadline) {
    this.delay = 1;
    this.deadline = deadline;
  }

  /** Sleeps before the next attempt; false once the deadline has passed. */
  async wait() {
    const now = performance.now();
    if (now >= this.deadline) return false;
    await sleep(Math.min(this.delay, this.deadline - now));
    this.delay = Math.min(this.delay * 2, 50);
    return true;
  }
}

function lockedError(path, kind, holder, options) {
  return new LockError(
    `store is locked: ${path} is held by ${holder}; gave up waiting for the ${kind} lock ` +
      `after ${options.timeoutText} ms (--lock-timeout)`,
  );
}

/** `<db>.lock.owner`: the record of the exclusive `flock` holder. */
export const ownerFilePath = (store) => `${lockFilePath(store)}.owner`;

/** Who holds the `<db>.lock` of `store`: shared readers, or the writer its record names. */
function flockHolder(store) {
  let probe = null;
  try {
    probe = FileLock.tryAcquire(lockFilePath(store), LockMode.Shared);
  } catch {
    // Judged by the record below.
  }
  if (probe !== null) {
    probe.release();
    return 'shared readers';
  }
  let owner = null;
  try {
    owner = LockOwner.parse(readFileSync(ownerFilePath(store), 'utf8'));
  } catch {
    // A missing record names no one.
  }
  return owner === null
    ? 'an exclusive writer that left no owner record'
    : `an exclusive writer: ${owner.describe()}`;
}

const removeFile = (path) => {
  try {
    unlinkSync(path);
  } catch {
    // Already gone, or for the next holder to remove.
  }
};

/**
 * Replaces the record at `path` at once, so that no reader sees half of it.
 * Best effort: the record only serves diagnostics.
 */
export function writeRecord(path, record) {
  const staging = `${path}.${uniqueToken()}.tmp`;
  try {
    writeFileSync(staging, record);
    renameSync(staging, path);
  } catch {
    removeFile(staging);
  }
}

/**
 * An exclusive or shared `flock` on `<db>.lock`. An exclusive holder keeps
 * its owner record in `<db>.lock.owner` and removes it on release.
 */
class FlockGuard {
  constructor(lock, owner) {
    this.lock = lock;
    this.owner = owner;
  }

  path() {
    return this.lock.path();
  }

  release() {
    if (this.lock.mode() === LockMode.Exclusive) removeFile(this.owner);
    this.lock.release();
  }
}

async function acquireFlock(store, mode, deadline, options) {
  const path = lockFilePath(store);
  const kind = mode === LockMode.Exclusive ? 'exclusive flock' : 'shared flock';
  const backoff = new Backoff(deadline);
  let reported = false;
  for (;;) {
    const lock = FileLock.tryAcquire(path, mode);
    if (lock !== null) {
      const owner = ownerFilePath(store);
      if (mode === LockMode.Exclusive) writeRecord(owner, LockOwner.current(uniqueToken()).toRecord());
      // No exclusive holder exists, so a record is a crashed one's.
      else removeFile(owner);
      if (options.trace) options.stderr.write(`[TRACE] Acquired ${kind} lock ${path}\n`);
      return new FlockGuard(lock, owner);
    }
    if (options.trace && !reported) {
      reported = true;
      options.stderr.write(`[TRACE] Waiting for ${kind} lock ${path}: held by ${flockHolder(store)}\n`);
    }
    if (!(await backoff.wait())) throw lockedError(path, kind, flockHolder(store), options);
  }
}

async function acquirePid(store, deadline, options) {
  const dir = lockDirPath(store);
  const backoff = new Backoff(deadline);
  let reported = false;
  for (;;) {
    let attempt;
    try {
      attempt = tryAcquirePid(dir, options.trace, options.stderr);
    } catch (error) {
      throw ioError(error);
    }
    if (attempt.acquired !== undefined) {
      if (options.trace) options.stderr.write(`[TRACE] Acquired pid lock ${dir}\n`);
      return attempt.acquired;
    }
    const holder = describePidHolder(dir, attempt.held);
    if (options.trace && !reported) {
      reported = true;
      options.stderr.write(`[TRACE] Waiting for pid lock ${dir}: held by ${holder}\n`);
    }
    if (!(await backoff.wait())) throw lockedError(dir, 'pid', holder, options);
  }
}

/** Locks held for one invocation; `release()` releases them. */
export class StoreLock {
  constructor(guards) {
    this.guards = guards;
  }

  /**
   * Locks every store in `stores` (see `lockOrder`), waiting up to
   * `options.timeout` milliseconds for each. `--lock pid` is always
   * exclusive. `options` is a `parseStoreLockOptions` result with `trace`
   * and `stderr`.
   */
  static async acquire(stores, mode, options) {
    const settings = { trace: false, stderr: process.stderr, ...options };
    let { kind } = settings;
    if (kind === StoreLockKind.Flock && !settings.explicit && stores.length > 0) {
      const { unavailable } = lockBackend();
      if (unavailable !== undefined) {
        kind = StoreLockKind.Pid;
        if (settings.trace) {
          settings.stderr.write(`[TRACE] Locking with --lock pid: ${unavailable}\n`);
        }
      }
    }
    const deadline = performance.now() + settings.timeout;
    const lock = new StoreLock([]);
    try {
      for (const store of stores) {
        if (kind === StoreLockKind.Flock) {
          lock.guards.push(await acquireFlock(store, mode, deadline, settings));
        } else if (kind === StoreLockKind.Pid) {
          lock.guards.push(await acquirePid(store, deadline, settings));
        }
      }
    } catch (error) {
      lock.release();
      throw error;
    }
    return lock;
  }

  /** The lock files and directories held. */
  paths() {
    return this.guards.map((guard) => guard.path());
  }

  /** Releases the locks, the last taken first. */
  release() {
    for (const guard of this.guards.splice(0).reverse()) guard.release();
  }

  [Symbol.dispose]() {
    this.release();
  }
}

/**
 * `--lock-status`: one line per store and lock kind, without taking or
 * creating any lock that is not already there.
 */
export function lockStatus(stores) {
  let report = '';
  for (const store of stores) {
    const path = lockFilePath(store);
    let state = 'unlocked';
    if (existsSync(path)) {
      try {
        const probe = FileLock.tryAcquire(path, LockMode.Exclusive);
        if (probe === null) state = `locked by ${flockHolder(store)}`;
        else probe.release();
      } catch (error) {
        state = `unknown (${error.message})`;
      }
    }
    report += `flock ${path}: ${state}\n`;
    const dir = lockDirPath(store);
    state = 'unlocked';
    if (exists(dir)) {
      const owner = readOwner(dir);
      state = `locked by ${describePidHolder(dir, owner)}`;
      const reason = staleReason(dir, owner);
      if (reason !== null) state += `; stale, the next writer breaks it: ${reason}`;
    }
    report += `pid ${dir}: ${state}\n`;
  }
  return report;
}

/**
 * Keeps `node:ffi`'s "FFI is an experimental feature" warning, which the
 * first `flock` of a Node.js 26 process prints, off the standard error of
 * the `clink` executable, as the Rust `clink` prints none.
 */
export function quietFfiWarning(target = process) {
  const emitWarning = target.emitWarning;
  target.emitWarning = function (warning, ...rest) {
    const text = typeof warning === 'string' ? warning : warning?.message;
    if (String(text).startsWith('FFI is an experimental feature')) return undefined;
    return emitWarning.call(this, warning, ...rest);
  };
}
