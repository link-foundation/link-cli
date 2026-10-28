// Independently retain the C# baseline trace counts suppressed by minimal VSTest output.
// Full tests separately execute every observed before/after process-exit cut.
import assert from 'node:assert/strict';
import fs from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { FileCheckpointPagedCommitLog } from '../../js/client/src/engine-file-checkpoint-pages.js';

const assembly = resolve('csharp/Foundation.Data.Doublets.Cli.Tests/bin/Release/net10.0/Foundation.Data.Doublets.Cli.Tests.dll');
const scenarios = [[false, false, 'replace', 34], [true, false, 'replace', 62], [true, false, 'repair', 12],
  [false, true, 'replace', 40], [true, true, 'replace', 68], [true, true, 'repair', 12],
  [false, true, 'root-repair', 6], [true, true, 'root-repair', 6], [true, true, 'both-repair', 18]];
const directory = fs.mkdtempSync(join(tmpdir(), 'checkpoint-cut-baselines-'));
let total = 0;
try {
  for (const [mirroredBanks, mirroredRoot, phase, expected] of scenarios) {
    const path = join(directory, `case-${total}`);
    using log = new FileCheckpointPagedCommitLog(path, { mirroredBanks, mirroredRoot, maxPages: 64 });
    log.replace(['retired bank']); log.replace(['acknowledged']); log.close();
    if (phase === 'root-repair' || phase === 'both-repair') fs.writeFileSync(path + '.meta0', 'torn root');
    if (phase === 'repair' || phase === 'both-repair') { fs.truncateSync(path + '.bank0', 19); fs.writeFileSync(path + '.bank0.meta0', 'torn'); }
    const child = spawnSync('dotnet', [assembly, 'checkpoint-pages-cut', path, String(mirroredBanks), '-1', phase, String(mirroredRoot)],
      { env: { ...process.env, DOTNET_GCHeapHardLimit: '0x06000000' }, encoding: 'utf8', timeout: 10000, maxBuffer: 65536 });
    assert.equal(child.error, undefined); assert.equal(child.status, 0, child.stderr);
    const points = JSON.parse(child.stdout); assert.equal(points.length, expected);
    for (const operation of ['write', 'truncate', 'sync']) assert.ok(points.some(point => point.includes(operation)));
    console.log(JSON.stringify({ mirroredBanks, mirroredRoot, phase, points: points.length, trace: points }));
    total += points.length;
  }
  assert.equal(total, 258);
  console.log(`C# baseline traces: ${scenarios.length} independent scenarios; ${total} actual before/after process-exit cuts.`);
} finally { fs.rmSync(directory, { recursive: true, force: true }); }
