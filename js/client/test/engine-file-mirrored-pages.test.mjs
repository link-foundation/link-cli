import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { syncBuiltinESMExports } from 'node:module';
import { spawnSync } from 'node:child_process';
import { FilePageDevice, FilePagedCommitLog } from '../src/engine-file-paged-log.js';
import { FileMirroredPagedCommitLog } from '../src/engine-file-mirrored-pages.js';
const directory = () => fs.mkdtempSync(join(tmpdir(), 'clink-mirror-'));

test('native mirrors recover committed page/decision damage, short copies and following groups', () => {
  const dir = directory(), path = join(dir, 'log');
  try {
    using first = new FileMirroredPagedCommitLog(path);
    for (const owned of [path, path + '.mirror']) assert.throws(() => new FilePageDevice(owned), /EEXIST/);
    first.append('雪'.repeat(700)); first.flush(); first.append('\ufeff\0😀'); first.flush(); first.close();
    const good = fs.readFileSync(path), meta = fs.readFileSync(path + '.meta0');
    for (const side of ['', '.mirror']) {
      fs.truncateSync(path + side, 19); fs.writeFileSync(path + side + '.meta0', 'torn');
      using recovered = new FileMirroredPagedCommitLog(path);
      assert.deepEqual(JSON.parse(recovered.read()), ['雪'.repeat(700), '\ufeff\0😀']); recovered.close();
      assert.deepEqual(fs.readFileSync(path + side), good); assert.deepEqual(fs.readFileSync(path + side + '.meta0'), meta);
    }
    assert.throws(() => new FilePagedCommitLog(path), /flags|mode/);
    using following = new FileMirroredPagedCommitLog(path); following.append('next'); following.flush(); following.close(); following.close();
    using reopened = new FileMirroredPagedCommitLog(path); assert.deepEqual(JSON.parse(reopened.read()), ['雪'.repeat(700), '\ufeff\0😀', 'next']);
    for (const run of [() => first.read(), () => first.append('closed'), () => first.flush()]) assert.throws(run, /closed/);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('mirror initialization and uncertain close attempt both owned devices without stealing fences', (context) => {
  const dir = directory(), path = join(dir, 'log');
  try {
    fs.writeFileSync(path + '.mirror.writer', 'other owner');
    assert.throws(() => new FileMirroredPagedCommitLog(path), /EEXIST/);
    assert.equal(fs.existsSync(path + '.writer'), false); assert.equal(fs.readFileSync(path + '.mirror.writer', 'utf8'), 'other owner');
    fs.unlinkSync(path + '.mirror.writer');
    fs.writeFileSync(path, 'unrelated'); assert.throws(() => new FileMirroredPagedCommitLog(path), /metadata/);
    assert.equal(fs.readFileSync(path, 'utf8'), 'unrelated'); fs.truncateSync(path, 0);
    using log = new FileMirroredPagedCommitLog(path);
    const close = fs.closeSync; let count = 0;
    const mocked = context.mock.method(fs, 'closeSync', (fd) => { close(fd); count++; throw new Error('injected mirror close'); });
    syncBuiltinESMExports();
    try {
      assert.throws(() => log.close(), AggregateError); assert.equal(count, 8);
      for (const side of ['', '.mirror']) assert.equal(fs.existsSync(path + side + '.writer'), true);
      log.close(); assert.equal(count, 8);
    } finally { mocked.mock.restore(); syncBuiltinESMExports(); }
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('bounded mirror process exits retain acknowledgements and require both owners to be fenced', () => {
  const dir = directory(), adapter = new URL('../src/engine-file-mirrored-pages.js', import.meta.url).href;
  try {
    for (const phase of ['append', 'flush']) {
      const path = join(dir, phase); using first = new FileMirroredPagedCommitLog(path);
      first.append('acknowledged'); first.flush(); first.close();
      const child = spawnSync(process.execPath, ['--max-old-space-size=96', '--input-type=module', '-e',
        `import {FileMirroredPagedCommitLog} from ${JSON.stringify(adapter)}; const log=new FileMirroredPagedCommitLog(process.argv[1]); log.append('pending 雪'); if(process.argv[2]==='flush')log.flush(); process.exit(73);`, path, phase],
      { encoding: 'utf8', timeout: 10000, maxBuffer: 65536 });
      assert.equal(child.error, undefined); assert.equal(child.status, 73, child.stderr);
      assert.throws(() => new FileMirroredPagedCommitLog(path), /EEXIST/);
      for (const side of ['', '.mirror']) fs.unlinkSync(path + side + '.writer'); // Observed exited child, never lock age.
      using reopened = new FileMirroredPagedCommitLog(path);
      const prior = phase === 'flush' ? ['acknowledged', 'pending 雪'] : ['acknowledged'];
      assert.deepEqual(JSON.parse(reopened.read()), prior); reopened.append('following'); reopened.flush(); reopened.close();
      using again = new FileMirroredPagedCommitLog(path); assert.deepEqual(JSON.parse(again.read()), [...prior, 'following']);
    }
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});

test('native repair rejects invalid bounds and holes before changing owned files', () => {
  const dir = directory(), path = join(dir, 'log');
  try {
    using device = new FilePageDevice(path);
    for (const [index, bytes] of [[-1, new Uint8Array(4096)], [0.5, new Uint8Array(4096)],
      [2199023255551, new Uint8Array(4096)], [0, null], [0, new Uint8Array(1)]])
      assert.throws(() => device.repairPage(index, bytes), /repair page/);
    assert.throws(() => device.repairPage(1, new Uint8Array(4096)), /hole/);
    assert.equal(device.dataLength, 0); device.close();
    assert.throws(() => device.repairPage(0, new Uint8Array(4096)), /closed/);
  } finally { fs.rmSync(dir, { recursive: true, force: true }); }
});
