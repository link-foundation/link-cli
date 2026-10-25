// Optional synchronous SQLite boundary; the universal client imports no provider.
const applicationId = 0x4c4e4b4a;
const schema = "CREATE TABLE link_cli_log (sequence INTEGER PRIMARY KEY CHECK(sequence>0), record BLOB NOT NULL CHECK(typeof(record)='blob'))";
const maxSequence = 9223372036854775807n;
const encoder = new TextEncoder();
const decoder = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });

function integer(value, name) {
  if (!Number.isInteger(value) || value < 0 || value > 2147483647) throw new RangeError(`${name} must be a nonnegative signed 32-bit integer`);
  return value;
}
function sequence(text) {
  if (typeof text !== 'string' || !/^(0|[1-9][0-9]*)$/.test(text)) throw new Error('invalid journal sequence');
  const value = BigInt(text);
  if (value > maxSequence) throw new Error('journal sequence exceeds signed i64');
  return value;
}

/** Owns a dedicated file-backed DatabaseSync/bun:sqlite-compatible connection.
 * Appends share BEGIN IMMEDIATE until flush COMMITs. Stale/component failures
 * fence the log; close/reopen discovers an uncertain committed prefix. */
export class SqliteCommitLog {
  #db;
  #tail = 0n;
  #pending = false;
  #closed = false;
  #poisoned = false;
  constructor(connection, options = {}) {
    this.#db = connection;
    try {
      if (options === null || typeof options !== 'object' || Array.isArray(options)) throw new TypeError('SQLite options must be an object');
      const { busyTimeoutMs = 0, autoCheckpointPages = 1000 } = options;
      integer(busyTimeoutMs, 'busyTimeoutMs'); integer(autoCheckpointPages, 'autoCheckpointPages');
      const version = this.#scalar('SELECT sqlite_version()');
      if (typeof version !== 'string' || !/^\d+\.\d+\.\d+$/.test(version)) throw new Error('invalid SQLite runtime version');
      const [major, minor, patch] = version.split('.').map(Number);
      if (major < 3 || (major === 3 && (minor < 51 || (minor === 51 && patch < 3)))) throw new Error('SQLite >= 3.51.3 is required');
      this.#identity(); // Reject a foreign database before changing its journal mode.
      this.#db.exec(`PRAGMA busy_timeout=${busyTimeoutMs}`);
      if (this.#scalar('PRAGMA journal_mode=WAL') !== 'wal') throw new Error('SQLite journal requires file-backed WAL');
      this.#db.exec(`PRAGMA synchronous=FULL; PRAGMA wal_autocheckpoint=${autoCheckpointPages}`);
      this.#settings();
      if (this.#scalar('PRAGMA busy_timeout') !== busyTimeoutMs || this.#scalar('PRAGMA wal_autocheckpoint') !== autoCheckpointPages) throw new Error('SQLite options were not applied');
      this.#db.exec('BEGIN IMMEDIATE'); this.#pending = true;
      if (this.#identity()) {
        this.#db.exec(`${schema}; PRAGMA application_id=${applicationId}; PRAGMA user_version=1`);
      }
      const row = this.#one('SELECT CAST(COUNT(*) AS TEXT) AS count, CAST(COALESCE(MIN(sequence),0) AS TEXT) AS first, CAST(COALESCE(MAX(sequence),0) AS TEXT) AS last FROM link_cli_log');
      const count = sequence(row.count); const first = sequence(row.first); const last = sequence(row.last);
      if (count !== last || first !== (count === 0n ? 0n : 1n)) throw new Error('journal sequences must be contiguous from one');
      this.#tail = last;
      this.#db.exec('COMMIT'); this.#pending = false;
    } catch (error) {
      this.#rollback();
      try { this.#db.close(); } catch { /* Preserve the construction failure. */ }
      this.#closed = true; throw error;
    }
  }
  #one(sql) {
    const rows = this.#db.prepare(sql).all();
    if (rows.length !== 1) throw new Error('SQLite query must return one row');
    return rows[0];
  }
  #scalar(sql) { return Object.values(this.#one(sql))[0]; }
  #identity() {
    const id = this.#scalar('PRAGMA application_id'); const version = this.#scalar('PRAGMA user_version');
    const objects = this.#db.prepare('SELECT type, name, tbl_name, sql FROM sqlite_schema ORDER BY name').all();
    if (id === 0 && version === 0 && objects.length === 0) return true;
    const table = objects[0];
    if (id !== applicationId || version !== 1 || objects.length !== 1 || table.type !== 'table' || table.name !== 'link_cli_log' || table.tbl_name !== 'link_cli_log' || table.sql !== schema) throw new Error('foreign or unsupported journal schema');
    return false;
  }
  #settings() {
    if (this.#scalar('PRAGMA journal_mode') !== 'wal' || this.#scalar('PRAGMA synchronous') !== 2) throw new Error('SQLite WAL/FULL settings changed');
  }
  #check() {
    if (this.#closed) throw new Error('journal is closed');
    if (this.#poisoned) throw new Error('indeterminate journal outcome; close and reopen');
  }
  #rollback() {
    if (this.#pending) { try { this.#db.exec('ROLLBACK'); } catch { /* A failed COMMIT may already have committed. */ } }
    this.#pending = false;
  }
  #io(operation) {
    try { return operation(); }
    catch (error) { this.#poisoned = true; this.#rollback(); throw error; }
  }
  get supportsDurableFlush() { return true; }
  read() {
    this.#check();
    return this.#io(() => {
      const rows = this.#db.prepare('SELECT CAST(sequence AS TEXT) AS sequence, record FROM link_cli_log ORDER BY sequence').all();
      const records = []; let previous = 0n;
      for (const row of rows) {
        const current = sequence(row.sequence);
        if (current !== previous + 1n) throw new Error('journal sequences must be contiguous from one');
        if (!(row.record instanceof Uint8Array)) throw new Error('journal record must be a BLOB');
        const record = decoder.decode(row.record);
        if (/[\r\n]/.test(record)) throw new Error('journal record contains a physical line break');
        records.push(record); previous = current;
      }
      if (previous !== this.#tail) throw new Error('stale journal owner: sequence tail changed');
      return JSON.stringify(records);
    });
  }
  append(record) {
    this.#check();
    if (typeof record !== 'string' || /[\r\n]/.test(record) || !record.isWellFormed()) throw new TypeError('journal record must be well-formed Unicode without line breaks');
    if (this.#tail === maxSequence) throw new RangeError('journal sequence exhausted');
    const bytes = encoder.encode(record);
    this.#io(() => {
      if (!this.#pending) {
        this.#db.exec('BEGIN IMMEDIATE'); this.#pending = true; this.#settings();
        const tail = sequence(this.#scalar('SELECT CAST(COALESCE(MAX(sequence),0) AS TEXT) FROM link_cli_log'));
        if (tail !== this.#tail) throw new Error('stale journal owner: sequence tail changed');
      }
      const next = this.#tail + 1n;
      this.#db.prepare('INSERT INTO link_cli_log (sequence, record) VALUES (?, ?)').run(next, bytes);
      this.#tail = next;
    });
  }
  flush() {
    this.#check();
    if (!this.#pending) return;
    this.#io(() => { this.#settings(); this.#db.exec('COMMIT'); this.#pending = false; });
  }
  checkpoint(mode = 'passive') {
    this.#check();
    if (!['passive', 'full', 'restart', 'truncate'].includes(mode)) throw new TypeError('invalid checkpoint mode');
    if (this.#pending) throw new Error('cannot checkpoint pending appends');
    return this.#io(() => {
      this.#settings(); const row = this.#one(`PRAGMA wal_checkpoint(${mode})`);
      return { busy: row.busy, logFrames: row.log, checkpointedFrames: row.checkpointed };
    });
  }
  close() {
    if (this.#closed) return;
    this.#rollback();
    this.#db.close(); // Keep close retryable if the provider retains an open handle.
    this.#closed = true;
  }
  [Symbol.dispose]() { this.close(); }
}
