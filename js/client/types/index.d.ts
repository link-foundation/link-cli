import type { LinoDocument } from './lino.js';

export interface EngineOptions {
  /** Default full scans, or persistent indexes for only the selected constraints. */
  constraintValidation?: 'scan' | 'indexed';
  /** In-memory immutable state; independent of journal and isolation. */
  stateStructure?: 'flat' | 'shared' | 'sorted-runs';
  /** Logical journal records; independent of the physical delta decorator. */
  revisionEncoding?: 'snapshot' | 'changes';
  concurrency?: 'optimistic' | 'exclusive' | 'pessimistic';
  /** With pessimistic concurrency, how long a request waits for another session's
   * write address before answering busy; a wait closing a cycle is refused at once. */
  lockWaitMs?: number;
  isolation?: 'read-committed' | 'snapshot' | 'serializable';
  /** Serializable point dependencies; scans conservatively validate the revision. */
  serializableValidation?: 'revision' | 'read-set';
  history?: null | 'all' | number;
  constraints?: ('references' | 'unique-pairs' | 'unique-names')[];
  trace?: boolean;
  requireDurableFlush?: boolean;
}
export interface CommitLog {
  /** Optional synchronous atomic replacement; false means identical records. */
  readonly supportsReplacement?: boolean;
  replace?(records: readonly string[]): boolean;
  readonly supportsDurableFlush: boolean;
  read(): string;
  append(record: string): void;
  flush(): void;
}
export class MemoryCommitLog implements CommitLog {
  constructor(records?: string[]);
  readonly supportsDurableFlush: false;
  read(): string;
  append(record: string): void;
  flush(): void;
}
/** Records `revision|unix milliseconds` in a second log for every appended record. */
export class TimedCommitLog implements CommitLog {
  constructor(inner: CommitLog, timeline: CommitLog, options?: { clock?: () => number });
  readonly supportsDurableFlush: boolean;
  readonly supportsReplacement: boolean;
  replace(records: readonly string[]): boolean;
  read(): string;
  append(record: string): void;
  flush(): void;
}
/** The latest revision written by `time`; refuses a time before the first timestamp. */
export function revisionAt(timeline: CommitLog, time: Date | number): bigint;
export class DeltaCommitLog implements CommitLog {
  constructor(inner: CommitLog, options?: { checkpointInterval?: number });
  readonly supportsDurableFlush: boolean;
  readonly supportsReplacement: boolean;
  replace(records: readonly string[]): boolean;
  read(): string;
  append(record: string): void;
  flush(): void;
}
export class ReplicatedCommitLog implements CommitLog {
  constructor(primary: CommitLog, replicas: CommitLog[], options?: { acknowledgements?: number | null; trace?: (message: string) => void });
  readonly supportsDurableFlush: boolean;
  read(): string;
  append(record: string): void;
  flush(): void;
  synchronize(): number;
}
export interface StagedSessionLike extends ClinkLike {
  commit(): bigint;
  rollback(): void;
  savepoint?(): bigint;
  rollbackTo?(savepoint: bigint): void;
  releaseSavepoint?(savepoint: bigint): void;
}
export interface StagedEngineLike {
  begin(): StagedSessionLike;
  group(): StagedCommitGroupLike;
  snapshot(revision?: bigint): string;
  journal(): string;
  exportCheckpoint(revision?: bigint): string;
  backup(destination: CommitLog, revision?: bigint): bigint;
  backupIncremental(destination: CommitLog): bigint;
  compactJournal(): boolean;
  compactJournalAfter(commits: bigint): boolean;
  fork(options: string, journal: string): StagedBranchLike;
  forkWithLog(options: string, log: CommitLog): StagedBranchLike;
  resume(options: string, journal: string, base: string): StagedBranchLike;
  resumeWithLog(options: string, log: CommitLog, base: string): StagedBranchLike;
  free(): void;
}
export interface StagedCommitGroupLike {
  /** Takes ownership of the native session handle. */
  add(session: StagedSessionLike): void;
  commit(): string;
  free(): void;
}
export type GroupCommitOutcome =
  | { status: 'committed'; revision: bigint }
  | { status: 'aborted' | 'indeterminate'; error: string };
