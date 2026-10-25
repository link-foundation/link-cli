// The engine is compiled from the Rust core, so native and browser sessions
// use identical isolation, history, constraints and journal validation.
import { LinksClient } from './client.js';
import { WasmTransport, canTransferHandle, transferHandle } from './wasm.js';
import { toAddress } from './operations.js';
import { ConnectionClosedError, LinksError } from './errors.js';

const nativeHandle = Symbol('native engine handle');
const sessionOwners = new WeakMap();
function capabilities(journal) {
  const durable = !Array.isArray(journal) && journal.supportsDurableFlush === true;
  return { durableFlush: durable, persistent: durable };
}

/** An isolated LinksClient session. Close rolls back an uncommitted session. */
export class EngineClient extends LinksClient {
  constructor(session, { durableFlush = false, persistent = false } = {}) {
    if (typeof session?.commit !== 'function' || typeof session?.rollback !== 'function')
      throw new TypeError('expected a staged engine session with commit and rollback');
    super(new WasmTransport(session, { owned: true, capabilities: {
      backend: 'clink-wasm-engine', atomicTransactions: true, rollback: true,
      // MemoryLog is volatile. Exporting its bytes does not make them durable.
      persistent, durableFlush, crashConsistency: durableFlush,
      duplicateLinks: true,
    } }));
  }
  commit() { return this.transport.control('commit'); }
  rollback() { return this.transport.control('rollback'); }
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
    if (typeof binding?.StagedEngine !== 'function') throw new TypeError('build clink-wasm with StagedEngine support');
    this.#capabilities = capabilities(journal);
    this.#native = Array.isArray(journal)
      ? new binding.StagedEngine(JSON.stringify(options), JSON.stringify(journal))
      : binding.StagedEngine.withLog(JSON.stringify(options), journal);
  }
  #check() {
    if (this.#closed) throw new ConnectionClosedError('the engine is closed', { outcome: 'not-sent' });
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
    if (new Set(members).size !== members.length || members.some((client) =>
      sessionOwners.get(client) !== this || !client.transport[canTransferHandle]())) {
      throw new TypeError('group members must be distinct open sessions of this engine');
    }
    // Allocate first: allocation failure leaves every member usable. This
    // native handle also keeps the engine alive if close runs while draining.
    const group = this.#native.group();
    const handles = [];
    try {
      // Reservations happen synchronously before the first await. Each transfer
      // waits only on its own queue, whose errors cannot break the tail.
      const transferred = await Promise.all(members.map((client) => client.transport[transferHandle]()));
      handles.push(...transferred);
      while (handles.length) group.add(handles.shift());
      return JSON.parse(group.commit()).map((outcome) => outcome.status === 'committed'
        ? { status: outcome.status, revision: BigInt(outcome.revision) } : outcome);
    } finally {
      for (const handle of handles) handle.free();
      group.free();
    }
  }
  fork(options = {}, journal = []) {
    this.#check();
    const selected = capabilities(journal);
    const branch = Array.isArray(journal)
      ? this.#native.fork(JSON.stringify(options), JSON.stringify(journal))
      : this.#native.forkWithLog(JSON.stringify(options), journal);
    return new WasmBranch(branch, selected);
  }
  resumeBranch(journal, mergeBaseRecord, options = {}) {
    this.#check();
    const selected = capabilities(journal);
    const branch = Array.isArray(journal)
      ? this.#native.resume(JSON.stringify(options), JSON.stringify(journal), mergeBaseRecord)
      : this.#native.resumeWithLog(JSON.stringify(options), journal, mergeBaseRecord);
    return new WasmBranch(branch, selected);
  }
  snapshot(revision) {
    this.#check();
    const value = JSON.parse(this.#native.snapshot(revision === undefined ? undefined : toAddress(revision)));
    return {
      revision: BigInt(value.revision),
      rows: value.rows.map((row) => ({ ...row, index: BigInt(row.index), source: BigInt(row.source), target: BigInt(row.target) })),
    };
  }
  journal() { this.#check(); return JSON.parse(this.#native.journal()); }
  /** Existing sessions own independent handles; close each to release them. */
  close() { if (!this.#closed) { this.#closed = true; this.#native.free(); } }
  [Symbol.dispose]() { this.close(); }
}

/** Owns a branch handle; its journal adapter remains caller-owned. */
export class WasmBranch {
  #native;
  #closed = false;
  constructor(branch, selected) {
    this.#native = branch;
    this.engine = new WasmEngine(nativeHandle, branch.engine(), selected);
  }
  #check() { if (this.#closed) throw new ConnectionClosedError('the branch is closed', { outcome: 'not-sent' }); }
  mergeBaseRecord() { this.#check(); return this.#native.mergeBaseRecord(); }
  merge() {
    this.#check();
    try { return this.#native.merge(); }
    catch (cause) { throw new LinksError(String(cause), {
      outcome: String(cause).includes('indeterminate commit') ? 'unknown' : 'failed', cause,
    }); }
  }
  close() {
    if (!this.#closed) { this.#closed = true; this.engine.close(); this.#native.free(); }
  }
  [Symbol.dispose]() { this.close(); }
}
