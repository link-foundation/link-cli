import { LinksClient, LinksDecorator, createWasmClient, ANY, type Links } from '../types/index.js';
import { connectTcp } from '../types/tcp.js';
import { connectProcess } from '../types/process.js';
class Logging extends LinksDecorator {
  override async create(source: bigint | number, target: bigint | number) { return super.create(source, target); }
}
async function main(links: Links) {
  const link = await links.create(1n, 1);
  const n: bigint = link ? await links.count([ANY, link.index]) : 0n;
  const r = await links.batch([{ op: 'create', source: 1, target: 2 }], { timeout: 1000 });
  if (!r.completed) throw r.firstError;
  const c = await connectTcp('127.0.0.1:1', { verify: true, deadline: new Date() });
  const p = await connectProcess({ command: 'clink', db: 'a b.links' });
  const w = createWasmClient({ request: (t: string) => t });
  new Logging(w).require('rawTcp');
  await using x = c;
  return [n, p];
}

import { WasmEngine, EngineClient, MemoryCommitLog, ReplicatedCommitLog, type EngineOptions, type StagedEngineLike, type CommitLog } from '../types/index.js';
import { FileCommitLog } from '../types/engine-file-log.js';
async function engineTypes(binding: { StagedEngine: { new(options: string, journal: string): StagedEngineLike; withLog(options: string, log: CommitLog): StagedEngineLike } }) {
  const options: EngineOptions = { isolation: 'snapshot', concurrency: 'exclusive', history: 'all', constraints: ['unique-names'], requireDurableFlush: true };
  using file = new FileCommitLog('journal');
  const replica = new MemoryCommitLog();
  const log: CommitLog = new ReplicatedCommitLog(file, [replica], { acknowledgements: 1 });
  using engine = new WasmEngine(binding, options, log);
  const session: EngineClient = engine.begin();
  await using owned = session;
  await session.create(0n, 0n);
  const revision: bigint = await session.commit();
  using fork = engine.fork({ history: 1 }, new MemoryCommitLog());
  const branchSession: EngineClient = fork.engine.begin();
  await branchSession.close();
  const merged: bigint = fork.merge();
  const descriptor: string = fork.mergeBaseRecord();
  using resumed = engine.resumeBranch(fork.engine.journal(), descriptor);
  resumed.merge();
  return engine.snapshot(revision);
}
