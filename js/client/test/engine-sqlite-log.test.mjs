// Actual provider contracts, first authored before the optional adapter.
import assert from 'node:assert/strict';
import test from 'node:test';
import { DatabaseSync as Database } from 'node:sqlite';
import { mkdtempSync, rmSync, statSync, readFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { SqliteCommitLog } from '../src/engine-sqlite-log.js';
const records = log => JSON.parse(log.read());
function owned(path, options = {}) { return new SqliteCommitLog(new Database(path), options); }
const bytes = new TextEncoder();
function scenario(label, operation) {
  test('SQLite: ' + label, { timeout: 10000 }, () => {
    const directory = mkdtempSync(join(tmpdir(), 'clink-sqlite-test-'));
    try { operation(join(directory, 'journal.db')); }
    finally { rmSync(directory, { recursive: true, force: true }); }
  });
}
  scenario('BLOB fidelity, own pending view, independent committed view and rollback on close', file => {
    const log = owned(file); const independent = new Database(file);
    try {
      assert.equal(log.supportsDurableFlush, true);
      assert.deepEqual(records(log), []);
      log.append(''); log.append('one 雪\0😀');
      assert.deepEqual(records(log), ['', 'one 雪\0😀']);
      assert.equal(independent.prepare('SELECT COUNT(*) AS count FROM link_cli_log').get().count, 0);
      for (const invalid of ['line\nbreak', 'line\rbreak', '\ud800', '\udfff', null, 12]) assert.throws(() => log.append(invalid));
      assert.deepEqual(records(log), ['', 'one 雪\0😀']);
      log.flush();
      assert.equal(independent.prepare('SELECT COUNT(*) AS count FROM link_cli_log').get().count, 2);
      log.append('pending'); log.close();
      const reopened = owned(file);
      try { assert.deepEqual(records(reopened), ['', 'one 雪\0😀']); reopened.flush(); }
      finally { reopened.close(); }
      for (const op of [() => log.read(), () => log.append('late'), () => log.flush(), () => log.checkpoint()]) assert.throws(op, /closed/i);
      log.close();
    } finally { log.close(); independent.close(); }
  });
  for (const operation of ['read', 'append']) scenario(`stale owner ${operation} fails and fences all subsequent operations`, file => {
    const winner = owned(file); const stale = owned(file);
    try {
      winner.append('winner'); winner.flush();
      assert.throws(() => operation === 'read' ? stale.read() : stale.append('stale'), /stale/i);
      for (const op of [() => stale.read(), () => stale.append('later'), () => stale.flush()]) assert.throws(op, /indeterminate/i);
      assert.deepEqual(records(winner), ['winner']);
    } finally { stale.close(); winner.close(); }
    const reopened = owned(file);
    try { assert.deepEqual(records(reopened), ['winner']); reopened.append('following'); reopened.flush(); }
    finally { reopened.close(); }
  });
  scenario('busy writer cannot publish and pending maintenance rejects without poisoning the writer', file => {
    const winner = owned(file); const competitor = owned(file);
    try {
      winner.append('winner');
      assert.throws(() => winner.checkpoint(), /pending/i);
      assert.throws(() => competitor.append('competing'), /busy|locked/i);
      assert.throws(() => competitor.flush(), /indeterminate/i);
      winner.flush(); assert.deepEqual(records(winner), ['winner']);
    } finally { competitor.close(); winner.close(); }
  });
  scenario('pinned reader reports blocked checkpoint, then truncates after release', file => {
    const log = owned(file, { autoCheckpointPages: 0 }); const reader = new Database(file);
    try {
      log.append('one'); log.flush(); reader.exec('BEGIN');
      assert.equal(reader.prepare('SELECT COUNT(*) AS count FROM link_cli_log').get().count, 1);
      log.append('two'); log.flush();
      const blocked = log.checkpoint('truncate'); assert.equal(blocked.busy, 1);
      assert.ok(blocked.logFrames > blocked.checkpointedFrames);
      reader.exec('COMMIT');
      assert.deepEqual(log.checkpoint('truncate'), { busy: 0, logFrames: 0, checkpointedFrames: 0 });
      assert.equal(statSync(file + '-wal').size, 0);
      for (const mode of ['passive', 'full', 'restart', 'truncate']) assert.equal(log.checkpoint(mode).busy, 0);
      assert.throws(() => log.checkpoint('unknown'));
      assert.deepEqual(records(log), ['one', 'two']);
    } finally { reader.close(); log.close(); }
  });
  for (const after of [false, true]) scenario(`uncertain COMMIT ${after ? 'after' : 'before'} operation resolves exact durable prefix`, file => {
    let armed = false;
    const db = new Database(file);
    const wrapped = {
      prepare: sql => db.prepare(sql),
      exec(sql) {
        if (armed && /^COMMIT$/i.test(sql.trim())) {
          if (after) db.exec(sql);
          throw Error('injected commit failure');
        }
        return db.exec(sql);
      },
      close: () => db.close(),
    };
    const log = new SqliteCommitLog(wrapped);
    try {
      log.append('before'); log.flush(); log.append('uncertain'); armed = true;
      assert.throws(() => log.flush(), /injected/);
      assert.throws(() => log.read(), /indeterminate/i);
      assert.throws(() => log.append('retry'), /indeterminate/i);
      assert.throws(() => log.flush(), /indeterminate/i);
    } finally { log.close(); }
    const reopened = owned(file);
    try {
      assert.deepEqual(records(reopened), after ? ['before', 'uncertain'] : ['before']);
      reopened.append('following'); reopened.flush();
    } finally { reopened.close(); }
  });
  scenario('reject unrelated databases without changing their schema or payload', file => {
    const db = new Database(file); db.exec('CREATE TABLE unrelated(value TEXT); INSERT INTO unrelated VALUES (\'keep\')'); db.close();
    assert.throws(() => owned(file), /schema|foreign|unclaimed/i);
    const reopened = new Database(file);
    try { assert.equal(reopened.prepare('SELECT value FROM unrelated').get().value, 'keep'); }
    finally { reopened.close(); }
  });
  scenario('sequence holes and malformed UTF-8 fail closed', file => {
    const log = owned(file); log.append('valid'); log.flush(); log.close();
    let db = new Database(file);
    db.prepare('INSERT INTO link_cli_log VALUES (?, ?)').run(3, bytes.encode('hole')); db.close();
    assert.throws(() => owned(file), /sequence|contiguous/i);
    db = new Database(file); db.exec('DELETE FROM link_cli_log WHERE sequence=3');
    db.prepare('UPDATE link_cli_log SET record=? WHERE sequence=1').run(new Uint8Array([255])); db.close();
    const bad = owned(file);
    try { assert.throws(() => bad.read(), /UTF|encoded|encoding/i); assert.throws(() => bad.flush(), /indeterminate/i); }
    finally { bad.close(); }
  });

// Actual SQL providers plus narrow response/syscall faults at the owned boundary.
function wrapped(file, { rows = (_sql, value) => value, exec = (db, sql) => db.exec(sql), close = db => db.close() } = {}) {
  const db = new Database(file);
  return { db, connection: {
    exec: sql => exec(db, sql),
    prepare(sql) {
      const statement = db.prepare(sql);
      return { all: (...parameters) => rows(sql, statement.all(...parameters)), run: (...parameters) => statement.run(...parameters) };
    },
    close: () => close(db),
  } };
}
const fixture = JSON.parse(readFileSync(new URL('../../../docs/protocol/sqlite-log-vectors.json', import.meta.url), 'utf8'));
for (const options of [null, [], 1]) scenario('invalid options object closes owned connection: ' + JSON.stringify(options), file => {
  const db = new Database(file); assert.throws(() => new SqliteCommitLog(db, options), /options/i);
  assert.throws(() => db.prepare('SELECT 1'), /closed|open/i);
});
scenario('independent fixture bytes, invalid UTF-8 and physical newlines', file => {
  using log = owned(file);
  for (const row of fixture.records) log.append(row.text);
  log.flush();
  using db = new Database(file);
  const actual = db.prepare('SELECT record FROM link_cli_log ORDER BY sequence').all();
  assert.deepEqual(actual.map(row => Buffer.from(row.record).toString('hex')), fixture.records.map(row => row.utf8Hex));
  assert.deepEqual(records(log), fixture.records.map(row => row.text));
  for (const invalid of [...fixture.invalidUtf8Hex.map(hex => Uint8Array.from(Buffer.from(hex, 'hex'))), bytes.encode('\n'), bytes.encode('\r')]) {
    db.prepare('UPDATE link_cli_log SET record=? WHERE sequence=1').run(invalid);
    const bad = owned(file);
    try { assert.throws(() => bad.read()); assert.throws(() => bad.flush(), /indeterminate/i); }
    finally { bad.close(); }
  }
});
for (const [key, value] of [['busyTimeoutMs', -1], ['busyTimeoutMs', 0.5], ['busyTimeoutMs', 2147483648], ['autoCheckpointPages', -1]]) {
  scenario(`invalid ${key}=${value} closes the owned connection`, file => {
    const db = new Database(file);
    assert.throws(() => new SqliteCommitLog(db, { [key]: value }), /integer/i);
    assert.throws(() => db.prepare('SELECT 1'), /closed|open/i);
  });
}
for (const version of ['3.51.2', '3.50.99', '2.99.99', 'invalid', 3]) scenario(`reject unsupported runtime ${version}`, file => {
  const { connection } = wrapped(file, { rows: (sql, value) => sql === 'SELECT sqlite_version()' ? [{ version }] : value });
  assert.throws(() => new SqliteCommitLog(connection), /version|required/i);
});
for (const version of ['3.51.3', '4.0.0']) scenario(`accept supported runtime version control ${version}`, file => {
  const { connection } = wrapped(file, { rows: (sql, value) => sql === 'SELECT sqlite_version()' ? [{ version }] : value });
  using log = new SqliteCommitLog(connection); log.append('ok'); log.flush();
});
for (const ddl of fixture.invalidSchemas) scenario('reject incompatible claimed schema: ' + ddl, file => {
  const db = new Database(file); db.exec(`${ddl}; PRAGMA application_id=${fixture.applicationId}; PRAGMA user_version=1`); db.close();
  assert.throws(() => owned(file), /schema/i);
});
scenario('reject in-memory journal mode and ignored SQLite options', file => {
  assert.throws(() => new SqliteCommitLog(new Database(':memory:')), /WAL/i);
  const { connection } = wrapped(file, { rows: (sql, value) => sql === 'PRAGMA busy_timeout' ? [{ timeout: 1 }] : value });
  assert.throws(() => new SqliteCommitLog(connection), /options/i);
});
scenario('setting drift fails before publishing and close permits exact recovery', file => {
  const { db, connection } = wrapped(file); const log = new SqliteCommitLog(connection);
  db.exec('PRAGMA synchronous=NORMAL');
  assert.throws(() => log.append('unsafe'), /settings/i); assert.throws(() => log.flush(), /indeterminate/i); log.close();
  using reopened = owned(file); assert.deepEqual(records(reopened), []); reopened.append('safe'); reopened.flush();
});
for (const field of ['count', 'first', 'last']) for (const value of ['01', '-1', '9223372036854775808', 1]) {
  scenario(`invalid sequence summary ${field}=${value}`, file => {
    const { connection } = wrapped(file, { rows: (sql, result) => sql.includes('COUNT(*) AS TEXT') ? [{ ...result[0], [field]: value }] : result });
    assert.throws(() => new SqliteCommitLog(connection), /sequence/i);
  });
}
scenario('sequence exhaustion rejects before I/O without poisoning read-only maintenance', file => {
  const { connection } = wrapped(file, { rows: (sql, result) => sql.includes('COUNT(*) AS TEXT') ? [{ count: '9223372036854775807', first: '1', last: '9223372036854775807' }] : result });
  using log = new SqliteCommitLog(connection);
  assert.throws(() => log.append('too far'), /exhausted/i); log.flush(); assert.equal(log.checkpoint().busy, 0);
});
for (const change of [row => ({ ...row, sequence: '2' }), row => ({ ...row, record: 'text' })]) scenario('malformed ordered-read boundary is fenced', file => {
  let armed = false;
  const { connection } = wrapped(file, { rows: (sql, result) => armed && sql.startsWith('SELECT CAST(sequence') ? result.map(change) : result });
  using log = new SqliteCommitLog(connection); log.append('one'); log.flush(); armed = true;
  assert.throws(() => log.read(), /sequence|BLOB/i); assert.throws(() => log.read(), /indeterminate/i);
});
scenario('missing scalar result and constructor cleanup errors preserve the original failure', file => {
  const { connection } = wrapped(file, { rows: () => [], close(db) { db.close(); throw Error('secondary close error'); } });
  assert.throws(() => new SqliteCommitLog(connection), /one row/i);
});
scenario('provider close failure remains retryable and disposal rolls back', file => {
  let fail = true;
  const { connection } = wrapped(file, { close(db) { if (fail) { fail = false; throw Error('close failed'); } db.close(); } });
  const log = new SqliteCommitLog(connection); log.append('pending'); assert.throws(() => log.close(), /close failed/); log[Symbol.dispose](); log.close();
  using reopened = owned(file); assert.deepEqual(records(reopened), []);
});
