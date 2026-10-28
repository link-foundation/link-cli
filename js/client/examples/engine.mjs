// node js/client/examples/engine.mjs [journal-path] [--shared] [--changes] [--delta] [--read-set] [--pessimistic]
import { readFileSync } from 'node:fs';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { WasmEngine, MemoryCommitLog, ReplicatedCommitLog, DeltaCommitLog } from '../src/index.js';
import { FileCommitLog } from '../src/engine-file-log.js';

const moduleUrl = process.env.CLINK_WASM ? pathToFileURL(join(process.env.CLINK_WASM, 'clink_wasm.js')) : new URL('../../../js/pkg/clink_wasm.js', import.meta.url);
const binding = await import(moduleUrl.href);
binding.initSync({ module: readFileSync(new URL('clink_wasm_bg.wasm', moduleUrl)) });
const stateStructure = process.argv.includes('--shared') ? 'shared' : 'flat';
const revisionEncoding = process.argv.includes('--changes') ? 'changes' : 'snapshot';
const path = process.argv.slice(2).find(arg => !arg.startsWith('--'));
const file = path ? new FileCommitLog(path) : null;
const follower = new MemoryCommitLog();
const baseLog = file ?? new ReplicatedCommitLog(new MemoryCommitLog(), [follower], { acknowledgements: 2 });
const log = process.argv.includes('--delta') ? new DeltaCommitLog(baseLog) : baseLog;
const engine = new WasmEngine(binding, { history: 'all', requireDurableFlush: file !== null,
  stateStructure, revisionEncoding,
  concurrency: process.argv.includes('--pessimistic') ? 'pessimistic' : 'optimistic',
  serializableValidation: process.argv.includes('--read-set') ? 'read-set' : 'revision' }, log);
const session = engine.begin();
try {
  const link = await session.create(0n, 0n);
  await session.setName(link.index, 'hello 雪');
  console.log('before commit', engine.snapshot());
  console.log('revision', await session.commit());
  console.log('after commit', engine.snapshot());
  const branch = engine.fork({ stateStructure, revisionEncoding });
  const edit = branch.engine.begin();
  try {
    await edit.setName(link.index, 'edited independently');
    await edit.commit();
    console.log('merged revision', branch.merge());
    // Persist this with application metadata to resume the same branch later.
    console.log('branch merge base', branch.mergeBaseRecord());
  } finally { await edit.close(); branch.close(); }
  if (process.argv.includes('--read-set')) {
    const seed = engine.begin(); let other;
    try { other = await seed.create(0n, 0n); await seed.commit(); }
    finally { await seed.close(); }
    const one = engine.begin(); const two = engine.begin();
    try {
      await one.setName(link.index, 'profile one'); await two.setName(other.index, 'profile two');
      if (process.argv.includes('--pessimistic')) {
        const blocked = engine.begin();
        try {
          await assert.rejects(blocked.removeName(link.index), /busy/i);
          console.log('Competing point write rejected as busy');
        } finally { await blocked.close(); }
      }
      await one.commit(); await two.commit();
      console.log('Independent serializable point edits both committed');
    } finally { await one.close(); await two.close(); }
  }
} finally { await session.close(); engine.close(); file?.close(); }