export interface StagedBranchLike {
  engine(): StagedEngineLike;
  merge(): bigint;
  mergeBaseRecord(): string;
  free(): void;
}
export class EngineClient extends LinksClient {
  constructor(session: StagedSessionLike, capabilities?: { durableFlush?: boolean; persistent?: boolean });
  commit(): Promise<bigint>;
  rollback(): Promise<void>;
  savepoint(): Promise<bigint>;
  /** Restores staged writes only: reads stay validated and addresses stay retired. */
  rollbackTo(savepoint: bigint): Promise<void>;
  releaseSavepoint(savepoint: bigint): Promise<void>;
}
/** A savepoint covers this shard only, not the containing group. */
export interface ClusterShardClient extends LinksClient {
  savepoint(): Promise<bigint>;
  rollbackTo(savepoint: bigint): Promise<void>;
  releaseSavepoint(savepoint: bigint): Promise<void>;
}
export class WasmEngine {
  constructor(binding: { StagedEngine: { new (options: string, journal: string): StagedEngineLike; withLog(options: string, log: CommitLog): StagedEngineLike } }, options?: EngineOptions, journal?: string[] | CommitLog);
  begin(): EngineClient;
  /** Drains queued requests, consumes all members and shares one flush. */
  commitGroup(sessions: EngineClient[]): Promise<GroupCommitOutcome[]>;
  fork(options?: EngineOptions, journal?: string[] | CommitLog): WasmBranch;
  resumeBranch(journal: string[] | CommitLog, mergeBaseRecord: string, options?: EngineOptions): WasmBranch;
  snapshot(revision?: AddressInput): { revision: bigint; rows: (Link & { name: string | null })[] };
  journal(): string[];
  /** Retained committed snapshots; does not rewrite or publish the live log. */
  exportCheckpoint(revision?: AddressInput): string[];
  /** Copy a consistent cut to an empty log and verify it. Commits continue during
   * the copy. Returns the revision it ends at. Discard the destination on failure. */
  backup(destination: CommitLog, options?: { revision?: AddressInput }): bigint;
  backupIncremental(destination: CommitLog): bigint;
  /** Copy an archived journal to an empty log, stopping at one revision, and verify
   * it. Opening the destination gives the engine at that revision, on a new timeline. */
  static restoreBackup(
    binding: object,
    source: CommitLog,
    destination: CommitLog,
    options?: { revision?: AddressInput },
  ): bigint;
  /** Publish under the commit lock. Unknown outcomes require reopening. */
  compactJournal(): boolean;
  /** Compact once this many revisions have landed since the last compaction. */
  compactJournalAfter(commits: AddressInput): boolean;
  /** Call compactJournalAfter on a timer until stop(), which throws the first failure. */
  compactEvery(schedule: { commits: AddressInput; periodMs: number }): { stop(): void };
  close(): void;
  [Symbol.dispose](): void;
}
export class WasmBranch {
  private constructor();
  readonly engine: WasmEngine;
  merge(): bigint;
  mergeBaseRecord(): string;
  close(): void;
  [Symbol.dispose](): void;
}

export type { LinoDocument, LinoLink, LinoNode, LinoRef } from './lino.js';
export { formatDocument, formatReference, group, named, parseDocument } from './lino.js';

/** What is known about a request an error ended. */
export type Outcome = 'not-sent' | 'failed' | 'unknown';

export class LinksError extends Error {
  constructor(message: string, options?: { outcome?: Outcome; cause?: unknown });
  outcome: Outcome;
}
export class RemoteError extends LinksError {
  remoteMessage: string;
}
export class MalformedMessageError extends LinksError {}
export class ProtocolMismatchError extends LinksError {}
export class LimitExceededError extends LinksError {}
export class AddressRangeError extends LinksError {}
export class ConnectionClosedError extends LinksError {}
export class AbortError extends LinksError {}
export class DeadlineExceededError extends LinksError {}
export class UnsupportedError extends LinksError {}
/** No link has the address a write names; nothing was sent. */
export class NotFoundError extends LinksError {
  constructor(index: unknown);
  readonly index: unknown;
}

/** A bigint, a safe-integer number, or a string of digits. */
export type AddressInput = bigint | number | string;
/** `'*'` matches any address. */
export const ANY: '*';
export type Restriction =
  | []
  | [index: AddressInput | '*']
  | [index: AddressInput | '*', sourceOrTarget: AddressInput | '*']
  | [index: AddressInput | '*', source: AddressInput | '*', target: AddressInput | '*'];
