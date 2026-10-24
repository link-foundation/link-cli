// The engine is compiled from the Rust core, so native and browser sessions
// use identical isolation, history, constraints and journal validation.
import { LinksClient } from './client.js';
import { WasmTransport } from './wasm.js';
import { toAddress } from './operations.js';
import { ConnectionClosedError } from './errors.js';

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
    if (typeof binding?.StagedEngine !== 'function') throw new TypeError('build clink-wasm with StagedEngine support');
    const durable = !Array.isArray(journal) && journal.supportsDurableFlush === true;
    this.#native = Array.isArray(journal)
      ? new binding.StagedEngine(JSON.stringify(options), JSON.stringify(journal))
      : binding.StagedEngine.withLog(JSON.stringify(options), journal);
    this.#capabilities = { durableFlush: durable, persistent: durable };
  }
  #check() {
    if (this.#closed) throw new ConnectionClosedError('the engine is closed', { outcome: 'not-sent' });
  }
  begin() { this.#check(); return new EngineClient(this.#native.begin(), this.#capabilities); }
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
