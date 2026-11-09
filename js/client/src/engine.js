// The engine is compiled from the Rust core, so native and browser sessions
// use identical isolation, history, constraints and journal validation.
import { LinksClient } from './client.js';
import { WasmTransport, canTransferHandle, transferHandle } from './wasm.js';
import { toAddress } from './operations.js';
import { ConnectionClosedError, LinksError, UnsupportedError } from './errors.js';
import { LockWaits } from './engine-lock-wait.js';

const nativeHandle = Symbol('native engine handle');
const sessionOwners = new WeakMap();
const optionalAddress = (revision) => (revision === undefined ? undefined : toAddress(revision));
function capabilities(journal) {
  const durable = !Array.isArray(journal) && journal.supportsDurableFlush === true;
  return { durableFlush: durable, persistent: durable };
}
/** Splits what the Rust core reads from what this client implements on top of it. */
function selected(options, journal) {
  const { lockWaitMs, ...core } = options;
  // Unchanged text keeps the core's refusal of options that are not an object.
  if (lockWaitMs === undefined) return { text: JSON.stringify(options), ...capabilities(journal) };
  if (core.concurrency !== 'pessimistic')
    throw new TypeError('lock waiting requires pessimistic concurrency');
  if (!Number.isFinite(lockWaitMs) || lockWaitMs < 0)
    throw new TypeError('lockWaitMs must be a non-negative number of milliseconds');
  return {
    text: JSON.stringify(core),
    ...capabilities(journal),
    lockWaits: new LockWaits(lockWaitMs),
  };
}

function journalCheck(check) {
  return {
    validRecords: check.validRecords,
    totalRecords: check.totalRecords,
    revision: BigInt(check.revision),
    damage: check.damage,
  };
}

/** An isolated LinksClient session. Close rolls back an uncommitted session. */
export class EngineClient extends LinksClient {
  #lockWaits;
  constructor(session, { durableFlush = false, persistent = false, lockWaits } = {}) {
    if (typeof session?.commit !== 'function' || typeof session?.rollback !== 'function')
      throw new TypeError('expected a staged engine session with commit and rollback');
    if (lockWaits && typeof session.takeWriteBlocker !== 'function')
      throw new UnsupportedError('this WebAssembly session cannot wait for write addresses');
    super(
      new WasmTransport(session, {
        owned: true,
        capabilities: {
          backend: 'clink-wasm-engine',
          atomicTransactions: true,
          rollback: true,
          // MemoryLog is volatile. Exporting its bytes does not make them durable.
          persistent,
          durableFlush,
          crashConsistency: durableFlush,
          duplicateLinks: true,
        },
        waiter: lockWaits && ((...request) => lockWaits.request(...request)),
      }),
    );
    this.#lockWaits = lockWaits;
  }
  #ending(outcome) {
    return this.#lockWaits ? outcome.finally(() => this.#lockWaits.wakeAll()) : outcome;
  }
  commit() {
    return this.#ending(this.transport.control('commit'));
  }
  rollback() {
    return this.#ending(this.transport.control('rollback'));
  }
  close() {
    return this.#ending(super.close());
  }
  /** Remembers the staged writes; resolves to a token for this session. */
  savepoint() {
    return this.transport.control('savepoint');
  }
  /** Restores staged writes only: reads stay validated and addresses stay retired. */
  rollbackTo(savepoint) {
    return this.transport.control('rollbackTo', savepoint);
  }
  releaseSavepoint(savepoint) {
    return this.transport.control('releaseSavepoint', savepoint);
  }
}

