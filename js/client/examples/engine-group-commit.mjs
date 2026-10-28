// CLINK_WASM=/absolute/path/to/web-package node js/client/examples/engine-group-commit.mjs <journal> [--delta]
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { WasmEngine, DeltaCommitLog } from '../src/index.js';
import { FileCommitLog } from '../src/engine-file-log.js';

const dir = process.env.CLINK_WASM;
if (!dir || !process.argv[2]) throw new Error('provide CLINK_WASM and an exclusive journal path');
const binding = await import(pathToFileURL(join(dir, 'clink_wasm.js')).href);
binding.initSync({ module: readFileSync(join(dir, 'clink_wasm_bg.wasm')) });
using file = new FileCommitLog(process.argv[2]);
using engine = new WasmEngine(binding, { isolation: 'snapshot', history: 'all', requireDurableFlush: true },
  process.argv.includes('--delta') ? new DeltaCommitLog(file) : file);
await using first = engine.begin(); await using second = engine.begin();
const a = await first.create(0n, 0n); await first.setName(a.index, 'first independent session');
const b = await second.create(a.index, 0n); await second.setName(b.index, 'second independent session');
for (const outcome of await engine.commitGroup([first, second])) {
  if (outcome.status === 'indeterminate') throw new Error(`${outcome.error}; inspect reopened state before retrying`);
  console.log(outcome.status === 'committed' ? `acknowledged revision ${outcome.revision}` : `known abort: ${outcome.error}`);
}
