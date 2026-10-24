// Synchronous log adapters compose identically in every WASM host. Adapters
// that need asynchronous I/O must provide a server-side transaction engine.
const complete = (value) => {
  if (value !== undefined) throw new TypeError('log operations must complete synchronously and return undefined');
};
const entries = (log) => {
  const records = JSON.parse(log.read());
  if (!Array.isArray(records) || records.some((record) => typeof record !== 'string'))
    throw new TypeError('log read must return a JSON array of strings');
  return records;
};

export class MemoryCommitLog {
  #records;
  constructor(records = []) { this.#records = [...records]; }
  get supportsDurableFlush() { return false; }
  read() { return JSON.stringify(this.#records); }
  append(record) { this.#records.push(record); }
  flush() {}
}

/** Fixed-primary replication; does not perform elections or automatic failover. */
export class ReplicatedCommitLog {
  #primary;
  #replicas;
  #acknowledgements;
  #trace;
  constructor(primary, replicas, { acknowledgements = null, trace } = {}) {
    this.#primary = primary;
    this.#replicas = [...replicas];
    if (this.#replicas.includes(primary) || new Set(this.#replicas).size !== this.#replicas.length)
      throw new TypeError('replica membership must contain distinct log instances');
    if (acknowledgements !== null && (!Number.isInteger(acknowledgements) || acknowledgements < 1 || acknowledgements > this.#replicas.length + 1))
      throw new RangeError('acknowledgements must fit the fixed membership');
    this.#acknowledgements = acknowledgements;
    this.#trace = trace;
  }
  get supportsDurableFlush() {
    return this.#primary.supportsDurableFlush === true &&
      (this.#acknowledgements === null || this.#acknowledgements === 1 || this.#replicas.every((log) => log.supportsDurableFlush === true));
  }
  read() { return this.#primary.read(); }
  append(record) { complete(this.#primary.append(record)); }
  flush() {
    complete(this.#primary.flush());
    if (this.#acknowledgements !== null && this.synchronize() < this.#acknowledgements)
      throw new Error('indeterminate replication outcome; reopen the engine');
  }
  synchronize() {
    complete(this.#primary.flush());
    const records = entries(this.#primary);
    let count = 1;
    for (const replica of this.#replicas) {
      try {
        const previous = entries(replica);
        if (previous.length > records.length || previous.some((record, i) => record !== records[i]))
          throw new Error('replica diverges from primary');
        for (const record of records.slice(previous.length)) complete(replica.append(record));
        complete(replica.flush()); count++;
      } catch (error) { try { this.#trace?.(String(error)); } catch { /* Diagnostics cannot change the storage outcome. */ } }
    }
    return count;
  }
}
