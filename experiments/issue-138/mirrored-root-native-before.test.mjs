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
