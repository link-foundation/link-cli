import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { FileCheckpointPagedCommitLog } from '../src/engine-file-checkpoint-pages.js';

test('native root mirroring recovers the acknowledged epoch with either bank mode', () => {
  const directory = fs.mkdtempSync(join(tmpdir(), 'clink-checkpoint-root-'));
  try {
    for (const mirroredBanks of [false, true]) {
      const path = join(directory, String(mirroredBanks));
      using initial = new FileCheckpointPagedCommitLog(path, { mirroredBanks, mirroredRoot: true });
      initial.replace(['old']); initial.replace(['acknowledged 雪']); initial.close();
      assert.equal(fs.existsSync(path + '.mirror.meta0'), true);
      fs.writeFileSync(path + '.meta0', 'torn');
      using recovered = new FileCheckpointPagedCommitLog(path, { mirroredBanks, mirroredRoot: true });
      assert.equal(recovered.epoch, 2n); assert.deepEqual(JSON.parse(recovered.read()), ['acknowledged 雪']);
      recovered.append('following'); recovered.flush(); recovered.replace(['next', 'following']); recovered.close();
      assert.deepEqual(fs.readFileSync(path + '.meta1'), fs.readFileSync(path + '.mirror.meta1'));
      using again = new FileCheckpointPagedCommitLog(path, { mirroredBanks, mirroredRoot: true });
      assert.deepEqual(JSON.parse(again.read()), ['next', 'following']);
    }
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});

function deviceNames(path, mirroredBanks, mirroredRoot) {
  return [path, ...(mirroredRoot ? [path + '.mirror'] : []), ...[0, 1].flatMap(index => {
    const bank = path + `.bank${index}`; return [bank, ...(mirroredBanks ? [bank + '.mirror'] : [])];
  })];
}
const filesFor = devices => devices.flatMap(name => [name, name + '.meta0', name + '.meta1']);
const snapshot = files => files.map(name => fs.readFileSync(name));

test('all independently selected native fences precede root and bank repair', () => {
  const directory = fs.mkdtempSync(join(tmpdir(), 'clink-root-fences-'));
  try {
    for (const mirroredRoot of [false, true]) for (const mirroredBanks of [false, true]) {
      const path = join(directory, `${mirroredRoot}-${mirroredBanks}`), options = { mirroredRoot, mirroredBanks };
      using initial = new FileCheckpointPagedCommitLog(path, options);
      initial.replace(['previous']); initial.replace(['acknowledged 雪']); initial.close();
      if (mirroredRoot) fs.writeFileSync(path + '.meta0', 'torn root');
      if (mirroredBanks) { fs.truncateSync(path + '.bank0', 19); fs.writeFileSync(path + '.bank0.meta0', 'torn bank'); }
      const devices = deviceNames(path, mirroredBanks, mirroredRoot), files = filesFor(devices), before = snapshot(files);
      for (const foreign of devices) {
        fs.writeFileSync(foreign + '.writer', 'foreign owner');
        assert.throws(() => new FileCheckpointPagedCommitLog(path, options), /EEXIST/);
        assert.deepEqual(snapshot(files), before);
        for (const name of devices) assert.equal(fs.existsSync(name + '.writer'), name === foreign);
        assert.equal(fs.readFileSync(foreign + '.writer', 'utf8'), 'foreign owner'); fs.unlinkSync(foreign + '.writer');
      }
      using recovered = new FileCheckpointPagedCommitLog(path, options);
      assert.deepEqual(JSON.parse(recovered.read()), ['acknowledged 雪']); recovered.close();
      if (mirroredRoot) assert.deepEqual(fs.readFileSync(path + '.meta0'), fs.readFileSync(path + '.mirror.meta0'));
      if (mirroredBanks) assert.deepEqual(fs.readFileSync(path + '.bank0'), fs.readFileSync(path + '.bank0.mirror'));
      using following = new FileCheckpointPagedCommitLog(path, options);
      following.append('following'); following.flush(); following.replace(['compacted']); following.close();
      using again = new FileCheckpointPagedCommitLog(path, options); assert.deepEqual(JSON.parse(again.read()), ['compacted']);
    }
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});

test('native root and bank mode conversions reject without modifying existing bytes', () => {
  const directory = fs.mkdtempSync(join(tmpdir(), 'clink-root-modes-'));
  try {
    for (const mirroredRoot of [false, true]) for (const mirroredBanks of [false, true]) {
      const path = join(directory, `${mirroredRoot}-${mirroredBanks}`), options = { mirroredRoot, mirroredBanks };
      using initial = new FileCheckpointPagedCommitLog(path, options); initial.replace(['acknowledged']); initial.close();
      const devices = deviceNames(path, mirroredBanks, mirroredRoot), files = filesFor(devices), before = snapshot(files);
      for (const wrong of [{ mirroredRoot: !mirroredRoot, mirroredBanks }, { mirroredRoot, mirroredBanks: !mirroredBanks }]) {
        assert.throws(() => new FileCheckpointPagedCommitLog(path, wrong), /flags|mode/);
        assert.deepEqual(snapshot(files), before);
        for (const name of deviceNames(path, true, true)) assert.equal(fs.existsSync(name + '.writer'), false);
      }
      using again = new FileCheckpointPagedCommitLog(path, options); assert.deepEqual(JSON.parse(again.read()), ['acknowledged']);
    }
    assert.throws(() => new FileCheckpointPagedCommitLog(join(directory, 'invalid'), { mirroredRoot: 1 }), /mirroredRoot must be boolean/);
    assert.equal(fs.existsSync(join(directory, 'invalid')), false);
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});
