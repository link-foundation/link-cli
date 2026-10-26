import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import process from 'node:process';
import { syncBuiltinESMExports } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { FilePageDevice, FilePagedCommitLog } from '../src/engine-file-paged-log.js';

function directory() { return fs.mkdtempSync(join(tmpdir(), 'clink-pages-fault-')); }
function replace(context, name, implementation) {
  const mocked = context.mock.method(fs, name, implementation);
  syncBuiltinESMExports();
  return () => { mocked.mock.restore(); syncBuiltinESMExports(); };
}

test('native page I/O completes short reads/writes and fences zero-progress writes', (context) => {
  const dir = directory(), path = join(dir, 'log');
  const read = fs.readSync, write = fs.writeSync;
  const restoreRead = replace(context, 'readSync', (fd, bytes, offset, length, position) =>
    read(fd, bytes, offset, Math.min(length, 19), position));
  let stalled = false;
  const restoreWrite = replace(context, 'writeSync', (fd, bytes, offset, length, position) =>
    stalled ? 0 : write(fd, bytes, offset, Math.min(length, 23), position));
  try {
    using log = new FilePagedCommitLog(path);
    log.append('雪'.repeat(700)); log.flush(); log.close();
    using reopened = new FilePagedCommitLog(path);
    assert.deepEqual(JSON.parse(reopened.read()), ['雪'.repeat(700)]);
    stalled = true; reopened.append('pending');
    assert.throws(() => reopened.flush(), /no progress/);
    assert.throws(() => reopened.read(), /fenced/);
    reopened.close(); stalled = false;
    using following = new FilePagedCommitLog(path);
    following.append('following'); following.flush();
    assert.deepEqual(JSON.parse(following.read()), ['雪'.repeat(700), 'following']);
  } finally { restoreRead(); restoreWrite(); fs.rmSync(dir, { recursive: true, force: true }); }
});

