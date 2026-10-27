import { CausalSession, type CausalWorkspaceBinding, type CausalPreparedBatch } from '../types/engine-causal-session.js';
async function causalSessionTypes(binding: CausalWorkspaceBinding, batch: CausalBatch) {
  const tx = await CausalSession.open(binding, batch, '32', '33', 'reject', { stateStructure: 'shared' });
  const client = tx.client();
  await client.create(18446744073709551615n, 0n);
  await client.query("(('4294967312': $s $t)) ()");
  const address: string = await tx.createLink('0', '18446744073709551615');
  const prepared: CausalPreparedBatch = await tx.prepare('00000000000000000000000000000064');
  // @ts-expect-error Addresses preserve the full canonical u64 range.
  await tx.ensureCreated(32);
  // @ts-expect-error A causal publication requires an explicit token.
  await tx.commit();
  // @ts-expect-error Private staging does not configure distributed isolation.
  await CausalSession.open(binding, batch, '32', '33', 'reject', { isolation: 'serializable' });
  await tx.close(); return [address, prepared];
}
import { CausalProjection, type CausalProjectedRow } from '../types/engine-causal-projection.js';
function causalProjectionTypes(values: readonly CausalValue[]) {
  const projection = new CausalProjection(values, 'max-dot', { maxVersions: 64 });
  const rows: CausalProjectedRow[] = projection.rows();
  const retained: CausalValue[] = projection.versions();
  // @ts-expect-error Conflict policy is explicit.
  new CausalProjection(values);
  // @ts-expect-error Policy cannot silently fall back.
  new CausalProjection(values, 'latest');
  // @ts-expect-error Row indexes preserve the full canonical u64 range.
  const invalid: CausalProjectedRow = { index: 1, source: '0', target: '0', name: null };
  void invalid;
  return [rows, retained, projection.selected()];
}
import { LinksClient, LinksDecorator, createWasmClient, ANY, type Links } from '../types/index.js';
import { connectTcp } from '../types/tcp.js';
import { connectProcess } from '../types/process.js';
import { CausalVersion, type CausalValue } from '../types/engine-causal-version.js';
import { CausalReplica, type CausalSnapshot } from '../types/engine-causal-replica.js';
import { CausalBatch, type CausalBatchSnapshot } from '../types/engine-causal-batch.js';
import { CausalMerkleSnapshot, pullCausal, CausalPullCursor, type CausalPullProgress } from '../types/engine-causal-merkle.js';
async function causalBatchTypes(store: import('../types/index.js').AtomicJournalStoreProvider) {
  const batch = new CausalBatch(store, 'replica', 'batches', '00000000000000000000000000000001');
  const state: CausalBatchSnapshot = await batch.snapshot();
  const result: CausalValue[] = await batch.write('00000000000000000000000000000064', state.generation,
    [{ address: '1', observed: state.versions, payload: null }]);
  // @ts-expect-error Base generations preserve the canonical u64 range.
  await batch.write('00000000000000000000000000000064', 0, []);
  // @ts-expect-error Edit addresses preserve the canonical u64 range.
  await batch.write('00000000000000000000000000000064', '0', [{ address: 1, observed: [], payload: null }]);
  return result;
}
async function causalMerkleTypes(owner: CausalReplica) {
  const source = await CausalMerkleSnapshot.create((await owner.snapshot()).versions, { maxVersions: 64 });
  const leaf = await CausalMerkleSnapshot.decodeLeaf(source.root,
    await CausalMerkleSnapshot.encodeLeaf(source.leaf('18446744073709551615')));
  const result: CausalPullProgress = await pullCausal(source, owner, { maxLeafRequests: 1 });
  await pullCausal(source, owner, { continuation: result.continuation });
  // @ts-expect-error Continuations have an opaque owned frontier.
  new CausalPullCursor();
  // @ts-expect-error A frontier cannot be fabricated as a plain object.
  await pullCausal(source, owner, { continuation: { root: source.root } });
  // @ts-expect-error A snapshot root is immutable.
  source.root = leaf.root;
  // @ts-expect-error Addresses preserve the entire canonical u64 range.
  source.leaf(1);
  return result;
}
async function causalReplicaTypes(store: import('../types/index.js').AtomicJournalStoreProvider) {
  const owner = new CausalReplica(store, 'replica', '00000000000000000000000000000001', { requireDurableFlush: true });
  const snapshot: CausalSnapshot = await owner.snapshot();
  const changed: boolean = await owner.receive(snapshot.versions);
  await owner.write('00000000000000000000000000000001', '1', await owner.read('1'), null);
  // @ts-expect-error Addresses must preserve the complete canonical u64 range.
  await owner.read(1);
  // @ts-expect-error A coherent atomic provider is required.
  new CausalReplica({}, 'replica', snapshot.author);
  return changed;
}
function causalTypes(wire: string) {
  const value: CausalValue = CausalVersion.decode(wire);
  const merged: CausalValue[] = CausalVersion.merge([value], []);
  const next: CausalValue = CausalVersion.advance(value.address, value.dot.author,
    value.dot.counter, merged, { source: '0', target: '0', name: '雪' });
  // @ts-expect-error Numbers cannot preserve every canonical u64 counter.
  CausalVersion.advance('1', value.dot.author, 1, [], null);
  // @ts-expect-error Causal row endpoints use canonical decimal strings.
  CausalVersion.advance('1', value.dot.author, '0', [], { source: 1, target: '0', name: null });
  return CausalVersion.encode(next);
}
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