/** Parts of `[index, type, from, to]` left out match anything, as `'*'` does. */
export type TripletRestriction =
  | []
  | [index: AddressInput | '*']
  | [index: AddressInput | '*', type: AddressInput | '*']
  | [index: AddressInput | '*', type: AddressInput | '*', from: AddressInput | '*']
  | [index: AddressInput | '*', type: AddressInput | '*', from: AddressInput | '*', to: AddressInput | '*'];
export function toAddress(value: AddressInput, what?: string): bigint;

export interface Link {
  index: bigint;
  source: bigint;
  target: bigint;
}
/** A link as the backend wrote it: numbers, or names of named links. */
export interface NamedLink {
  index: string;
  source: string;
  target: string;
}
export interface Change<L = Link> {
  before: L | null;
  after: L | null;
}

export type Guarantee =
  | 'atomicTransactions'
  | 'rollback'
  | 'crashConsistency'
  | 'durableFlush'
  | 'pagination'
  | 'persistent'
  | 'rawTcp'
  | 'sharding'
  | 'subscriptions'
  | 'triplets';
export const GUARANTEES: readonly Guarantee[];
export const MAX_U32_ADDRESS: bigint;

export interface Capabilities {
  readonly backend: 'clink-server' | 'clink-wasm' | string;
  readonly transport: 'tcp' | 'in-process' | string;
  readonly protocol: 'links-text/1' | string;
  readonly operations: readonly string[];
  /** Original query-text envelopes are understood by this peer. */
  readonly losslessQueries?: boolean;
  readonly addressBits: number;
  readonly maxAddress: bigint;
  readonly atomicTransactions: boolean;
  readonly rollback: boolean;
  readonly crashConsistency: boolean;
  readonly durableFlush: boolean;
  readonly pagination: boolean;
  readonly persistent: boolean;
  readonly rawTcp: boolean;
  readonly sharding: boolean;
  /** Whether other connections' writes are pushed to subscriptions (a server connection). */
  readonly subscriptions: boolean;
  /** Whether triplets are served (true), are not (false) or it is unknown (null): a server has them with `--triplets`. */
  readonly triplets: boolean | null;
  /** Whether equal links stay apart (true), merge (false) or it is unknown (null). */
  readonly duplicateLinks: boolean | null;
  readonly bulk: 'pipelined-non-atomic' | 'sequential-non-atomic' | string;
  readonly limits?: Readonly<Record<string, number>>;
}
export function tcpServerCapabilities(overrides?: Partial<Capabilities>): Capabilities;
export function wasmCapabilities(overrides?: Partial<Capabilities>): Capabilities;

export interface RequestOptions {
  signal?: AbortSignal;
  /** Absolute: a Date or epoch milliseconds. */
  deadline?: Date | number;
  /** Relative milliseconds; the earlier of `deadline` and `timeout` wins. */
  timeout?: number;
}
export interface Scope {
  signal?: AbortSignal;
  deadline?: number;
}
export function scopeOf(options?: RequestOptions): Scope;

export interface Transport {
  readonly capabilities: Capabilities;
  send(text: string, scope: Scope): Promise<string>;
  close?(): Promise<void>;
  readonly stats?: { requests: number; bytesSent: number; bytesReceived: number; pushes?: number };
  /** Transports that receive pushes (TCP): the text of the next one. */
  receivePush?(scope: Scope): Promise<string>;
  /** Transports that receive pushes (TCP): every push from now on, instead of `receivePush`. */
  onPush?(listener: (text: string) => void): () => void;
  readonly pendingPushes?: number;
}

/** A push of a subscription: the changes of another connection's write it may see. */
export interface Notification {
  subscription: number;
  changes: Change[];
}
/** Whether a document is a push: its first link is `(notify: N)`. */
export function isNotification(document: LinoDocument): boolean;
export function parseNotification(document: LinoDocument): Notification;

/** A triplet of a server in triplet mode (`clink --serve --triplets`). */
export interface Triplet {
  index: bigint;
  type: bigint;
  from: bigint;
  to: bigint;
}
/** A triplet with every reference as the backend wrote it: a number, or a name. */
export interface NamedTriplet {
  index: string;
  type: string;
  from: string;
  to: string;
}
/** A push of a subscription on a server in triplet mode. */
export interface TripletNotification {
  subscription: number;
  changes: Change<Triplet>[];
}
export function parseTripletNotification(document: LinoDocument): TripletNotification;

