import type { LinoDocument } from './lino.js';

export interface EngineOptions {
  concurrency?: 'optimistic' | 'exclusive';
  isolation?: 'read-committed' | 'snapshot' | 'serializable';
  history?: null | 'all' | number;
  constraints?: ('references' | 'unique-pairs' | 'unique-names')[];
  trace?: boolean;
  requireDurableFlush?: boolean;
}
export interface CommitLog {
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
}
export interface StagedEngineLike {
  begin(): StagedSessionLike;
  snapshot(revision?: bigint): string;
  journal(): string;
  fork(options: string, journal: string): StagedBranchLike;
  forkWithLog(options: string, log: CommitLog): StagedBranchLike;
  resume(options: string, journal: string, base: string): StagedBranchLike;
  resumeWithLog(options: string, log: CommitLog, base: string): StagedBranchLike;
  free(): void;
}
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
}
export class WasmEngine {
  constructor(binding: { StagedEngine: { new (options: string, journal: string): StagedEngineLike; withLog(options: string, log: CommitLog): StagedEngineLike } }, options?: EngineOptions, journal?: string[] | CommitLog);
  begin(): EngineClient;
  fork(options?: EngineOptions, journal?: string[] | CommitLog): WasmBranch;
  resumeBranch(journal: string[] | CommitLog, mergeBaseRecord: string, options?: EngineOptions): WasmBranch;
  snapshot(revision?: AddressInput): { revision: bigint; rows: (Link & { name: string | null })[] };
  journal(): string[];
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

/** A bigint, a safe-integer number, or a string of digits. */
export type AddressInput = bigint | number | string;
/** `'*'` matches any address. */
export const ANY: '*';
export type Restriction =
  | []
  | [index: AddressInput | '*']
  | [index: AddressInput | '*', sourceOrTarget: AddressInput | '*']
  | [index: AddressInput | '*', source: AddressInput | '*', target: AddressInput | '*'];
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
  | 'sharding';
export const GUARANTEES: readonly Guarantee[];
export const MAX_U32_ADDRESS: bigint;

export interface Capabilities {
  readonly backend: 'clink-server' | 'clink-wasm' | string;
  readonly transport: 'tcp' | 'in-process' | string;
  readonly protocol: 'links-text/1' | string;
  readonly operations: readonly string[];
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
  readonly stats?: { requests: number; bytesSent: number; bytesReceived: number };
}

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
  | { op: 'query'; text: string };

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
export interface EtcdJournalStoreOptions {
  prefix?: string;
  fetch?: typeof globalThis.fetch;
  headers?: Record<string, string>;
  timeoutMs?: number;
  maxRequestBytes?: number;
  maxResponseBytes?: number;
}
export class EtcdJournalStore implements AtomicJournalStore {
  constructor(endpoints: string[], options?: EtcdJournalStoreOptions);
  readonly supportsDurableFlush: true;
  read(keys: string[]): Promise<JournalVersion[]>;
  compareExchange(keys: string[], expected: JournalVersion[], updates: (string[] | null)[]): Promise<boolean>;
}
export class ClusterEngine {
  constructor(binding: ConstructorParameters<typeof WasmEngine>[0], store: AtomicJournalStore, options?: EngineOptions);
  begin(keys: string[]): Promise<ClusterTransaction>;
}
export class ClusterTransaction {
  private constructor();
  shard(key: string): LinksClient;
  commit(): Promise<void>;
  rollback(): Promise<void>;
  close(): Promise<void>;
  [Symbol.asyncDispose](): Promise<void>;
}