import { WasmEngine, EngineClient, MemoryCommitLog, ReplicatedCommitLog, DeltaCommitLog, type EngineOptions, type StagedEngineLike, type CommitLog } from '../types/index.js';
import { FileCommitLog } from '../types/engine-file-log.js';
import { FileCheckpointPagedCommitLog } from '../types/engine-file-checkpoint-pages.js';
function nativeCheckpointTypes(path: string) {
  using log = new FileCheckpointPagedCommitLog(path, { mirroredBanks: true, mirroredRoot: true, maxPages: 64 });
  const replaceable: ReplaceableCommitLog = log;
  const durable: true = log.supportsDurableFlush;
  const changed: boolean = replaceable.replace(['雪']); const epoch: bigint = log.epoch;
  // @ts-expect-error Exact boolean selection rejects ambiguous modes.
  new FileCheckpointPagedCommitLog(path, { mirroredBanks: 'yes' });
  // @ts-expect-error root selection is boolean, independently of bank selection
  new FileCheckpointPagedCommitLog(path, { mirroredRoot: 1 });
  return { durable, changed, epoch };
}
import { SqliteCommitLog, type SqliteConnection } from '../types/engine-sqlite-log.js';
function sqliteTypes(connection: SqliteConnection) {
  using log = new SqliteCommitLog(connection, { busyTimeoutMs: 0, autoCheckpointPages: 0 });
  const commitLog: CommitLog = log;
  commitLog.append('opaque'); commitLog.flush();
  const busy: number = log.checkpoint('truncate').busy;
  // @ts-expect-error Invalid maintenance modes must not silently choose one.
  log.checkpoint('invalid');
  return busy;
}
async function engineTypes(binding: { StagedEngine: { new(options: string, journal: string): StagedEngineLike; withLog(options: string, log: CommitLog): StagedEngineLike } }) {
  const pointOptions: EngineOptions = { serializableValidation: 'read-set' };
  const indexedOptions: EngineOptions = { constraintValidation: 'indexed', constraints: ['references', 'unique-pairs', 'unique-names'] };
  const scanOptions: EngineOptions = { constraintValidation: 'scan' };
  void indexedOptions; void scanOptions;
  // @ts-expect-error Unknown constraint algorithms must not silently fall back.
  const invalidConstraints: EngineOptions = { constraintValidation: 'disabled' };
  void invalidConstraints;
  const sharedOptions: EngineOptions = { stateStructure: 'shared' };
  const flatOptions: EngineOptions = { stateStructure: 'flat' };
  const runOptions: EngineOptions = { stateStructure: 'sorted-runs' };
  void runOptions;
  void sharedOptions; void flatOptions;
  // @ts-expect-error Unknown state structures must not silently select a policy.
  const invalidStructure: EngineOptions = { stateStructure: 'mutable' };
  void invalidStructure;
  const changeOptions: EngineOptions = { revisionEncoding: 'changes', stateStructure: 'shared' };
  const snapshotOptions: EngineOptions = { revisionEncoding: 'snapshot' };
  void changeOptions; void snapshotOptions;
  // @ts-expect-error Unknown mandatory encodings must not select a fallback.
  const invalidEncoding: EngineOptions = { revisionEncoding: 'delta' };
  void invalidEncoding;
  new WasmEngine(binding, pointOptions).close();
  // @ts-expect-error unknown validator
  const invalidValidation: EngineOptions = { serializableValidation: 'none' };
  void invalidValidation;
  const options: EngineOptions = { isolation: 'snapshot', concurrency: 'pessimistic', history: 'all', constraints: ['unique-names'], requireDurableFlush: true };
  using file = new FileCommitLog('journal');
  const replica = new MemoryCommitLog();
  const log: CommitLog = new DeltaCommitLog(new ReplicatedCommitLog(file, [replica], { acknowledgements: 1 }), { checkpointInterval: 128 });
  using engine = new WasmEngine(binding, options, log);
  const checkpoint: string[] = engine.exportCheckpoint(); void checkpoint;
  const changed: boolean = engine.compactJournal(); void changed;
  const replaceable = new DeltaCommitLog(new MemoryCommitLog());
  const supported: boolean = replaceable.supportsReplacement; void supported;
  const replaced: boolean = replaceable.replace(checkpoint); void replaced;
  // @ts-expect-error replacement is synchronous and requires string records
  replaceable.replace([1]);
  const session: EngineClient = engine.begin();
  await using owned = session;
  await session.create(0n, 0n);
  const revision: bigint = await session.commit();
  const grouped = await engine.commitGroup([engine.begin(), engine.begin()]);
  for (const outcome of grouped) {
    if (outcome.status === 'committed') { const n: bigint = outcome.revision; void n; }
    else { const message: string = outcome.error; void message; }
  }
  // @ts-expect-error one session is not an array
  engine.commitGroup(session);
  using fork = engine.fork({ history: 1 }, new MemoryCommitLog());
  const branchSession: EngineClient = fork.engine.begin();
  await branchSession.close();
  const merged: bigint = fork.merge();
  const descriptor: string = fork.mergeBaseRecord();
  using resumed = engine.resumeBranch(fork.engine.journal(), descriptor);
  resumed.merge();
  return engine.snapshot(revision);
}

