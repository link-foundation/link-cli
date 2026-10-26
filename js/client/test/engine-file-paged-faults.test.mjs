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
