// The JS twin of rust/examples/engine_crash_writer.rs: a writer to be killed by
// .github/scripts/docker-crash.mjs, speaking the same protocol.
//
//   CLINK_WASM=/path/to/web-package node engine-crash-writer.mjs COMMAND ...
//
//   write  PRIMARY REPLICA [COMMITS]  print `ack R` once revision R is durable on
//                                     both files; `error ...` and exit 1 on a
//                                     failure such as a full disk
//   verify JOURNAL                    open it as a restarting writer does and
//                                     print `revision R`, or `refused ...` and exit 2
//   repair JOURNAL [COPY...]          rebuild it from its valid prefix or a copy:
//                                     `repaired R changed true|false`
//
// Revision R holds links 1..=R, each pointing at itself; a torn transaction
// would leave a link pointing at nothing, which `verify` refuses. A killed
// writer leaves its `.writer` lock directory behind: remove it only once the
// process is known to be dead, as the driver does after `docker wait`.
import { readFileSync, writeSync } from 'node:fs';
import { join } from 'node:path';
import process from 'node:process';
import { pathToFileURL } from 'node:url';
import { ReplicatedCommitLog, WasmEngine } from '../src/index.js';
import { FileCommitLog, repairJournalFile } from '../src/engine-file-log.js';

const options = { revisionEncoding: 'changes', requireDurableFlush: true };
// Synchronous, so an acknowledgement is on stdout before the next commit starts.
const print = (line) => writeSync(1, line + '\n');

async function load() {
  const dir = process.env.CLINK_WASM;
  if (!dir) throw new Error('set CLINK_WASM to the wasm-pack web package');
  const binding = await import(pathToFileURL(join(dir, 'clink_wasm.js')).href);
  binding.initSync({ module: readFileSync(join(dir, 'clink_wasm_bg.wasm')) });
  return binding;
}

async function write(binding, primaryPath, replicaPath, commits) {
  using primary = new FileCommitLog(primaryPath);
  using replica = new FileCommitLog(replicaPath);
  const log = new ReplicatedCommitLog(primary, [replica], { acknowledgements: 2 });
  using engine = new WasmEngine(binding, options, log);
  for (let written = 0n; commits === undefined || written < commits; written++) {
    const tx = engine.begin();
    try {
      const link = await tx.create(0n, 0n);
      await tx.update(link.index, link.index, link.index);
      print(`ack ${await tx.commit()}`);
    } finally {
      await tx.close();
    }
  }
}

function verify(binding, path) {
  using log = new FileCommitLog(path);
  using engine = new WasmEngine(binding, options, log);
  const { revision, rows } = engine.snapshot();
  if (BigInt(rows.length) !== revision) return `${rows.length} links at revision ${revision}`;
  for (const [offset, row] of rows.entries()) {
    const expected = BigInt(offset + 1);
    if (row.index !== expected || row.source !== expected || row.target !== expected)
      return `torn transaction: link ${row.index}:${row.source}:${row.target} where ${expected}:${expected}:${expected} was committed`;
  }
  print(`revision ${revision}`);
  return null;
}

const [command, ...args] = process.argv.slice(2);
const oneLine = (error) => String(error?.message ?? error).replace(/\s+/g, ' ');
try {
  const binding = await load();
  if (command === 'write' && (args.length === 2 || args.length === 3)) {
    await write(binding, args[0], args[1], args[2] === undefined ? undefined : BigInt(args[2]));
  } else if (command === 'verify' && args.length === 1) {
    let reason;
    try {
      reason = verify(binding, args[0]);
    } catch (error) {
      reason = oneLine(error);
    }
    if (reason !== null) {
      print(`refused ${reason}`);
      process.exitCode = 2;
    }
  } else if (command === 'repair' && args.length >= 1) {
    const repair = repairJournalFile(
      args[0],
      (journals) => WasmEngine.recoverJournal(binding, journals, options),
      { copies: args.slice(1) },
    );
    print(`repaired ${repair.recovery.revision} changed ${repair.changed}`);
  } else {
    throw new Error(
      'usage: write PRIMARY REPLICA [COMMITS] | verify JOURNAL | repair JOURNAL [COPY...]',
    );
  }
} catch (error) {
  print(`error ${oneLine(error)}`);
  process.exitCode = 1;
}