import { ClusterEngine, EtcdJournalStore, PartitionedJournalStore, type JournalVersion, type AtomicJournalStore } from '../types/index.js';
import type { JournalMaintenanceStore, JournalNamespaceEntry, JournalScanOptions } from '../types/index.js';
async function maintenanceTypes() {
  const maintenance: JournalMaintenanceStore = new EtcdJournalStore(['https://localhost:2379']);
  const options: JournalScanOptions = { maxKeys: 64, maxBytes: 4096, pageKeys: 8, keysOnly: true };
  const snapshot: JournalNamespaceEntry[] = await maintenance.scanNamespace(options);
  const deleted: boolean = await maintenance.compareDelete(snapshot[0].key, snapshot[0].version.generation);
  // @ts-expect-error Non-reusable generations use canonical decimal strings.
  await maintenance.compareDelete('a', 7);
  // @ts-expect-error Ordinary atomic stores do not require maintenance methods.
  await (maintenance as AtomicJournalStore).scanNamespace();
  return deleted;
}
async function clusterTypes(binding: ConstructorParameters<typeof WasmEngine>[0]) {
  const store: AtomicJournalStore = new EtcdJournalStore(['https://localhost:2379'], { headers: { Authorization: 'token' }, timeoutMs: 1000 });
  const versions: JournalVersion[] = await store.read(['a']);
  await store.compareExchange(['a'], versions, [null]);
  const engine = new ClusterEngine(binding, store, { requireDurableFlush: true, journalCheckpointInterval: 64 });
  // @ts-expect-error Checkpoint intervals are numeric revision counts.
  new ClusterEngine(binding, store, { journalCheckpointInterval: '64' });
  const partitioned: AtomicJournalStore = new PartitionedJournalStore(store, { east: store }, () => 'east');
  const cached: AtomicJournalStore = new PartitionedJournalStore(store, { east: store }, () => 'east', { cacheBytes: 65536 });
  const chunked: AtomicJournalStore = new PartitionedJournalStore(store, { east: store }, () => 'east', { chunks: { maxRecords: 64, maxBytes: 65536, maxChunks: 1024 } });
  void chunked;
  // @ts-expect-error chunk byte limits are numbers
  new PartitionedJournalStore(store, { east: store }, () => 'east', { chunks: { maxBytes: '65536' } });
  await cached.read(['a']);
  // @ts-expect-error Cache budgets are numeric UTF-8 payload budgets.
  new PartitionedJournalStore(store, { east: store }, () => 'east', { cacheBytes: '65536' });
  const routed = new ClusterEngine(binding, partitioned);
  await (await routed.begin(['a'])).close();
  await using transaction = await engine.begin(['a', 'b']);
  const links: Links = transaction.shard('a');
  await links.create(0, 0); await transaction.commit();
}