/** Pass the initialized clink-wasm module. No Node.js dependency is imported. */
export class WasmEngine {
  #native;
  #closed = false;
  #capabilities;
  constructor(binding, options = {}, journal = []) {
    if (binding === nativeHandle) {
      this.#native = options;
      this.#capabilities = journal;
      return;
    }
    if (typeof binding?.StagedEngine !== 'function')
      throw new TypeError('build clink-wasm with StagedEngine support');
    const { text, ...chosen } = selected(options, journal);
    this.#capabilities = chosen;
    this.#native = Array.isArray(journal)
      ? new binding.StagedEngine(text, JSON.stringify(journal))
      : binding.StagedEngine.withLog(text, journal);
  }
  #check() {
    if (this.#closed)
      throw new ConnectionClosedError('the engine is closed', { outcome: 'not-sent' });
  }
  begin() {
    this.#check();
    const client = new EngineClient(this.#native.begin(), this.#capabilities);
    sessionOwners.set(client, this);
    return client;
  }
  /** Consumes sessions after draining queued requests. Results retain input
   * order; each accepted write has its own revision and shares one log flush. */
  async commitGroup(sessions) {
    this.#check();
    if (!Array.isArray(sessions)) throw new TypeError('group members must be an array');
    const members = [...sessions];
    if (
      new Set(members).size !== members.length ||
      members.some(
        (client) => sessionOwners.get(client) !== this || !client.transport[canTransferHandle](),
      )
    ) {
      throw new TypeError('group members must be distinct open sessions of this engine');
    }
    // Allocate first: allocation failure leaves every member usable. This
    // native handle also keeps the engine alive if close runs while draining.
    const group = this.#native.group();
    const handles = [];
    try {
      // Reservations happen synchronously before the first await. Each transfer
      // waits only on its own queue, whose errors cannot break the tail.
      const transferred = await Promise.all(
        members.map((client) => client.transport[transferHandle]()),
      );
      handles.push(...transferred);
      while (handles.length) group.add(handles.shift());
      return JSON.parse(group.commit()).map((outcome) =>
        outcome.status === 'committed'
          ? { status: outcome.status, revision: BigInt(outcome.revision) }
          : outcome,
      );
    } finally {
      for (const handle of handles) handle.free();
      group.free();
      this.#capabilities.lockWaits?.wakeAll();
    }
  }
  fork(options = {}, journal = []) {
    this.#check();
    const { text, ...chosen } = selected(options, journal);
    const branch = Array.isArray(journal)
      ? this.#native.fork(text, JSON.stringify(journal))
      : this.#native.forkWithLog(text, journal);
    return new WasmBranch(branch, chosen);
  }
  resumeBranch(journal, mergeBaseRecord, options = {}) {
    this.#check();
    const { text, ...chosen } = selected(options, journal);
    const branch = Array.isArray(journal)
      ? this.#native.resume(text, JSON.stringify(journal), mergeBaseRecord)
      : this.#native.resumeWithLog(text, journal, mergeBaseRecord);
    return new WasmBranch(branch, chosen);
  }
  snapshot(revision) {
    this.#check();
    const value = JSON.parse(this.#native.snapshot(optionalAddress(revision)));
    return {
      revision: BigInt(value.revision),
      rows: value.rows.map((row) => ({
        ...row,
        index: BigInt(row.index),
        source: BigInt(row.source),
        target: BigInt(row.target),
      })),
    };
  }
  journal() {
    this.#check();
    return JSON.parse(this.#native.journal());
  }
  /** Independent retained snapshots, optionally only up to one retained revision.
   * The caller owns publication and durability. */
  exportCheckpoint(revision) {
    this.#check();
    return JSON.parse(this.#native.exportCheckpoint(optionalAddress(revision)));
  }
  /** Copies a consistent cut to an empty log and verifies it, then returns the
   * revision it ends at. Commits continue while the log is called. */
  backup(destination, { revision } = {}) {
    this.#check();
    return this.#native.backup(destination, optionalAddress(revision));
  }
  /** Appends only the revisions an earlier backup lacks, as changed rows, then
   * returns the revision it ends at. An empty log gets a full copy. */
  backupIncremental(destination) {
    this.#check();
    return this.#native.backupIncremental(destination);
  }
  /** Copies an archived journal to an empty log, stopping at one revision, verifies
   * it and returns the revision it ends at. No revision copies the whole archive.
   * Opening the destination gives the engine at that revision, on a new timeline. */
  static restoreBackup(binding, source, destination, { revision } = {}) {
    if (typeof binding?.StagedEngine?.restoreBackup !== 'function')
      throw new TypeError('build clink-wasm with restoreBackup support');
    return binding.StagedEngine.restoreBackup(source, destination, optionalAddress(revision));
  }
  /** The longest prefix of `journal` that opening would replay, and the first
   * damaged record. Never throws for damage: it is part of the report. */
  static checkJournal(binding, journal, options = {}) {
    if (typeof binding?.StagedEngine?.checkJournal !== 'function')
      throw new TypeError('build clink-wasm with checkJournal support');
    const { text } = selected(options, []);
    return journalCheck(
      JSON.parse(binding.StagedEngine.checkJournal(text, JSON.stringify(journal))),
    );
  }
  /** The longest valid prefix among copies of one journal (the damaged primary
   * first, then replicas, mirrors or a restored backup). Copies whose valid
   * prefixes disagree are refused rather than guessed between. */
  static recoverJournal(binding, copies, options = {}) {
    if (typeof binding?.StagedEngine?.recoverJournal !== 'function')
      throw new TypeError('build clink-wasm with recoverJournal support');
    const { text } = selected(options, []);
    const recovery = JSON.parse(binding.StagedEngine.recoverJournal(text, JSON.stringify(copies)));
    return {
      entries: recovery.entries,
      source: recovery.source,
      revision: BigInt(recovery.revision),
      checks: recovery.checks.map(journalCheck),
    };
  }
  /** Atomically publish under the commit lock; an unknown outcome requires reopen. */
  compactJournal() {
    this.#check();
    return this.#native.compactJournal();
  }
  /** Compacts once this many revisions have landed since the last compaction by
   * this owner, and otherwise returns false. A log that cannot be replaced is
   * refused either way. The first call after opening counts from revision 0. */
  compactJournalAfter(commits) {
    this.#check();
    return this.#native.compactJournalAfter(toAddress(commits, 'commits'));
  }
  /** Calls compactJournalAfter every periodMs until stop(). JavaScript has one
   * thread here, so each pass that compacts delays other work on it. The timer does
   * not keep the process alive and ends with the engine. The first failure ends it,
   * and stop() throws that failure. */
  compactEvery({ commits, periodMs }) {
    this.#check();
    const count = toAddress(commits, 'commits');
    if (count === 0n) throw new TypeError('compaction needs at least one commit');
    if (!Number.isFinite(periodMs) || periodMs <= 0)
      throw new TypeError('periodMs must be a positive number of milliseconds');
    // Nothing is due yet at this distance; this only checks the selection.
    this.#native.compactJournalAfter(0xffffffffffffffffn);
    let failure;
    const timer = setInterval(() => {
      if (this.#closed) return clearInterval(timer);
      try {
        this.#native.compactJournalAfter(count);
      } catch (cause) {
        failure = { cause };
        clearInterval(timer);
      }
    }, periodMs);
    timer.unref?.();
    return {
      stop() {
        clearInterval(timer);
        if (failure) throw failure.cause;
      },
    };
  }
  /** Existing sessions own independent handles; close each to release them. */
  close() {
    if (!this.#closed) {
      this.#closed = true;
      this.#native.free();
    }
  }
  [Symbol.dispose]() {
    this.close();
  }
}

/** Owns a branch handle; its journal adapter remains caller-owned. */
export class WasmBranch {
  #native;
  #closed = false;
  constructor(branch, selected) {
    this.#native = branch;
    this.engine = new WasmEngine(nativeHandle, branch.engine(), selected);
  }
  #check() {
    if (this.#closed)
      throw new ConnectionClosedError('the branch is closed', { outcome: 'not-sent' });
  }
  mergeBaseRecord() {
    this.#check();
    return this.#native.mergeBaseRecord();
  }
  merge() {
    this.#check();
    try {
      return this.#native.merge();
    } catch (cause) {
      throw new LinksError(String(cause), {
        outcome: String(cause).includes('indeterminate commit') ? 'unknown' : 'failed',
        cause,
      });
    }
  }
  close() {
    if (!this.#closed) {
      this.#closed = true;
      this.engine.close();
      this.#native.free();
    }
  }
  [Symbol.dispose]() {
    this.close();
  }
}