test('truncated native reads and malformed metadata preserve the owned files', (context) => {
  const dir = directory(), path = join(dir, 'log');
  try {
    using log = new FilePagedCommitLog(path);
    log.append('ack'); log.flush(); log.close();
    const restore = replace(context, 'readSync', () => 0);
    try { assert.throws(() => new FilePagedCommitLog(path), /truncated/); }
    finally { restore(); }
    fs.writeFileSync(path + '.meta0', 'short');
    using recovered = new FilePagedCommitLog(path);
    assert.deepEqual(JSON.parse(recovered.read()), ['ack']);
    recovered.close();
    fs.writeFileSync(path + '.meta1', 'invalid');
    assert.throws(() => new FilePagedCommitLog(path), /metadata/);
    assert.equal(fs.statSync(path).size, 4096);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('changed native file length rejects publication and closed devices reject I/O', () => {
  const dir = directory(), path = join(dir, 'log');
  try {
    using log = new FilePagedCommitLog(path);
    fs.appendFileSync(path, 'unexpected'); log.append('pending');
    assert.throws(() => log.flush(), /changed during ownership/);
    log.close();
    using device = new FilePageDevice(path);
    assert.equal(device.supportsDurableFlush, true);
    assert.equal(device.newDatabaseId().length, 16);
    device.truncateData(0); device.syncData(); device.close();
    assert.throws(() => device.readPage(0), /closed/);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('directory sync failure releases all acquired handles and ownership', (context) => {
  const dir = directory(), path = join(dir, 'log');
  const restore = replace(context, 'fsyncSync', () => { throw new Error('injected sync failure'); });
  try { assert.throws(() => new FilePagedCommitLog(path), /injected sync failure/); }
  finally { restore(); }
  try {
    assert.equal(fs.existsSync(path + '.writer'), false);
    using retry = new FilePagedCommitLog(path);
    retry.append('retry'); retry.flush();
    assert.deepEqual(JSON.parse(retry.read()), ['retry']);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('native adapter uses the explicit Windows file-flush path', () => {
  const dir = directory(), path = join(dir, 'log');
  const descriptor = Object.getOwnPropertyDescriptor(process, 'platform');
  Object.defineProperty(process, 'platform', { value: 'win32' });
  try { using log = new FilePagedCommitLog(path); log.append('windows path'); log.flush(); }
  finally { Object.defineProperty(process, 'platform', descriptor); }
  try { using log = new FilePagedCommitLog(path); assert.deepEqual(JSON.parse(log.read()), ['windows path']); }
  finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('uncertain close attempts every handle and retains the writer fence', (context) => {
  const dir = directory(), path = join(dir, 'log');
  const device = new FilePageDevice(path);
  const close = fs.closeSync;
  let closed = 0;
  const restore = replace(context, 'closeSync', (fd) => {
    close(fd); closed++;
    throw new Error('injected close failure');
  });
  try {
    assert.throws(() => device.close(), (error) => error instanceof AggregateError && error.errors.length === 4);
    assert.equal(closed, 4); assert.equal(fs.existsSync(path + '.writer'), true);
    assert.throws(() => new FilePagedCommitLog(path), /EEXIST/);
    device.close();
  } finally { restore(); fs.rmSync(dir, { recursive: true, force: true }); }
});

test('owner-removal errors are reported after all handles close', (context) => {
  const dir = directory(), path = join(dir, 'log');
  const device = new FilePageDevice(path), unlink = fs.unlinkSync;
  const restore = replace(context, 'unlinkSync', (file) => {
    unlink(file); throw new Error('injected unlink failure');
  });
  try {
    assert.throws(() => device.close(), (error) => error instanceof AggregateError && error.errors.length === 1);
    assert.equal(fs.existsSync(path + '.writer'), false);
  } finally { restore(); fs.rmSync(dir, { recursive: true, force: true }); }
});

test('failed native construction preserves the original cause and every cleanup error', (context) => {
  const messages = (error) => error instanceof AggregateError ? error.errors.flatMap(messages) : [error.message];
  for (const partial of [true, false]) {
    const dir = directory(), path = join(dir, 'log'), owned = new Set();
    const open = fs.openSync, close = fs.closeSync;
    const restoreOpen = replace(context, 'openSync', (name, ...options) => {
      if (partial && name === path + '.meta0') throw new Error('original acquisition failure');
      const fd = open(name, ...options);
      if ([path, path + '.writer', path + '.meta0', path + '.meta1'].includes(name)) owned.add(fd);
      return fd;
    });
    let closed = 0;
    const restoreClose = replace(context, 'closeSync', (fd) => {
      close(fd);
      if (owned.delete(fd)) { closed++; throw new Error(`cleanup handle ${closed}`); }
    });
    try {
      assert.throws(() => partial ? new FilePageDevice(path) : new FilePagedCommitLog(path, { maxPages: 0 }), (error) => {
        const causes = messages(error);
        assert.match(causes[0], partial ? /original acquisition failure/ : /page limit/);
        assert.deepEqual(causes.slice(1), Array.from({ length: partial ? 2 : 4 }, (_, i) => `cleanup handle ${i + 1}`));
        return true;
      });
      assert.equal(owned.size, 0); assert.equal(closed, partial ? 2 : 4);
      assert.equal(fs.existsSync(path + '.writer'), true);
    } finally { restoreOpen(); restoreClose(); fs.rmSync(dir, { recursive: true, force: true }); }
  }
});

test('directory synchronization and close faults preserve every original cause', (context) => {
  const causes = (error) => error instanceof AggregateError ? error.errors.flatMap(causes) : [error];
  for (const syncFails of [false, true]) for (const cleanupFails of [false, true]) {
    const dir = directory(), path = join(dir, 'log');
    using initial = new FilePagedCommitLog(path); initial.append('acknowledged 雪'); initial.flush(); initial.close();
    const open = fs.openSync, close = fs.closeSync, sync = fs.fsyncSync, handles = new Set();
    const syncError = new Error('directory synchronization failed'), closeError = new Error('directory close failed');
    const cleanupErrors = Array.from({ length: 4 }, (_, i) => new Error(`raw cleanup ${i}`));
    let directoryHandle, attempted = 0;
    const restoreOpen = replace(context, 'openSync', (name, ...options) => {
      const fd = open(name, ...options);
      if (name === dir) directoryHandle = fd; else handles.add(fd);
      return fd;
    });
    const restoreSync = replace(context, 'fsyncSync', (fd) => {
      if (fd === directoryHandle && syncFails) throw syncError;
      return sync(fd);
    });
    const restoreClose = replace(context, 'closeSync', (fd) => {
      close(fd);
      if (fd === directoryHandle) throw closeError;
      assert.equal(handles.delete(fd), true);
      const error = cleanupErrors[attempted++]; if (cleanupFails) throw error;
    });
    try {
      assert.throws(() => new FilePagedCommitLog(path), (error) => {
        assert.deepEqual(causes(error), [...(syncFails ? [syncError] : []), closeError, ...(cleanupFails ? cleanupErrors : [])]);
        return true;
      });
      assert.equal(attempted, 4); assert.equal(handles.size, 0);
      assert.equal(fs.existsSync(path + '.writer'), cleanupFails);
    } finally { restoreOpen(); restoreSync(); restoreClose(); }
    try {
      // All actual close calls completed above; only the injected report was uncertain.
      if (cleanupFails) fs.unlinkSync(path + '.writer');
      using recovered = new FilePagedCommitLog(path);
      assert.deepEqual(JSON.parse(recovered.read()), ['acknowledged 雪']);
      recovered.append('following'); recovered.flush(); recovered.close();
      using again = new FilePagedCommitLog(path);
      assert.deepEqual(JSON.parse(again.read()), ['acknowledged 雪', 'following']);
    } finally { fs.rmSync(dir, { recursive: true, force: true }); }
  }
});
