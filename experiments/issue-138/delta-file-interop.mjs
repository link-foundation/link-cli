// Build the Rust/C# engine examples and js/pkg first. Finite shared-file probe.
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import * as binding from '../../js/pkg/clink_wasm.js';
import { DeltaCommitLog, MemoryCommitLog, WasmEngine } from '../../js/client/src/index.js';

const root = fileURLToPath(new URL('../../', import.meta.url));
const vectors = JSON.parse(readFileSync(join(root, 'docs/protocol/engine-delta-vectors.json'), 'utf8'));
const directory = mkdtempSync(join(tmpdir(), 'delta-interop-'));
const path = join(directory, 'journal.log');
try {
  writeFileSync(path, vectors.validSequences[0].logical[0] + '\n');
  for (const [name, command, args] of [
    ['rust', join(root, 'rust/target/debug/examples/engine_strategies'), [path, '--delta']],
    ['csharp', 'dotnet', [join(root, 'examples/engine-csharp/bin/Debug/net10.0/EngineExample.dll'), path, '--delta']],
    ['js', process.execPath, [join(root, 'js/client/examples/engine.mjs'), path, '--delta']],
  ]) {
    const result = spawnSync(command, args, { cwd: root, encoding: 'utf8',
      env: { ...process.env, DOTNET_GCHeapHardLimit: '0x28000000' } });
    const log = new URL(`delta-file-interop-${name}.log`, import.meta.url);
    writeFileSync(log, (result.stdout ?? '') + (result.stderr ?? ''));
    assert.equal(result.status, 0, result.error?.message ?? `See ${fileURLToPath(log)}`);
  }
  const records = readFileSync(path, 'utf8').trimEnd().split('\n');
  assert.equal(records.length, 6);
  assert.equal(records.filter(record => record.startsWith('delta1|')).length, 5);
  binding.initSync({ module: readFileSync(join(root, 'js/pkg/clink_wasm_bg.wasm')) });
  const engine = new WasmEngine(binding, { history: 'all' }, new DeltaCommitLog(new MemoryCommitLog(records)));
  try {
    assert.equal(engine.snapshot().revision, 6n);
    assert.equal(engine.snapshot().rows.length, 203);
    assert.equal(engine.snapshot(1n).rows.length, 200);
  } finally { engine.close(); }
  console.log('Rust -> C# -> actual WASM: 6 revisions, 203 rows, 5 deltas; history recovered');
} finally { rmSync(directory, { recursive: true, force: true }); }