export type Operation =
  | { op: 'count'; restriction?: Restriction }
  | { op: 'each'; restriction?: Restriction }
  | { op: 'create'; source: AddressInput; target: AddressInput }
  | { op: 'update'; index: AddressInput; source: AddressInput; target: AddressInput }
  | { op: 'delete'; index: AddressInput }
  | { op: 'getName'; index: AddressInput }
  | { op: 'setName'; index: AddressInput; name: string }
  | { op: 'getByName'; name: string }
  | { op: 'removeName'; index: AddressInput }
  | { op: 'query'; text: string }
  | { op: 'countTriplets'; restriction?: TripletRestriction }
  | { op: 'eachTriplets'; restriction?: TripletRestriction }
  | { op: 'createTriplet'; type: AddressInput; from: AddressInput; to: AddressInput }
  | { op: 'updateTriplet'; index: AddressInput; type: AddressInput; from: AddressInput; to: AddressInput }
  | { op: 'deleteTriplet'; index: AddressInput }
  | { op: 'queryTriplets'; text: string };

export type OperationResult =
  | { status: 'ok'; value: unknown }
  | { status: 'failed' | 'unknown' | 'not-sent'; error?: LinksError | Error };

export interface BatchResult {
  /** True only when every operation is `ok`. */
  completed: boolean;
  results: OperationResult[];
  ok: number;
  failed: number;
  unknown: number;
  notSent: number;
  firstError: LinksError | Error | null;
}

/** The contract every client and decorator implements. */
export interface Links {
  readonly capabilities: Capabilities;
  require(...guarantees: Guarantee[]): this;
  requestText(text: string, options?: RequestOptions): Promise<string>;
  request(document: LinoDocument, options?: RequestOptions): Promise<LinoDocument>;
  count(restriction?: Restriction, options?: RequestOptions): Promise<bigint>;
  each(restriction?: Restriction, options?: RequestOptions): Promise<Link[]>;
  /** Null when the backend keeps links unique and merged this one into an equal link. */
  create(source: AddressInput, target: AddressInput, options?: RequestOptions): Promise<Link | null>;
  update(index: AddressInput, source: AddressInput, target: AddressInput, options?: RequestOptions): Promise<Change[]>;
  delete(index: AddressInput, options?: RequestOptions): Promise<Change[]>;
  getName(index: AddressInput, options?: RequestOptions): Promise<string | null>;
  /** Resolves to the backend-specific address that answers for the name. */
  setName(index: AddressInput, name: string, options?: RequestOptions): Promise<bigint>;
  getByName(name: string, options?: RequestOptions): Promise<bigint | null>;
  removeName(index: AddressInput, options?: RequestOptions): Promise<void>;
  query(text: string, options?: RequestOptions): Promise<Change<NamedLink>[]>;
  exportLinks(options?: RequestOptions): Promise<NamedLink[]>;
  batch(operations: Operation[], options?: RequestOptions & { window?: number }): Promise<BatchResult>;
  /** Resolves to the subscription's number; needs `capabilities.subscriptions`. */
  subscribe(restriction?: Restriction, options?: RequestOptions): Promise<number>;
  unsubscribe(subscription: number | bigint, options?: RequestOptions): Promise<void>;
  /** Null when the deadline or timeout passes before a notification arrives. */
  receiveNotification(options?: RequestOptions): Promise<Notification | null>;
  /** Returns the function that removes the listener. */
  onNotification(listener: (notification: Notification) => void): () => void;
  readonly pendingNotifications: number;
  /** Up to four parts `[index, type, from, to]`; this and the calls below need a server with `--triplets`. */
  countTriplets(restriction?: TripletRestriction, options?: RequestOptions): Promise<bigint>;
  eachTriplets(restriction?: TripletRestriction, options?: RequestOptions): Promise<Triplet[]>;
  createTriplet(type: AddressInput, from: AddressInput, to: AddressInput, options?: RequestOptions): Promise<Triplet>;
  /** Creates a triplet that is its own type. */
  createTripletPoint(options?: RequestOptions): Promise<Triplet>;
  updateTriplet(index: AddressInput, type: AddressInput, from: AddressInput, to: AddressInput, options?: RequestOptions): Promise<Change<Triplet>[]>;
  deleteTriplet(index: AddressInput, options?: RequestOptions): Promise<Change<Triplet>[]>;
  queryTriplets(text: string, options?: RequestOptions): Promise<Change<NamedTriplet>[]>;
  exportTriplets(options?: RequestOptions): Promise<NamedTriplet[]>;
  subscribeTriplets(restriction?: TripletRestriction, options?: RequestOptions): Promise<number>;
  /** Null when the deadline or timeout passes before a notification arrives. */
  receiveTripletNotification(options?: RequestOptions): Promise<TripletNotification | null>;
  /** Returns the function that removes the listener. */
  onTripletNotification(listener: (notification: TripletNotification) => void): () => void;
  close(): Promise<void>;
  [Symbol.asyncDispose](): Promise<void>;
}

