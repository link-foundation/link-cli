// node js/client/examples/engine.mjs [journal-path] [--delta] [--read-set]
import { readFileSync } from 'node:fs';
import * as binding from '../../../js/pkg/clink_wasm.js';
import { WasmEngine, MemoryCommitLog, ReplicatedCommitLog, DeltaCommitLog } from '../src/index.js';
import { FileCommitLog } from '../src/engine-file-log.js';

binding.initSync({ module: readFileSync(new URL('../../../js/pkg/clink_wasm_bg.wasm', import.meta.url)) });
const path = process.argv.slice(2).find(arg => !arg.startsWith('--'));
const file = path ? new FileCommitLog(path) : null;
const follower = new MemoryCommitLog();
const baseLog = file ?? new ReplicatedCommitLog(new MemoryCommitLog(), [follower], { acknowledgements: 2 });
const log = process.argv.includes('--delta') ? new DeltaCommitLog(baseLog) : baseLog;
const engine = new WasmEngine(binding, { history: 'all', requireDurableFlush: file !== null,
  serializableValidation: process.argv.includes('--read-set') ? 'read-set' : 'revision' }, log);
const session = engine.begin();
try {
  const link = await session.create(0n, 0n);
  await session.setName(link.index, 'hello 雪');
  console.log('before commit', engine.snapshot());
  console.log('revision', await session.commit());
  console.log('after commit', engine.snapshot());
  const branch = engine.fork();
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
      await one.commit(); await two.commit();
      console.log('Independent serializable point edits both committed');
    } finally { await one.close(); await two.close(); }
  }
} finally { await session.close(); engine.close(); file?.close(); }
