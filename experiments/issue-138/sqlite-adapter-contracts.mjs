// First-authored finite integration contracts for the optional SQLite journal.
// Before implementation this fails because the optional adapter does not exist.
// node --max-old-space-size=128 experiments/issue-138/sqlite-adapter-contracts.mjs
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { SqliteCommitLog } from '../../js/client/src/engine-sqlite-log.js';

const sqlite = await import(globalThis.Bun ? 'bun:sqlite' : 'node:sqlite');
const Database = sqlite.DatabaseSync ?? sqlite.Database;
const directory = mkdtempSync(join(tmpdir(), 'clink-sqlite-adapter-'));
const records = log => JSON.parse(log.read());
function owned(path, options = {}) { return new SqliteCommitLog(new Database(path), options); }
const bytes = new TextEncoder();
let passed = 0;
function scenario(label, operation) {
  const file = join(directory, `${++passed}.db`);
  operation(file);
  console.log(`ok ${label}`);
}
try {
  scenario('BLOB fidelity, own pending view, independent committed view and rollback on close', file => {
    const log = owned(file); const independent = new Database(file);
    try {
      assert.equal(log.supportsDurableFlush, true);
      assert.deepEqual(records(log), []);
      log.append(''); log.append('\ufeffone 雪\0😀');
      assert.deepEqual(records(log), ['', '\ufeffone 雪\0😀']);
      assert.equal(independent.prepare('SELECT COUNT(*) AS count FROM link_cli_log').get().count, 0);
      for (const invalid of ['line\nbreak', 'line\rbreak', '\ud800', '\udfff', null, 12]) assert.throws(() => log.append(invalid));
      assert.deepEqual(records(log), ['', '\ufeffone 雪\0😀']);
      log.flush();
      assert.equal(independent.prepare('SELECT COUNT(*) AS count FROM link_cli_log').get().count, 2);
      log.append('pending'); log.close();
      const reopened = owned(file);
      try { assert.deepEqual(records(reopened), ['', '\ufeffone 雪\0😀']); reopened.flush(); }
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
  console.log(JSON.stringify({ passed, failed: 0 }));
} finally { rmSync(directory, { recursive: true, force: true }); }