export interface LinksClient extends Links {}
export class LinksClient implements Links {
  constructor(transport: Transport, options?: { capabilities?: Partial<Capabilities> });
  readonly transport: Transport;
  readonly maxAddress: bigint | null;
}

export interface LinksDecorator extends Links {}
export class LinksDecorator implements Links {
  constructor(links: Links);
  readonly links: Links;
}

/**
 * Many links networks seen as one: the first member that has an address
 * shadows the rest, new links go to the primary (first) member, writes and
 * names to the member that has the link. Raw text, queries, exports,
 * subscriptions and triplets belong to one store: they reject with an
 * UnsupportedError.
 */
export interface UnionLinks extends Links {}
export class UnionLinks implements Links {
  constructor(members: Links[]);
  readonly members: readonly Links[];
  readonly primary: Links;
  /** The position of the member that has the link at `index`, null when none has it. */
  ownerOf(index: AddressInput, options?: RequestOptions): Promise<number | null>;
}

/** How the addresses of a network below a RemapLinks read above it; either method may return a promise. */
export interface AddressMap<Outer = bigint> {
  /** The address below of `outer`, null when it is no address of the network below. */
  toInner(outer: Outer): bigint | null | Promise<bigint | null>;
  /** The address above of `inner`, given one when it has none yet. */
  toOuter(inner: bigint): Outer | Promise<Outer>;
  /** The largest address above, given the largest one below. */
  maxAddress?(innerMaxAddress: bigint): bigint;
}
/** Every address moved by `offset`: the network below reads at `offset + 1` and up. */
export class OffsetAddressMap implements AddressMap<bigint> {
  constructor(offset: AddressInput);
  readonly offset: bigint;
  toInner(outer: AddressInput): bigint | null;
  toOuter(inner: AddressInput): bigint;
  maxAddress(innerMaxAddress: bigint): bigint;
}
/** Addresses given from `first` on, or ids of another kind from `newId()` (UUIDs). */
export class AddressTable<Outer = bigint> implements AddressMap<Outer> {
  constructor(options: Outer extends bigint ? { first: AddressInput } : { newId: () => Outer });
  readonly numeric: boolean;
  toInner(outer: Outer): bigint | null;
  toOuter(inner: AddressInput): Outer;
  add(outer: Outer, inner: AddressInput): void;
  pairs(): [Outer, bigint][];
}
/** A link above a remap whose ids are not addresses (UUID strings). */
export interface RemappedLink<Outer> {
  index: Outer;
  source: Outer | bigint;
  target: Outer | bigint;
}
/**
 * `links` under the addresses of `map`. With a map of BigInt addresses it is a
 * links client like any other (and a member of a union); with ids of another
 * kind its links read under those ids, the null link staying `0n`.
 */
export interface RemapLinks<Outer = bigint> extends Links {}
export class RemapLinks<Outer = bigint> extends LinksDecorator {
  constructor(links: Links, map: AddressMap<Outer>);
  readonly map: AddressMap<Outer>;
  toOuter(inner: AddressInput): Promise<Outer | bigint>;
  toInner(outer: Outer | AddressInput): Promise<bigint | null>;
}

/** What a `CacheLinks` enforces. Each is optional, and the default is neither. */
export interface CacheOptions {
  /** How many links the store may hold, reached by evicting. */
  maxLinks?: number | null;
  /** The milliseconds a link created here has before it comes due. */
  timeToLive?: number | null;
  /** Milliseconds, `Date.now` by default. A step back delays what comes due. */
  clock?: () => number;
}
/**
 * `links` kept as a cache: what comes due goes, and capacity evicts the links used
 * least recently that nothing else uses. Every write has to go through it.
 */
