import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { FileCheckpointPagedCommitLog } from '../src/engine-file-checkpoint-pages.js';
import { FilePageDevice } from '../src/engine-file-paged-log.js';

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
      for (const run of [() => log.read(), () => log.append('closed'), () => log.flush(), () => log.replace([])]) assert.throws(run, /closed/);
      using reopened = new FileCheckpointPagedCommitLog(path, { mirroredBanks });
      assert.deepEqual(JSON.parse(reopened.read()), ['acknowledged 雪']);
      reopened.append('following'); reopened.flush(); assert.equal(reopened.replace(['compacted']), true); reopened.close();
      using again = new FileCheckpointPagedCommitLog(path, { mirroredBanks }); assert.deepEqual(JSON.parse(again.read()), ['compacted']);
    }
  } finally { fs.rmSync(directory, { recursive: true, force: true }); }
});