import { PagedCommitLog, type PageDevice } from '../types/engine-paged-log.js';
import { FilePagedCommitLog } from '../types/engine-file-paged-log.js';
import { CheckpointPagedCommitLog, type ReplaceableCommitLog } from '../types/engine-checkpoint-pages.js';
function inspectCheckpointTypes(root: PageDevice, first: PageDevice, second: PageDevice) {
  const records: string[] = CheckpointPagedCommitLog.inspect(root, [first, second], { maxPages:128 });
  records.push('owned');
  // @ts-expect-error both banks are required
  CheckpointPagedCommitLog.inspect(root, [first]);
  // @ts-expect-error page bounds use portable numbers
  CheckpointPagedCommitLog.inspect(root, [first,second], { maxPages:1n });
}
function checkpointPageTypes(root: PageDevice, first: PageDevice, second: PageDevice) {
  const log: ReplaceableCommitLog = new CheckpointPagedCommitLog(root, [first, second], { maxPages: 1024 });
  const replaced: boolean = log.replace(['opaque'] as const);
  // @ts-expect-error A complete replacement takes an ordered array of strings.
  log.replace('opaque');
  // @ts-expect-error There are exactly two physical banks.
  new CheckpointPagedCommitLog(root, [first]);
  return replaced;
}
function nativePageTypes(device: PageDevice) {
  const portable: CommitLog = new PagedCommitLog(device, { maxPages: 1024 });
  using native = new FilePagedCommitLog('native-pages', { maxPages: 1024 });
  const physical: CommitLog = native;
  physical.append('opaque'); physical.flush();
  // @ts-expect-error Page limits must be numeric.
  new PagedCommitLog(device, { maxPages: '1024' });
  return portable.read();
}

import type { RepairPageDevice } from '../types/engine-paged-log.js';
import { MirroredPageDevice } from '../types/engine-mirrored-pages.js';
import { FileMirroredPagedCommitLog } from '../types/engine-file-mirrored-pages.js';
function mirroredPageTypes(primary: RepairPageDevice, secondary: RepairPageDevice, plain: PageDevice) {
  const device: PageDevice = new MirroredPageDevice(primary, secondary);
  const mode: 0 | 1 | 2 | 3 | undefined = device.metadataFlags;
  device.readCheckedPage?.(0, 0n);
  using file = new FileMirroredPagedCommitLog('mirrored-pages');
  const log: CommitLog = new PagedCommitLog(device);
  // @ts-expect-error Mirroring requires an explicit repair capability.
  new MirroredPageDevice(plain, secondary);
  // @ts-expect-error Checksums preserve all 64 bits.
  device.readCheckedPage?.(0, 0);
  // @ts-expect-error Unsupported format modes cannot silently select a fallback.
  const unsupported: PageDevice = { ...primary, metadataFlags: 4 };
  void unsupported;
  return [mode, file.read(), log.read()];
}

import { MirroredCheckpointRoot } from '../types/engine-checkpoint-root.js';
function checkpointRootTypes(primary: PageDevice, secondary: PageDevice, banks: [PageDevice, PageDevice]) {
  const root: PageDevice = new MirroredCheckpointRoot(primary, secondary, 0);
  const log = new CheckpointPagedCommitLog(root, banks);
  // @ts-expect-error Bank mode must be explicitly single or mirrored.
  new MirroredCheckpointRoot(primary, secondary, 2);
  return log.read();
}

import { ReclaimingPartitionedJournalStore, type ReclamationOptions, type ReclamationResult } from '../types/index.js';
async function reclamationTypes(provider: JournalMaintenanceStore) {
  const options: ReclamationOptions = { maxKeys: 4096, maxBytes: 8388608, pageKeys: 128, cacheBytes: 0 };
  const store = await ReclaimingPartitionedJournalStore.open(provider, { east: provider }, () => 'east', options);
  const atomic: AtomicJournalStore = store; const initial = await atomic.read(['a']);
  await atomic.compareExchange(['a'], initial, [['record']]); const result: ReclamationResult | null = await store.collectGarbage();
  // @ts-expect-error Dedicated metadata must be opened and validated before use.
  new ReclaimingPartitionedJournalStore();
  // @ts-expect-error Epochs preserve every u64 bit as canonical decimal strings.
  const invalid: number | undefined = result?.epoch;
  return { result, invalid, durable: store.supportsDurableFlush };
}
void reclamationTypes;