export interface CacheLinks extends Links {}
export class CacheLinks extends LinksDecorator {
  constructor(links: Links, options?: CacheOptions);
  /** Deletes what is due now; resolves to how many links that deleted, cascades included. */
  purgeExpired(options?: RequestOptions): Promise<number>;
  /** Gives `link` a deadline in place of any it had; false when there is no such link. */
  expire(link: AddressInput, milliseconds: number, options?: RequestOptions): Promise<boolean>;
  /** Takes the deadline of `link` away; false when it had none. */
  persist(link: AddressInput, options?: RequestOptions): Promise<boolean>;
  /** The milliseconds `link` has left; null when it has no deadline or is gone. */
  timeToLive(link: AddressInput, options?: RequestOptions): Promise<number | null>;
}

/** The name of the point every scheme of external ids starts from. */
export const EXTERNAL_ID_NAME: 'ExternalId';
/** The ids of one scheme (such as `uuid`) bound to links, stored as links; every method takes the store of the table. */
export class LinksExternalIds {
  constructor(scheme: string);
  readonly scheme: string;
  nameOf(id: string): string;
  find(store: Links, id: string, options?: RequestOptions): Promise<bigint | null>;
  idOf(store: Links, link: AddressInput, options?: RequestOptions): Promise<string | null>;
  bind(store: Links, id: string, link: AddressInput, options?: RequestOptions): Promise<bigint>;
  /** `data` is null when the table is in the data. */
  ensure(store: Links, data: Links | null, id: string, options?: RequestOptions): Promise<bigint>;
  unbind(store: Links, id: string, options?: RequestOptions): Promise<boolean>;
}
/** External ids kept in `table` (a store apart from the network remapped) as the map of a RemapLinks. */
export class ExternalIdMap implements AddressMap<string> {
  constructor(table: Links, scheme: string | LinksExternalIds, options?: { newId?: () => string });
  readonly table: Links;
  readonly ids: LinksExternalIds;
  toInner(outer: string): Promise<bigint | null> | null;
  toOuter(inner: bigint): Promise<string>;
}

/** The `Clink` class of `rust/wasm` (or anything answering LinksOperation text). */
export interface ClinkLike {
  request(text: string): string;
  free?(): void;
}
export interface WasmOptions {
  /** Free the store on close (default false: its creator keeps it). */
  owned?: boolean;
  maxRequestBytes?: number;
  maxReplyBytes?: number;
  capabilities?: Partial<Capabilities>;
}
export class WasmTransport implements Transport {
  constructor(clink: ClinkLike, options?: WasmOptions);
  readonly capabilities: Capabilities;
  readonly stats: { requests: number; bytesSent: number; bytesReceived: number };
  send(text: string, scope?: Scope): Promise<string>;
  control(method: 'commit'): Promise<bigint>;
  control(method: 'rollback'): Promise<void>;
  control(method: 'savepoint'): Promise<bigint>;
  control(method: 'rollbackTo' | 'releaseSavepoint', savepoint: bigint): Promise<void>;
  close(): Promise<void>;
}
export function createWasmClient(clink: ClinkLike, options?: WasmOptions): LinksClient;

export const DEFAULT_MAX_MESSAGE_BYTES: number;
export function encodeMessage(text: string): Uint8Array;
export class MessageReader {
  constructor(options?: { maxBytes?: number });
  push(chunk: Uint8Array): string[];
  readonly midMessage: boolean;
}


export interface JournalVersion { generation: string; entries: string[] }
export interface AtomicJournalStore {
  readonly supportsDurableFlush: boolean;
  read(keys: string[]): Promise<JournalVersion[]>;
  compareExchange(keys: string[], expected: JournalVersion[], updates: (string[] | null)[]): Promise<boolean>;
}
/** Synchronous providers may be selected by consumers that await I/O results. */
export interface SynchronousAtomicJournalStore {
  readonly supportsDurableFlush: boolean;
  read(keys: string[]): JournalVersion[];
  compareExchange(keys: string[], expected: JournalVersion[], updates: (string[] | null)[]): boolean;
}
export type AtomicJournalStoreProvider = AtomicJournalStore | SynchronousAtomicJournalStore;
export interface EtcdJournalStoreOptions {
  prefix?: string;
  fetch?: typeof globalThis.fetch;
  headers?: Record<string, string>;
  timeoutMs?: number;
  maxRequestBytes?: number;
  maxResponseBytes?: number;
}
/** Decoded key bytes + 8 generation bytes + UTF-8 records with one separator each.
 * Container overhead also depends on the accounted record count. */
