// Ordinary mutation runs in the actual u64 Rust workspace. Host publication
// uses one explicit-token coherent batch; per-row exchange is not group atomic.
import { normalizeCausalBatchEdits, validateCausalBatchToken } from './engine-causal-batch.js';
import { CausalVersion } from './engine-causal-version.js';
import { LinksClient } from './client.js';
import { wasmCapabilities } from './capabilities.js';
import { ConnectionClosedError } from './errors.js';
import { WasmTransport, transferHandle } from './wasm.js';

const own = value => structuredClone(value);
const closed = () => new ConnectionClosedError('the causal session is closed', { outcome: 'not-sent' });
export class CausalSession {
  #batch; #generation; #transport; #token; #preparing; #prepared = null;
  #committing; #result; #closed = false; #rows;
  static async open(binding, batch, first, last, policy, options = {}) {
    if (typeof binding?.CausalLinksWorkspace !== 'function') throw new TypeError('build clink-wasm with causal session support');
    if (typeof batch?.snapshot !== 'function' || typeof batch?.write !== 'function') throw new TypeError('expected a causal batch');
    const encodedOptions = JSON.stringify(options);
    const snapshot = await batch.snapshot();
    const native = new binding.CausalLinksWorkspace(JSON.stringify(snapshot.versions.map(CausalVersion.encode)),
      first, last, policy, encodedOptions);
    try { return new CausalSession(native, batch, snapshot.generation); }
    catch (error) { native.free?.(); throw error; }
  }
  constructor(native, batch, generation) {
    this.#transport = new WasmTransport(native, { owned: true, maxRequestBytes: 1048576 });
    this.#batch = batch; this.#generation = generation;
  }
  /** A standard links/query client over the same owned private workspace.
   * Preparation drains accepted requests and ends this client; close rolls back. */
  client() {
    return new LinksClient({
      capabilities: wasmCapabilities({ backend: 'clink-wasm-causal-session',
        addressBits: 64, maxAddress: 18446744073709551615n, losslessQueries: true,
        atomicTransactions: true, rollback: true,
        limits: { maxBytes: 1048576, maxDepth: 64, maxNodes: 4096 },
      }),
      send: (text, scope) => this.#send(['lino', text], scope),
      close: () => this.close(),
    });
  }
  #send(operation, scope) {
    if (this.#closed) return Promise.reject(closed());
    return this.#transport.send(JSON.stringify(operation), scope).then(JSON.parse);
  }
  createLink(source, target) { return this.#send(['create', source, target]); }
  ensureCreated(id) { return this.#send(['ensure', id]); }
  updateLink(id, source, target) { return this.#send(['update', id, source, target]); }
  deleteLink(id) { return this.#send(['delete', id]); }
  setName(id, name) { return this.#send(['set-name', id, name]); }
  removeName(id) { return this.setName(id, null); }
  rows() {
    return this.#rows && !this.#closed ? Promise.resolve(own(this.#rows)) : this.#send(['rows']);
  }
  getLink(id) {
    if (this.#rows && !this.#closed) {
      const row = this.#rows.find(row => row.index === id);
      return Promise.resolve(row ? { index: row.index, source: row.source, target: row.target } : null);
    }
    return this.#send(['get', id]);
  }
  getName(id) {
    return this.#rows && !this.#closed ? Promise.resolve(this.#rows.find(row => row.index === id)?.name ?? null)
      : this.#send(['get-name', id]);
  }
  getByName(name) {
    return this.#rows && !this.#closed ? Promise.resolve(this.#rows.find(row => row.name === name)?.index ?? null)
      : this.#send(['get-by-name', name]);
  }
  get prepared() { return own(this.#prepared); }
  #validateToken(token) {
    if (this.#closed) throw closed();
    validateCausalBatchToken(token, 0);
    if (this.#token !== undefined && this.#token !== token) throw new TypeError('a causal retry must keep its original token');
  }
  prepare(token) {
    try {
      this.#validateToken(token);
      if (!this.#preparing) {
        this.#token = token;
        // Reserve synchronously, before the first await. Earlier queued work
        // drains; later mutations cannot race the final difference.
        const transferred = this.#transport[transferHandle]();
        this.#preparing = (async () => {
          const native = await transferred;
          try {
            const edits = normalizeCausalBatchEdits(JSON.parse(native.finish()).map(edit =>
              ({ ...edit, observed: edit.observed.map(CausalVersion.decode) })));
            validateCausalBatchToken(token, edits.length);
            const rows = JSON.parse(native.request('["rows"]'));
            this.#rows = rows;
            this.#prepared = { token, generation: this.#generation, edits };
            return this.#prepared;
          } finally { native.free(); }
        })();
      }
      return this.#preparing.then(own);
    } catch (error) { return Promise.reject(error); }
  }
  commit(token) {
    // Refused calls must not occupy the pending slot and make an immediately
    // following valid retry inherit their rejection.
    try { this.#validateToken(token); } catch (error) { return Promise.reject(error); }
    const preparing = this.prepare(token);
    if (this.#committing) {
      const current = this.#committing;
      return preparing.then(() => current).then(own);
    }
    const pending = preparing.then(async request => {
      if (this.#result === undefined) this.#result = own(await this.#batch.write(request.token, request.generation, request.edits));
      return this.#result;
    });
    this.#committing = pending;
    pending.then(() => { if (this.#committing === pending) this.#committing = undefined; },
      () => { if (this.#committing === pending) this.#committing = undefined; });
    return pending.then(own);
  }
  async close() {
    this.#closed = true;
    try { await this.#committing; } catch { /* The original caller retains the publication outcome. */ }
    try { await this.#preparing; } catch { /* Preparation already released its native handle. */ }
    await this.#transport.close();
    this.#rows = undefined; this.#result = undefined;
  }
  rollback() { return this.close(); }
}