import { SortedJournalTable, type SortedJournalEntry } from '../types/engine-sorted-table.js';
function sortedTableTypes(device: PageDevice) {
  const rows: readonly SortedJournalEntry[] = [{ key: 'a', generation: '18446744073709551615', entries: null }];
  const table = SortedJournalTable.create(device, rows, { maxKeys: 4096, maxBytes: 8388608 });
  const digest: string = table.digest;
  const entry: SortedJournalEntry | undefined = SortedJournalTable.open(device).get('a');
  // @ts-expect-error Construction requires complete physical validation through the factories.
  new SortedJournalTable();
  // @ts-expect-error Exact generations use decimal strings.
  SortedJournalTable.create(device, [{ key: 'a', generation: 1, entries: [] }]);
  // @ts-expect-error Admission bounds are numbers.
  SortedJournalTable.open(device, { maxBytes: '8388608' });
  return { entry, digest, values: table.entries() };
}
void sortedTableTypes;

import { LsmWal, lsmOptions } from '../types/engine-lsm-wal.js';
function lsmWalTypes(records: readonly string[]) {
  const state = LsmWal.recover(records, { bufferKeys: 32 });
  const generation: string = state.generation;
  const encoded: string = LsmWal.encode(state);
  // @ts-expect-error Counters preserve every u64 bit as decimal strings.
  state.generation = 1;
  // @ts-expect-error WAL entries are strings.
  LsmWal.recover([1]);
  // @ts-expect-error Admission bounds are numeric.
  lsmOptions({ maxRuns: '4' });
  return { generation, encoded, options: lsmOptions() };
}
void lsmWalTypes;


import { PhysicalLsmJournalStore, type LsmTableNamespace } from '../types/engine-lsm-store.js';
import { FileLsmJournalStore, type FileLsmOptions } from '../types/engine-file-lsm-store.js';
function nativeLsmTypes(path: string) {
  const options: FileLsmOptions = { maxPages:128, mirroredRoot:true, mirroredBanks:true, mirroredTables:true, bufferKeys:4 };
  using store = new FileLsmJournalStore(path, options);
  const current: JournalVersion[] = store.read(['a']);
  const changed: boolean = store.compareExchange(['a'], current, [['owned']]);
  store.flush(); store.compact();
  const deleted: boolean = store.compareDelete('a', current[0].generation);
  // @ts-expect-error native modes are explicit booleans
  new FileLsmJournalStore(path, { mirroredTables:1 });
  // @ts-expect-error page admission uses finite portable numbers
  new FileLsmJournalStore(path, { maxPages:1n });
}
import type { SynchronousJournalMaintenanceStore } from '../types/index.js';
async function physicalLsmTypes(wal: ReplaceableCommitLog, namespace: LsmTableNamespace, binding: ConstructorParameters<typeof WasmEngine>[0]) {
  using store = PhysicalLsmJournalStore.open(wal, namespace, { bufferKeys: 16, maxNamespaceEntries: 256 });
  const maintenance: SynchronousJournalMaintenanceStore = store;
  const before: JournalVersion[] = maintenance.read(['a']);
  const changed: boolean = maintenance.compareExchange(['a'], before, [['record']]);
  const engine = new ClusterEngine(binding, store);
  const partitioned = new PartitionedJournalStore(store, { east: store }, () => 'east');
  const reclaiming = await ReclaimingPartitionedJournalStore.open(store, { east: store }, () => 'east');
  await ReclaimingPartitionedJournalStore.open(store, { east: store }, () => 'east', { chunks: { maxRecords: 64, maxBytes: 65536, maxChunks: 1024 }, cacheBytes: 0 });
  await ReclaimingPartitionedJournalStore.open(store, { east: store }, () => 'east', { chunks: null });
  // @ts-expect-error Chunk counts use numeric bounds.
  await ReclaimingPartitionedJournalStore.open(store, { east: store }, () => 'east', { chunks: { maxChunks: '1024' } });
  // @ts-expect-error A synchronous store does not advertise Promise-only methods.
  const asyncOnly: AtomicJournalStore = store;
  // @ts-expect-error Table namespace admission is numeric.
  PhysicalLsmJournalStore.open(wal, namespace, { maxNamespaceEntries: '256' });
  // @ts-expect-error Use the validating factory.
  new PhysicalLsmJournalStore();
  // @ts-expect-error Every guard preserves the complete decimal generation.
  store.compareDelete('a', 1);
  store.flush(); store.compact();
  return { changed, engine, partitioned, reclaiming, asyncOnly };
}
void physicalLsmTypes;