export interface JournalScanOptions { maxKeys?: number; maxBytes?: number; pageKeys?: number; keysOnly?: boolean }
export interface JournalNamespaceEntry { key: string; version: JournalVersion }
export interface JournalMaintenanceStore extends AtomicJournalStore {
  scanNamespace(options?: JournalScanOptions): Promise<JournalNamespaceEntry[]>;
  compareDelete(key: string, expectedGeneration: string): Promise<boolean>;
}
export interface SynchronousJournalMaintenanceStore extends SynchronousAtomicJournalStore {
  scanNamespace(options?: JournalScanOptions): JournalNamespaceEntry[];
  compareDelete(key: string, expectedGeneration: string): boolean;
}
export type JournalMaintenanceProvider = JournalMaintenanceStore | SynchronousJournalMaintenanceStore;
export class EtcdJournalStore implements JournalMaintenanceStore {
  constructor(endpoints: string[], options?: EtcdJournalStoreOptions);
  scanNamespace(options?: JournalScanOptions): Promise<JournalNamespaceEntry[]>;
  compareDelete(key: string, expectedGeneration: string): Promise<boolean>;
  readonly supportsDurableFlush: true;
  read(keys: string[]): Promise<JournalVersion[]>;
  compareExchange(keys: string[], expected: JournalVersion[], updates: (string[] | null)[]): Promise<boolean>;
}
export class ClusterEngine {
  constructor(binding: ConstructorParameters<typeof WasmEngine>[0], store: AtomicJournalStoreProvider, options?: EngineOptions & { journalCheckpointInterval?: number });
  begin(keys: string[]): Promise<ClusterTransaction>;
}
export class ClusterTransaction {
  private constructor();
  shard(key: string): ClusterShardClient;
  commit(): Promise<void>;
  rollback(): Promise<void>;
  close(): Promise<void>;
  [Symbol.asyncDispose](): Promise<void>;
}
/** Bounded immutable journal chunk write policy, independent of read caching. */
export interface JournalChunkOptions {
  /** Positive record limit, at most 65,536; default 64. */
  maxRecords?: number;
  /** UTF-8 payload bytes per chunk, at most 8,388,608; default 65,536. */
  maxBytes?: number;
  /** Positive manifest bound, at most 4,096; default 1,024. */
  maxChunks?: number;
}
/** Immutable data objects in independent stores; atomic publication in a catalog. */
export class PartitionedJournalStore implements AtomicJournalStore {
  /** Cache budget counts one byte per object/record plus record UTF-8 bytes; zero disables. Catalogs stay fresh. */
  constructor(catalog: AtomicJournalStoreProvider, partitions: Record<string, AtomicJournalStoreProvider>, route: (shard: string) => string, options?: { cacheBytes?: number; chunks?: JournalChunkOptions | null });
  readonly supportsDurableFlush: boolean;
  read(keys: string[]): Promise<JournalVersion[]>;
  compareExchange(keys: string[], expected: JournalVersion[], updates: (string[] | null)[]): Promise<boolean>;
}

/** Dedicated epoch-fenced namespaces; collection is caller-selected and never automatic. */
export interface ReclamationOptions { maxKeys?: number; maxBytes?: number; pageKeys?: number; cacheBytes?: number; chunks?: JournalChunkOptions | null }
export interface ReclamationResult { epoch: string; examined: number; deleted: number }
export class ReclaimingPartitionedJournalStore implements AtomicJournalStore {
  private constructor();
  static open(catalog: JournalMaintenanceProvider, partitions: Record<string, JournalMaintenanceProvider>, route: (shard: string) => string, options?: ReclamationOptions): Promise<ReclaimingPartitionedJournalStore>;
  readonly supportsDurableFlush: boolean;
  /** 1..31 distinct nonreserved shards; returned groups are owned. */
  read(keys: string[]): Promise<JournalVersion[]>;
  compareExchange(keys: string[], expected: JournalVersion[], updates: (string[] | null)[]): Promise<boolean>;
  /** Null means a known competing epoch. Errors can follow acknowledged cuts. */
  collectGarbage(): Promise<ReclamationResult | null>;
}
