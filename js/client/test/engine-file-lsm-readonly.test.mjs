import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { syncBuiltinESMExports } from 'node:module';
import { ReadOnlyLsmDevice, deferRepair, parts, throwCleanup, readOnlyWal } from '../src/engine-file-lsm-readonly.js';

function run(body) {
  const directory = fs.mkdtempSync(join(tmpdir(), 'clink-lsm-discovery-'));
  try { body(join(directory, 'table')); }
  finally { fs.rmSync(directory, { recursive: true, force: true }); }
}
const causes = error => error instanceof AggregateError ? error.errors.flatMap(causes) : [error.message];
const restore = mocks => { for (const mock of mocks) mock.mock.restore(); syncBuiltinESMExports(); };

test('read-only discovery distinguishes missing files, empty and malformed metadata, and complete pages', () => run(path => {
  assert.deepEqual(parts(path), [path, path + '.meta0', path + '.meta1']);
  const missing = new ReadOnlyLsmDevice(path);
  assert.equal(missing.dataLength, 0); assert.equal(missing.supportsDurableFlush, true);
  assert.equal(missing.readMetadata(0), undefined);
  assert.throws(() => missing.readPage(0), /missing selected/); missing.completeRecovery(); missing.close(); missing.close();
  for (const method of ['newDatabaseId', 'writeMetadata', 'syncMetadata', 'writePage', 'repairPage', 'syncData', 'truncateData'])
    assert.throws(() => missing[method](), /read-only/);
  assert.throws(() => missing.dataLength, /closed/); assert.throws(() => missing.readPage(0), /closed/);
  assert.throws(() => missing.readMetadata(0), /closed/);
  fs.writeFileSync(path, Buffer.alloc(4096, 71));
  fs.writeFileSync(path + '.meta0', Buffer.alloc(128, 23)); fs.writeFileSync(path + '.meta1', Buffer.alloc(17));
  const device = new ReadOnlyLsmDevice(path);
  assert.equal(device.dataLength, 4096); assert.deepEqual(device.readPage(0), new Uint8Array(4096).fill(71));
  assert.deepEqual(device.readMetadata(0), new Uint8Array(128).fill(23));
  assert.deepEqual(device.readMetadata(1), new Uint8Array(1));
  fs.truncateSync(path + '.meta1', 0); assert.equal(device.readMetadata(1), undefined);
  device.close();
}));

test('read-only exact reads accept finite short reads and reject a concurrent truncation', context => run(path => {
  fs.writeFileSync(path, Buffer.alloc(4096, 3)); fs.writeFileSync(path + '.meta0', Buffer.alloc(128, 4));
  const read = fs.readSync; let truncate = false;
  const mock = context.mock.method(fs, 'readSync', (fd, bytes, offset, length, position) =>
    truncate ? 0 : read(fd, bytes, offset, Math.min(13, length), position));
  syncBuiltinESMExports(); const device = new ReadOnlyLsmDevice(path);
  try {
    assert.equal(device.readPage(0)[4095], 3); assert.equal(device.readMetadata(0)[127], 4);
    truncate = true; assert.throws(() => device.readPage(0), /truncated/);
  } finally { device.close(); restore([mock]); }
}));

test('read-only acquisition rejects nonregular handles and retains original errors after clean cleanup', context => run(path => {
  fs.writeFileSync(path, ''); const stat = fs.fstatSync;
  let mock = context.mock.method(fs, 'fstatSync', fd => ({ ...stat(fd), isFile: () => false }));
  syncBuiltinESMExports();
  try { assert.throws(() => new ReadOnlyLsmDevice(path), /nonregular/); } finally { restore([mock]); }
  const open = fs.openSync;
  mock = context.mock.method(fs, 'openSync', (name, ...args) => {
    if (name.endsWith('.meta0')) throw Error('acquisition failure'); return open(name, ...args);
  });
  syncBuiltinESMExports();
  try { assert.throws(() => new ReadOnlyLsmDevice(path), /^Error: acquisition failure$/); } finally { restore([mock]); }
}));

test('read-only cleanup attempts every handle once and caches uncertain before/after close failures', context => {
  for (const after of [false, true]) run(path => {
    for (const name of parts(path)) fs.writeFileSync(name, '');
    const close = fs.closeSync, device = new ReadOnlyLsmDevice(path), attempts = [], live = new Set(); let hit = false;
    const mock = context.mock.method(fs, 'closeSync', fd => {
      attempts.push(fd); const fault = !hit; hit = true;
      if (fault && !after) { live.add(fd); throw Error('unknown close'); }
      close(fd); if (fault) throw Error('unknown close');
    });
    syncBuiltinESMExports();
    try {
      let first; assert.throws(() => device.close(), error => { first = error; assert.deepEqual(causes(error), ['unknown close']); return true; });
      assert.equal(attempts.length, 3); assert.equal(new Set(attempts).size, 3);
      assert.throws(() => device.close(), error => error === first); assert.equal(attempts.length, 3);
    } finally { restore([mock]); for (const fd of live) close(fd); }
  });
});

test('read-only acquisition reports both the primary failure and uncertain cleanup', context => run(path => {
  fs.writeFileSync(path, ''); const open = fs.openSync, close = fs.closeSync;
  const mocks = [context.mock.method(fs, 'openSync', (name, ...args) => {
    if (name.endsWith('.meta0')) throw Error('primary acquisition'); return open(name, ...args);
  }), context.mock.method(fs, 'closeSync', fd => { close(fd); throw Error('cleanup reply lost'); })];
  syncBuiltinESMExports();
  try {
    assert.throws(() => new ReadOnlyLsmDevice(path), error => {
      assert.deepEqual(causes(error), ['primary acquisition', 'cleanup reply lost']); return true;
    });
  } finally { restore(mocks); }
}));

test('deferred discovery forwards getters and bound methods without completing repairs', () => {
  class Device { #value = 7; get value() { return this.#value; } read() { return this.#value; } completeRecovery() { throw Error('early repair'); } }
  const device = deferRepair(new Device()); assert.equal(device.value, 7); assert.equal(device.read(), 7); device.completeRecovery();
  throwCleanup([]);
  assert.throws(() => throwCleanup([Error('first')]), error => { assert.match(error.message, /retain outer fence/); return true; });
  assert.throws(() => throwCleanup([Error('second')], 'specific cleanup'), /specific cleanup/);
});

test('read-only semantic WAL owns its snapshot and rejects every mutating operation', () => {
  const records = ['owned\0雪'], wal = readOnlyWal(records); records.push('later');
  assert.equal(wal.supportsReplacement, true); assert.equal(wal.read(), '["owned\\u0000雪"]');
  for (const method of ['append', 'flush', 'replace']) assert.throws(() => wal[method](), /read-only WAL/);
});
