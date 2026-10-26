import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { syncBuiltinESMExports } from 'node:module';
import { FileCheckpointPagedCommitLog } from '../src/engine-file-checkpoint-pages.js';

const names = (path, mirrored, mirroredRoot) => ['', ...(mirroredRoot ? ['.mirror'] : []), '.bank0', ...(mirrored ? ['.bank0.mirror'] : []), '.bank1', ...(mirrored ? ['.bank1.mirror'] : [])].map(suffix => path + suffix);
const closeOrder = (devices) => devices.slice().reverse().flatMap(name => [name + '.meta1', name + '.meta0', name, name + '.writer']);
const causes = (error) => error instanceof AggregateError ? error.errors.flatMap(causes) : [error.message];
function observe(context, path, { failed = Infinity, failClose = () => false } = {}) {
  const open = fs.openSync, close = fs.closeSync, handles = new Map(), opened = [], closed = [];
  const openMock = context.mock.method(fs, 'openSync', (name, ...options) => {
    if (name.endsWith('.writer')) {
      if (opened.length === failed) throw new Error('original acquisition');
      opened.push(name.slice(0, -7));
    }
    const fd = open(name, ...options);
    if (name === path || name.startsWith(path + '.')) handles.set(fd, name);
    return fd;
  });
  const closeMock = context.mock.method(fs, 'closeSync', (fd) => {
    close(fd); const name = handles.get(fd); handles.delete(fd);
    if (name !== undefined) { closed.push(name); if (failClose(name)) throw new Error('cleanup ' + name); }
  });
  syncBuiltinESMExports();
  return { opened, closed, handles, restore() { openMock.mock.restore(); closeMock.mock.restore(); syncBuiltinESMExports(); } };
}

test('every native acquisition/validation failure preserves all cleanup causes and closes in reverse order', (context) => {
  for (const mirroredBanks of [false, true]) for (const mirroredRoot of [false, true]) for (const failClose of [false, true]) {
    const count = (mirroredBanks ? 5 : 3) + Number(mirroredRoot);
    for (let failed = 0; failed <= count; failed++) {
      const directory = fs.mkdtempSync(join(tmpdir(), 'clink-checkpoint-init-')), path = join(directory, 'journal');
      const trace = observe(context, path, { failed, failClose: () => failClose });
      try {
        assert.throws(() => new FileCheckpointPagedCommitLog(path, { mirroredBanks, mirroredRoot, maxPages: failed === count ? 0 : 64 }), error => {
          const messages = causes(error); assert.match(messages[0], failed === count ? /page limit/ : /original acquisition/);
          assert.deepEqual(messages.slice(1), failClose ? closeOrder(trace.opened).map(name => 'cleanup ' + name) : []); return true;
        });
        assert.deepEqual(trace.closed, closeOrder(trace.opened)); assert.equal(trace.handles.size, 0);
        for (const name of names(path, mirroredBanks, mirroredRoot)) assert.equal(fs.existsSync(name + '.writer'), failClose && trace.opened.includes(name));
      } finally { trace.restore(); fs.rmSync(directory, { recursive: true, force: true }); }
    }
  }
});

test('explicit close attempts all raw handles root last and retains only uncertain owners', (context) => {
  for (const mirroredBanks of [false, true]) for (const mirroredRoot of [false, true]) for (const allFail of [false, true]) {
    const directory = fs.mkdtempSync(join(tmpdir(), 'clink-checkpoint-close-')), path = join(directory, 'journal'), devices = names(path, mirroredBanks, mirroredRoot);
    const bankIndex = mirroredRoot ? 2 : 1;
    const failing = name => allFail || [devices[bankIndex], devices[bankIndex] + '.meta0', devices[bankIndex] + '.meta1', devices[bankIndex] + '.writer'].includes(name);
    const trace = observe(context, path, { failClose: failing });
    try {
      const log = new FileCheckpointPagedCommitLog(path, { mirroredBanks, mirroredRoot }); log.replace(['acknowledged']);
      assert.throws(() => log.close(), error => { assert.deepEqual(causes(error), closeOrder(devices).filter(failing).map(name => 'cleanup ' + name)); return true; });
      assert.deepEqual(trace.closed, closeOrder(devices)); assert.equal(trace.handles.size, 0); log.close(); assert.equal(trace.closed.length, devices.length * 4);
      assert.throws(() => log.read(), /closed/); assert.throws(() => log.epoch, /closed/);
      trace.restore();
      for (const name of devices) assert.equal(fs.existsSync(name + '.writer'), allFail || name === devices[bankIndex]);
      const files = devices.flatMap(name => [name, name + '.meta0', name + '.meta1']), before = files.map(name => fs.readFileSync(name));
      assert.throws(() => new FileCheckpointPagedCommitLog(path, { mirroredBanks, mirroredRoot }), /EEXIST/);
      assert.deepEqual(files.map(name => fs.readFileSync(name)), before);
      // All injected closes released their descriptors before reporting errors.
      for (const name of devices) if (fs.existsSync(name + '.writer')) fs.unlinkSync(name + '.writer');
      using reopened = new FileCheckpointPagedCommitLog(path, { mirroredBanks, mirroredRoot }); assert.deepEqual(JSON.parse(reopened.read()), ['acknowledged']);
    } finally { trace.restore(); fs.rmSync(directory, { recursive: true, force: true }); }
  }
});

test('native checkpoint rejects a nonboolean mirror option before filesystem acquisition', () => {
  assert.throws(() => new FileCheckpointPagedCommitLog('unused', { mirroredBanks: 1 }), /boolean/);
});
