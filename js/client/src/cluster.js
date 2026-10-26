// Optional network coordination. The local WASM engine remains a private
// workspace; only an atomic coordinator CAS publishes its journals.
import { WasmEngine } from './engine.js';
import { LinksClient } from './client.js';
import { ConnectionClosedError, LimitExceededError, LinksError, UnsupportedError } from './errors.js';

const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true });
function base64(text) {
  let bytes = ''; for (const byte of encoder.encode(text)) bytes += String.fromCharCode(byte);
  return btoa(bytes);
}
function unbase64(text) {
  if (typeof text !== 'string' || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(text)) throw new TypeError('invalid coordinator base64');
  const decoded = atob(text);
  if (btoa(decoded) !== text) throw new TypeError('noncanonical coordinator base64');
  return decoder.decode(Uint8Array.from(decoded, (c) => c.charCodeAt(0)));
}
function keysOf(keys) {
  if (!Array.isArray(keys) || keys.length < 1 || keys.length > 32 || new Set(keys).size !== keys.length ||
      keys.some((key) => typeof key !== 'string' || !/^[A-Za-z0-9_.-]{1,128}$/.test(key)))
    throw new TypeError('select 1..32 distinct shard names using letters, numbers, dot, underscore or dash');
  return [...keys];
}
function records(value) {
  if (!Array.isArray(value) || value.some((entry) => typeof entry !== 'string')) throw new TypeError('journal must be an array of strings');
  return value;
}

/** Uses etcd's JSON gateway. Every read is one linearizable transaction;
 * writes compare every selected key, including read-only shards. No mutation
 * is retried after an I/O error. The caller supplies a trusted etcd deployment.
 */
