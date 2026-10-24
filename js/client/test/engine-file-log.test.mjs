import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtempSync, rmSync, appendFileSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FileCommitLog } from '../src/engine-file-log.js';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';

test('file journal fences writers, flushes, repairs torn tails, and validates UTF-8', () => {
  const directory = mkdtempSync(join(tmpdir(), 'engine-log-'));
  const path = join(directory, 'journal');
  try {
    const log = new FileCommitLog(path);
    assert.equal(log.supportsDurableFlush, true);
    assert.equal(log.read(), '[]');
    assert.throws(() => new FileCommitLog(path), /EEXIST/);
    log.append('雪'); log.flush();
    assert.equal(log.read(), '["雪"]');
    assert.throws(() => log.append('a\nb'), /line breaks/);
    log.close(); log.close();
    for (const operation of [() => log.read(), () => log.append('a'), () => log.flush()])
      assert.throws(operation, /closed/);
    appendFileSync(path, 'x'.repeat(20000));
    using repaired = new FileCommitLog(path);
    assert.equal(repaired.read(), '["雪"]');
    repaired.close();
    writeFileSync(path, new Uint8Array([255, 10]));
    using invalid = new FileCommitLog(path);
    assert.throws(() => invalid.read(), /encoded data|encoding/i);
  } finally { rmSync(directory, { recursive: true, force: true }); }
});

test('tail recovery handles short filesystem reads', (context) => {
  const directory = mkdtempSync(join(tmpdir(), 'engine-short-read-'));
  const path = join(directory, 'journal');
  writeFileSync(path, 'committed\nunfinished');
  const original = fs.readSync;
  const mocked = context.mock.method(fs, 'readSync', (fd, buffer, offset, length, position) =>
    original(fd, buffer, offset, Math.min(length, 3), position));
  syncBuiltinESMExports();
  try {
    using log = new FileCommitLog(path);
    assert.equal(log.read(), '["committed"]');
  } finally {
    mocked.mock.restore();
    syncBuiltinESMExports();
    rmSync(directory, { recursive: true, force: true });
  }
});

test('partial writes finish and zero progress is reported', (context) => {
  const directory = mkdtempSync(join(tmpdir(), 'engine-short-write-'));
  const path = join(directory, 'journal');
  const original = fs.writeSync;
  let stalled = false;
  const mocked = context.mock.method(fs, 'writeSync', (fd, bytes, offset, length) =>
    stalled ? 0 : original(fd, bytes, offset, Math.min(length, 3)));
  syncBuiltinESMExports();
  try {
    using log = new FileCommitLog(path);
    log.append('committed 雪'); log.flush();
    assert.equal(log.read(), '["committed 雪"]');
    stalled = true;
    assert.throws(() => log.append('failed'), /no progress/);
  } finally {
    mocked.mock.restore(); syncBuiltinESMExports();
    rmSync(directory, { recursive: true, force: true });
  }
});

test('failed constructor flush releases the writer fence for recovery', (context) => {
  const directory = mkdtempSync(join(tmpdir(), 'engine-flush-fault-'));
  const path = join(directory, 'journal');
  const mocked = context.mock.method(fs, 'fsyncSync', () => { throw new Error('injected fsync error'); });
  syncBuiltinESMExports();
  try {
    assert.throws(() => new FileCommitLog(path), /injected fsync error/);
  } finally { mocked.mock.restore(); syncBuiltinESMExports(); }
  try { using recovered = new FileCommitLog(path); assert.equal(recovered.read(), '[]'); }
  finally { rmSync(directory, { recursive: true, force: true }); }
});

test('truncation during a read is reported and an entirely torn tail is discarded', (context) => {
  const directory = mkdtempSync(join(tmpdir(), 'engine-truncated-'));
  const path = join(directory, 'journal');
  writeFileSync(path, 'x'.repeat(20000));
  try {
    using log = new FileCommitLog(path);
    assert.equal(log.read(), '[]');
    log.append('committed'); log.flush();
    const mocked = context.mock.method(fs, 'readSync', () => 0);
    syncBuiltinESMExports();
    try { assert.throws(() => log.read(), /truncated/); }
    finally { mocked.mock.restore(); syncBuiltinESMExports(); }
  } finally { rmSync(directory, { recursive: true, force: true }); }
});
