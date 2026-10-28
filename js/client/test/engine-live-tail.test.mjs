import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FileCommitLog } from '../src/engine-file-log.js';

for (const unicode of [false, true]) for (const next of ['read', 'append', 'flush']) {
  test(`live file journal repairs partial ${unicode ? 'UTF-8' : 'ASCII'} before ${next}`, (context) => {
    const directory = fs.mkdtempSync(join(tmpdir(), 'journal-live-tail-'));
    const original = fs.writeSync;
    let writes = 0;
    let fail = false;
    const mocked = context.mock.method(fs, 'writeSync', (fd, bytes, offset, length) => {
      if (!fail) return original(fd, bytes, offset, length);
      if (writes++ === 0) return original(fd, bytes, offset, 1);
      throw new Error('injected error after one byte');
    });
    syncBuiltinESMExports();
    try {
      using log = new FileCommitLog(join(directory, 'journal'));
      log.append('committed'); log.flush();
      fail = true;
      assert.throws(() => log.append(unicode ? '雪' : 'uncommitted'), /after one byte/);
      assert.equal(fs.statSync(join(directory, 'journal')).size, 11);
      fail = false;
      if (next === 'read') assert.deepEqual(JSON.parse(log.read()), ['committed']);
      if (next === 'flush') { log.flush(); assert.deepEqual(JSON.parse(log.read()), ['committed']); }
      log.append('following'); log.flush();
      assert.deepEqual(JSON.parse(log.read()), ['committed', 'following']);
      log.close();
      using reopened = new FileCommitLog(join(directory, 'journal'));
      assert.deepEqual(JSON.parse(reopened.read()), ['committed', 'following']);
    } finally {
      mocked.mock.restore(); syncBuiltinESMExports();
      fs.rmSync(directory, { recursive: true, force: true });
    }
  });
}

for (const phase of ['read', 'truncate', 'sync']) {
  test(`failed ${phase} during tail repair retains recovery for retry`, (context) => {
    const directory = fs.mkdtempSync(join(tmpdir(), 'journal-repair-retry-'));
    const path = join(directory, 'journal');
    const originals = { write: fs.writeSync, read: fs.readSync, truncate: fs.ftruncateSync, sync: fs.fsyncSync };
    let writes = 0;
    let failingWrite = false;
    let failingRepair = false;
    const mocks = [
      context.mock.method(fs, 'writeSync', (fd, bytes, offset, length) => {
        if (!failingWrite) return originals.write(fd, bytes, offset, length);
        if (writes++ === 0) return originals.write(fd, bytes, offset, 10000);
        throw new Error('partial write');
      }),
      context.mock.method(fs, 'readSync', (...args) => {
        if (failingRepair && phase === 'read') throw new Error('repair read');
        return originals.read(...args);
      }),
      context.mock.method(fs, 'ftruncateSync', (...args) => {
        if (failingRepair && phase === 'truncate') throw new Error('repair truncate');
        return originals.truncate(...args);
      }),
      context.mock.method(fs, 'fsyncSync', (...args) => {
        if (failingRepair && phase === 'sync') throw new Error('repair sync');
        return originals.sync(...args);
      }),
    ];
    syncBuiltinESMExports();
    try {
      using log = new FileCommitLog(path);
      log.append('committed'); log.flush();
      failingWrite = true;
      assert.throws(() => log.append('x'.repeat(20000)), /partial write/);
      failingWrite = false;
      failingRepair = true;
      assert.throws(() => log.read(), new RegExp(`repair ${phase}`));
      assert.throws(() => log.append('blocked'), new RegExp(`repair ${phase}`));
      assert.throws(() => log.flush(), new RegExp(`repair ${phase}`));
      failingRepair = false;
      log.append('following'); log.flush();
      assert.deepEqual(JSON.parse(log.read()), ['committed', 'following']);
      assert.equal(fs.readFileSync(path, 'utf8'), 'committed\nfollowing\n');
    } finally {
      for (const mocked of mocks) mocked.mock.restore();
      syncBuiltinESMExports(); fs.rmSync(directory, { recursive: true, force: true });
    }
  });
}

for (const phase of ['append', 'flush']) {
  test(`a complete record survives a ${phase} error after its effect`, (context) => {
    const directory = fs.mkdtempSync(join(tmpdir(), 'journal-complete-error-'));
    const write = fs.writeSync;
    const sync = fs.fsyncSync;
    let fail = false;
    const mocks = [
      context.mock.method(fs, 'writeSync', (...args) => {
        const count = write(...args);
        if (fail && phase === 'append') throw new Error('after complete write');
        return count;
      }),
      context.mock.method(fs, 'fsyncSync', (...args) => {
        sync(...args);
        if (fail && phase === 'flush') throw new Error('after complete flush');
      }),
    ];
    syncBuiltinESMExports();
    try {
      using log = new FileCommitLog(join(directory, 'journal'));
      fail = true;
      if (phase === 'append') assert.throws(() => log.append('committed'), /complete write/);
      else { log.append('committed'); assert.throws(() => log.flush(), /complete flush/); }
      fail = false;
      assert.deepEqual(JSON.parse(log.read()), ['committed']);
      log.append('following'); log.flush();
      assert.deepEqual(JSON.parse(log.read()), ['committed', 'following']);
    } finally {
      for (const mocked of mocks) mocked.mock.restore();
      syncBuiltinESMExports(); fs.rmSync(directory, { recursive: true, force: true });
    }
  });
}