export class EtcdJournalStore {
  #endpoints; #prefix; #fetch; #headers; #timeout; #requestLimit; #responseLimit; #cursor = 0;
  supportsDurableFlush = true;
  constructor(endpoints, { prefix = '/link-cli/engine1/', fetch = globalThis.fetch, headers = {}, timeoutMs = 10000,
    maxRequestBytes = 1048576, maxResponseBytes = 8388608 } = {}) {
    if (!Array.isArray(endpoints) || endpoints.length === 0) throw new TypeError('provide at least one etcd endpoint');
    this.#endpoints = endpoints.map((endpoint) => {
      const url = new URL(endpoint);
      if (!['http:', 'https:'].includes(url.protocol) || url.search || url.hash) throw new TypeError('etcd endpoints must be HTTP(S) URLs');
      return url.href.replace(/\/$/, '');
    });
    if (typeof prefix !== 'string' || !prefix.endsWith('/') || prefix.length < 2 || typeof fetch !== 'function') throw new TypeError('provide a nonempty prefix ending in / and a fetch implementation');
    for (const value of [timeoutMs, maxRequestBytes, maxResponseBytes]) if (!Number.isSafeInteger(value) || value <= 0) throw new TypeError('timeouts and byte limits must be positive safe integers');
    this.#prefix = prefix; this.#fetch = fetch; this.#headers = { ...headers }; this.#timeout = timeoutMs;
    this.#requestLimit = maxRequestBytes; this.#responseLimit = maxResponseBytes;
  }
  async #post(body, outcome) {
    const text = JSON.stringify(body);
    if (encoder.encode(text).length > this.#requestLimit) throw new LimitExceededError('coordinator request exceeds maxRequestBytes', { outcome: 'not-sent' });
    try {
      const response = await this.#fetch(`${this.#endpoints[this.#cursor]}/v3/kv/txn`, {
        method: 'POST', redirect: 'error', headers: { ...this.#headers, 'Content-Type': 'application/json' }, body: text,
        signal: AbortSignal.timeout(this.#timeout),
      });
      if (!response.ok) throw new Error(`etcd returned HTTP ${response.status}`);
      const reader = response.body.getReader(); const chunks = []; let length = 0;
      try {
        for (;;) {
          const { done, value } = await reader.read(); if (done) break;
          length += value.length;
          if (length > this.#responseLimit) throw new LimitExceededError('coordinator response exceeds maxResponseBytes', { outcome });
          chunks.push(value);
        }
      } finally { await reader.cancel(); reader.releaseLock(); }
      const bytes = new Uint8Array(length); let offset = 0;
      for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.length; }
      return JSON.parse(decoder.decode(bytes));
    } catch (cause) {
      if (cause instanceof LinksError) throw cause;
      throw new LinksError(`coordinator request failed: ${cause}`, { outcome, cause });
    }
  }
  async read(keys) {
    keys = keysOf(keys);
    let failure;
    for (let attempt = 0; attempt < this.#endpoints.length; attempt++) {
      try {
        const result = await this.#post({ success: keys.map((key) => ({ request_range: { key: base64(this.#prefix + key) } })) }, 'not-sent');
        if (result.succeeded !== true || !Array.isArray(result.responses) || result.responses.length !== keys.length) throw new Error('invalid coordinator snapshot');
        return result.responses.map((response, index) => {
          const range = response.response_range;
          if (!range || (range.more !== undefined && range.more !== false) || (range.kvs !== undefined && !Array.isArray(range.kvs))) throw new Error('incomplete coordinator snapshot');
          const values = range.kvs ?? [];
          if (values.length === 0) return { generation: '0', entries: [] };
          const [value] = values;
          if (values.length !== 1 || value.key !== base64(this.#prefix + keys[index]) ||
              typeof value.mod_revision !== 'string' || !/^[1-9][0-9]*$/.test(value.mod_revision) || BigInt(value.mod_revision) > 9223372036854775807n)
            throw new Error('invalid coordinator generation or key');
          return { generation: value.mod_revision, entries: records(JSON.parse(unbase64(value.value))) };
        });
      } catch (cause) { failure = cause; this.#cursor = (this.#cursor + 1) % this.#endpoints.length; }
    }
    throw failure;
  }
  async compareExchange(keys, expected, updates) {
    keys = keysOf(keys);
    if (expected.length !== keys.length || updates.length !== keys.length) throw new TypeError('coordinator arrays must match selected shards');
    const compare = keys.map((key, index) => {
      const generation = expected[index].generation;
      if (typeof generation !== 'string' || !/^(0|[1-9][0-9]*)$/.test(generation) || BigInt(generation) > 9223372036854775807n) throw new TypeError('invalid expected generation');
      return { key: base64(this.#prefix + key), target: 'MOD', result: 'EQUAL', mod_revision: generation };
    });
    const success = keys.flatMap((key, index) => updates[index] === null ? [] : [{ request_put: {
      key: base64(this.#prefix + key), value: base64(JSON.stringify(records(updates[index]))),
    } }]);
    const result = await this.#post({ compare, success }, 'unknown');
    // ProtoJSON omits the default false field. A valid response header is
    // required before interpreting omission as a rejected comparison.
    if (typeof result.header?.revision !== 'string' || !/^[1-9][0-9]*$/.test(result.header.revision) || BigInt(result.header.revision) > 9223372036854775807n ||
        (result.succeeded !== undefined && typeof result.succeeded !== 'boolean')) throw new LinksError('indeterminate coordinator response; read fresh journals to discover the outcome', { outcome: 'unknown' });
    return result.succeeded === true;
  }
}

/** Independent workers share a coordinator, never a stale local cache. Shards
 * have independent address spaces. Cross-shard atomicity is supplied by one
 * coordinator transaction, with conservative serializable validation.
 */
export class ClusterEngine {
  #binding; #store; #options; #journalCheckpointInterval;
  constructor(binding, store, options = {}) {
    if (typeof store?.read !== 'function' || typeof store?.compareExchange !== 'function') throw new TypeError('expected an atomic journal store');
    if (options.concurrency && options.concurrency !== 'optimistic') throw new UnsupportedError('network sessions use optimistic admission');
    if (options.isolation && options.isolation !== 'serializable') throw new UnsupportedError('network sessions use serializable snapshot validation');
    if (options.requireDurableFlush && store.supportsDurableFlush !== true) throw new UnsupportedError('coordinator does not support durable flush');
    const { journalCheckpointInterval, ...engineOptions } = options;
    if (journalCheckpointInterval !== undefined && (!Number.isSafeInteger(journalCheckpointInterval) || journalCheckpointInterval <= 0))
      throw new TypeError('journalCheckpointInterval must be a positive safe integer');
    this.#journalCheckpointInterval = journalCheckpointInterval;
    this.#binding = binding; this.#store = store; this.#options = { ...engineOptions, requireDurableFlush: false };
  }
  async begin(keys) {
    keys = keysOf(keys);
    const snapshot = await this.#store.read(keys);
    if (!Array.isArray(snapshot) || snapshot.length !== keys.length) throw new LinksError('incomplete coordinator snapshot', { outcome: 'not-sent' });
    return ClusterTransaction.open(this.#binding, this.#store, this.#options, keys, snapshot, this.#journalCheckpointInterval);
  }
}

export class ClusterTransaction {
  #store; #keys; #snapshot; #work = []; #clients = []; #state = 'open'; #pending; #journalCheckpointInterval;
  static async open(binding, store, options, keys, snapshot, journalCheckpointInterval) {
    const transaction = new ClusterTransaction();
    transaction.#store = store; transaction.#keys = keys;
    transaction.#journalCheckpointInterval = journalCheckpointInterval;
    transaction.#snapshot = snapshot.map((item) => ({ generation: item.generation, entries: [...records(item.entries)] }));
    try {
      for (const item of transaction.#snapshot) {
        const engine = new WasmEngine(binding, options, records(item.entries));
        const work = { engine, client: null }; transaction.#work.push(work);
        work.client = engine.begin();
        const transport = work.client.transport;
        transaction.#clients.push(new LinksClient({
          capabilities: { ...transport.capabilities, durableFlush: store.supportsDurableFlush === true,
            persistent: store.supportsDurableFlush === true, crashConsistency: store.supportsDurableFlush === true },
          send: (text, scope) => { transaction.#check(); return transport.send(text, scope); },
        }));
      }
      return transaction;
    } catch (error) { await transaction.close(); throw error; }
  }
  #check() { if (this.#state !== 'open') throw new ConnectionClosedError('cluster transaction is closed', { outcome: 'not-sent' }); }
  shard(key) {
    this.#check(); const index = this.#keys.indexOf(key);
    if (index < 0) throw new TypeError('shard was not selected at begin');
    return this.#clients[index];
  }
  async commit() {
    this.#check(); this.#state = 'committing';
    this.#pending = this.#commit();
    return this.#pending;
  }
  async #commit() {
    let failed = false;
    try {
      const updates = [];
      for (let index = 0; index < this.#work.length; index++) {
        const { engine, client } = this.#work[index];
        await client.commit();
        const journal = engine.journal();
        // Check before compaction: equal/shrinking lengths still publish writes.
        if (journal.length === this.#snapshot[index].entries.length) updates.push(null);
        else updates.push(this.#journalCheckpointInterval !== undefined &&
          engine.snapshot().revision % BigInt(this.#journalCheckpointInterval) === 0n ? engine.exportCheckpoint() : journal);
      }
      let published;
      try { published = await this.#store.compareExchange(this.#keys, this.#snapshot, updates); }
      catch (cause) {
        if (cause instanceof LinksError) throw cause;
        throw new LinksError('indeterminate coordinated commit; read fresh journals to discover the outcome', { outcome: 'unknown', cause });
      }
      if (!published) throw new LinksError('conflict: a selected shard changed since begin', { outcome: 'failed' });
    } catch (cause) { failed = true; throw cause; }
    finally {
      this.#state = 'closed';
      try { await this.#dispose(); } catch (cause) { if (!failed) throw cause; }
    }
  }
  async #dispose() {
    let failure;
    for (const { engine, client } of this.#work) {
      try { await client?.close(); } catch (error) { failure ??= error; }
      try { engine.close(); } catch (error) { failure ??= error; }
    }
    if (failure) throw failure;
  }
  async rollback() {
    if (this.#state === 'open') { this.#state = 'closed'; this.#pending = this.#dispose(); }
    await this.#pending;
  }
  async close() { await this.rollback(); }
  async [Symbol.asyncDispose]() { await this.close(); }
}
