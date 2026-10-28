import test from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { spawnSync } from 'node:child_process';
import { FileCheckpointPagedCommitLog } from '../src/engine-file-checkpoint-pages.js';
const probe = fileURLToPath(new URL('../../../experiments/issue-138/native-checkpoint-cut.mjs', import.meta.url));
function seed(path, mirroredBanks, mirroredRoot, phase) {
  using log = new FileCheckpointPagedCommitLog(path, { mirroredBanks, mirroredRoot, maxPages: 64 });
  log.replace(['retired bank']); log.replace(['acknowledged']); log.close();
  if (phase === 'root-repair' || phase === 'both-repair') fs.writeFileSync(path + '.meta0', 'torn root');
  if (phase === 'repair' || phase === 'both-repair') { fs.truncateSync(path + '.bank0', 19); fs.writeFileSync(path + '.bank0.meta0', 'torn'); }
}
function child(path, mirrored, mirroredRoot, point, phase) {
  const result = spawnSync(process.execPath, ['--max-old-space-size=96', probe, path, String(mirrored), String(point), phase, String(mirroredRoot)], { encoding: 'utf8', timeout: 10000, maxBuffer: 65536 });
  assert.equal(result.error, undefined); assert.equal(result.status, point < 0 ? 0 : 73, result.stderr); return result.stdout;
}

for (const [mirroredBanks, mirroredRoot, phase] of [[false, false, 'replace'], [true, false, 'replace'], [true, false, 'repair'],
    [false, true, 'replace'], [true, true, 'replace'], [true, true, 'repair'], [false, true, 'root-repair'], [true, true, 'root-repair'], [true, true, 'both-repair']]) {
  test(`every native before/after process exit: banks=${mirroredBanks} root=${mirroredRoot} ${phase}`, { timeout: 120000 }, () => {
    const directory = fs.mkdtempSync(join(tmpdir(), 'clink-checkpoint-cut-'));
    try {
      const baseline = join(directory, 'baseline'); seed(baseline, mirroredBanks, mirroredRoot, phase);
      const points = JSON.parse(child(baseline, mirroredBanks, mirroredRoot, -1, phase)); assert.ok(points.length >= (phase === 'replace' ? 30 : phase === 'root-repair' ? 6 : 12) && points.length <= 150);
      for (const operation of ['writeSync', 'ftruncateSync', 'fsyncSync']) assert.ok(points.some(point => point.includes(operation)));
      assert.ok(points.some(point => point.startsWith(phase === 'replace' ? '.meta1:writeSync:' : phase === 'root-repair' ? '.meta0:writeSync:' : '.bank0:writeSync:')));
      if (phase === 'both-repair') assert.ok(points.some(point => point.startsWith('.meta0:writeSync:')));
      for (let point = 0; point < points.length; point++) {
        const path = join(directory, `case-${point}`); seed(path, mirroredBanks, mirroredRoot, phase); child(path, mirroredBanks, mirroredRoot, point, phase);
        assert.throws(() => new FileCheckpointPagedCommitLog(path, { mirroredBanks, mirroredRoot }), /EEXIST/);
        const devices = ['', ...(mirroredRoot ? ['.mirror'] : []), '.bank0', ...(mirroredBanks ? ['.bank0.mirror'] : []), '.bank1', ...(mirroredBanks ? ['.bank1.mirror'] : [])].map(suffix => path + suffix);
        // Synchronous child reaping above proves the old owner has exited.
        for (const name of devices) fs.unlinkSync(name + '.writer');
        using recovered = new FileCheckpointPagedCommitLog(path, { mirroredBanks, mirroredRoot, maxPages: 64 });
        const actual = JSON.parse(recovered.read()), old = ['acknowledged'], replacement = ['雪'.repeat(700), '\ufeff\0😀'];
        assert.ok(JSON.stringify(actual) === JSON.stringify(old) || JSON.stringify(actual) === JSON.stringify(replacement), `torn records at point ${point}: ${points[point]}`);
        if (phase !== 'replace') assert.deepEqual(actual, old);
        recovered.append('following'); recovered.flush(); assert.deepEqual(JSON.parse(recovered.read()), [...actual, 'following']);
        recovered.replace(['compacted after exit']); recovered.close();
        using again = new FileCheckpointPagedCommitLog(path, { mirroredBanks, mirroredRoot }); assert.deepEqual(JSON.parse(again.read()), ['compacted after exit']);
      }
      console.log(`native checkpoint mirroredBanks=${mirroredBanks} mirroredRoot=${mirroredRoot} phase=${phase}: ${points.length} bounded before/after process exits`);
    } finally { fs.rmSync(directory, { recursive: true, force: true }); }
  });
}
