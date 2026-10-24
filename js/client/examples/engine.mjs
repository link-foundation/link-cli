// node js/client/examples/engine.mjs [journal-path]
import { readFileSync } from 'node:fs';
import * as binding from '../../../js/pkg/clink_wasm.js';
import { WasmEngine, MemoryCommitLog, ReplicatedCommitLog } from '../src/index.js';
import { FileCommitLog } from '../src/engine-file-log.js';

binding.initSync({ module: readFileSync(new URL('../../../js/pkg/clink_wasm_bg.wasm', import.meta.url)) });
const file = process.argv[2] ? new FileCommitLog(process.argv[2]) : null;
const follower = new MemoryCommitLog();
const log = file ?? new ReplicatedCommitLog(new MemoryCommitLog(), [follower], { acknowledgements: 2 });
const engine = new WasmEngine(binding, { history: 'all', requireDurableFlush: file !== null }, log);
const session = engine.begin();
try {
  const link = await session.create(0n, 0n);
  await session.setName(link.index, 'hello 雪');
  console.log('before commit', engine.snapshot());
  console.log('revision', await session.commit());
  console.log('after commit', engine.snapshot());
} finally { await session.close(); engine.close(); file?.close(); }
