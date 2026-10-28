import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { FileCheckpointPagedCommitLog } from '../src/engine-file-checkpoint-pages.js';
import { FilePageDevice, FilePagedCommitLog } from '../src/engine-file-paged-log.js';

test('native checkpoint factory owns every fixed device and never flushes on close', () => {
  const directory = fs.mkdtempSync(join(tmpdir(), 'clink-checkpoint-'));
  try {
    for (const mirroredBanks of [false, true]) {
      const path = join(directory, String(mirroredBanks));
      using log = new FileCheckpointPagedCommitLog(path, { mirroredBanks, maxPages: 64 });
      assert.equal(log.supportsReplacement, true); assert.equal(log.supportsDurableFlush, true);
      const suffixes = mirroredBanks ? ['', '.bank0', '.bank0.mirror', '.bank1', '.bank1.mirror'] : ['', '.bank0', '.bank1'];
      for (const suffix of suffixes) assert.throws(() => new FilePageDevice(path + suffix), /EEXIST/);
      assert.equal(log.replace(['acknowledged 雪']), true); assert.equal(log.replace(['acknowledged 雪']), false);
      log.append('private pending'); log.close(); log.close();
      for (const run of [() => log.epoch, () => log.read(), () => log.append('closed'), () => log.flush(), () => log.replace([])]) assert.throws(run, /closed/);
      using reopened = new FileCheckpointPagedCommitLog(path, { mirroredBanks });
      assert.deepEqual(JSON.parse(reopened.read()), ['acknowledged 雪']);
      reopened.append('following'); reopened.flush(); assert.equal(reopened.replace(['compacted']), true); reopened.close();
      using again = new FileCheckpointPagedCommitLog(path, { mirroredBanks }); assert.deepEqual(JSON.parse(again.read()), ['compacted']);
    }
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});

test('selected checkpoint ownership preserves ordinary journal bytes and rejects implicit conversion', () => {
  const directory = fs.mkdtempSync(join(tmpdir(), 'clink-checkpoint-format-'));
  try {
    for (const mirroredBanks of [false, true]) {
      const path = join(directory, String(mirroredBanks));
      using ordinary = new FilePagedCommitLog(path); ordinary.append('ordinary 雪'); ordinary.flush(); ordinary.close();
      const files = [path, path + '.meta0', path + '.meta1'], before = files.map(name => fs.readFileSync(name));
      assert.throws(() => new FileCheckpointPagedCommitLog(path, { mirroredBanks }));
      assert.deepEqual(files.map(name => fs.readFileSync(name)), before);
      assert.equal(fs.existsSync(path + '.writer'), false);
      using reopened = new FilePagedCommitLog(path); assert.deepEqual(JSON.parse(reopened.read()), ['ordinary 雪']);
      reopened.append('following'); reopened.flush();
      const selected = join(directory, `checkpoint-${mirroredBanks}`);
      using checkpoint = new FileCheckpointPagedCommitLog(selected, { mirroredBanks }); checkpoint.replace(['selected']); checkpoint.close();
      assert.throws(() => new FilePagedCommitLog(selected));
      using again = new FileCheckpointPagedCommitLog(selected, { mirroredBanks }); assert.deepEqual(JSON.parse(again.read()), ['selected']);
    }
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});

test('every native fence is acquired before any selected-bank repair or reset', () => {
  const directory = fs.mkdtempSync(join(tmpdir(), 'clink-checkpoint-fences-'));
  try {
    for (const mirroredBanks of [false, true]) {
      const path = join(directory, String(mirroredBanks));
      using initial = new FileCheckpointPagedCommitLog(path, { mirroredBanks });
      initial.replace(['previous']); initial.replace(['acknowledged 雪']);
      const selected = path + `.bank${initial.epoch % 2n}`; initial.close();
      if (mirroredBanks) { fs.truncateSync(selected, 19); fs.writeFileSync(selected + '.meta0', 'torn'); }
      const devices = ['', '.bank0', ...(mirroredBanks ? ['.bank0.mirror'] : []), '.bank1', ...(mirroredBanks ? ['.bank1.mirror'] : [])].map(suffix => path + suffix);
      const files = devices.flatMap(name => [name, name + '.meta0', name + '.meta1']);
      const before = files.map(name => fs.readFileSync(name));
      for (const foreign of devices) {
        fs.writeFileSync(foreign + '.writer', 'foreign owner');
        assert.throws(() => new FileCheckpointPagedCommitLog(path, { mirroredBanks }), /EEXIST/);
        assert.deepEqual(files.map(name => fs.readFileSync(name)), before);
        for (const name of devices) assert.equal(fs.existsSync(name + '.writer'), name === foreign);
        assert.equal(fs.readFileSync(foreign + '.writer', 'utf8'), 'foreign owner'); fs.unlinkSync(foreign + '.writer');
      }
      using recovered = new FileCheckpointPagedCommitLog(path, { mirroredBanks });
      assert.deepEqual(JSON.parse(recovered.read()), ['acknowledged 雪']);
      if (mirroredBanks) assert.deepEqual(fs.readFileSync(selected), fs.readFileSync(selected + '.mirror'));
      recovered.append('following'); recovered.flush(); assert.equal(recovered.replace(['compacted']), true); recovered.close();
      using again = new FileCheckpointPagedCommitLog(path, { mirroredBanks }); assert.deepEqual(JSON.parse(again.read()), ['compacted']);
    }
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});
