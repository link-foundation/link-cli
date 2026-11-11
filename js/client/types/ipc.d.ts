import type { LinksClient } from './index.js';
import type { SocketLike, TcpAddress, TcpOptions } from './tcp.js';
import { TcpTransport } from './tcp.js';

/**
 * A `node:stream` Duplex of bytes with half-close: `end()` ends the outgoing
 * direction only, and `'end'` is the peer ending its side.
 */
export interface IpcStream extends SocketLike {
  readonly destroyed: boolean;
  pipe<T>(destination: T, options?: { end?: boolean }): T;
}

/** The built-in endpoint kinds of docs/protocol/ipc-transports-v1.md. */
export type IpcKind =
  | 'tcp'
  | 'udp'
  | 'unix'
  | 'pipe'
  | 'fifo'
  | 'stdio'
  | 'exec'
  | 'shm'
  | 'mmap'
  | 'file'
  | 'signal'
  | 'mq'
  | 'sysvmq'
  | 'mailslot'
  | 'copydata'
  | 'alpc';

export const KINDS: readonly IpcKind[];

/** An IPC failure with a Node-style `code` (`ECONNREFUSED`, `ETIMEDOUT`, `ENOTSUP`, ...). */
export class IpcError extends Error {
  constructor(message: string, code?: string, options?: { cause?: unknown });
  readonly code: string;
}

/** A parsed `[kind:]address[?name=value&...]` endpoint. */
export class Endpoint {
  /** Lower case; a built-in kind or a registered one. */
  readonly kind: IpcKind | (string & {});
  readonly address: string;
  /** The options, sorted by name. */
  readonly options: Readonly<Record<string, string>>;
  option(name: string): string | undefined;
  /** A whole-number option within `min..=max`, or `fallback`; throws when out of range. */
  number(name: string, fallback: number, min: number, max: number): number;
  /** A duration option in milliseconds. */
  millis(name: string, fallback: number): number;
  /** The connect timeout in milliseconds (`timeout`, 10 000 by default). */
  timeout(): number;
  /** The canonical text, as the Rust reference prints it. */
  toString(): string;
}

/** Parses an endpoint; text without a known kind prefix is a TCP `host:port`. */
export function parseEndpoint(endpoint: string | Endpoint): Endpoint;

/** Options for connecting a stream. */
export interface ConnectStreamOptions {
  signal?: AbortSignal;
  /** Absolute: a Date or epoch milliseconds. */
  deadline?: Date | number;
  /** Relative milliseconds for the connect. */
  timeout?: number;
  /** The bridge command: default the environment variable `CLINK_BIN`, else `clink`. */
  clink?: string;
}

/**
 * Connects a custom kind: resolves to a duplex byte stream with half-close
 * (`end()` ends the outgoing direction only).
 */
export type IpcConnect = (
  endpoint: Endpoint,
  options: ConnectStreamOptions & { signal: AbortSignal },
) => IpcStream | Promise<IpcStream>;

/** Adds a custom endpoint kind; built-in kinds cannot be replaced. */
export function registerTransport(kind: string, connect: IpcConnect): void;

/**
 * Whether the kind is connected to without the `clink` bridge on `platform`
 * (default: this one; custom kinds count).
 */
export function supportsNatively(kind: string, platform?: string): boolean;
/**
 * Whether the kind can be connected to on `platform` and `arch` (default: this
 * machine), natively or through the bridge.
 */
export function supports(kind: string, platform?: string, arch?: string): boolean;
/** The kinds the `clink` bridge connects to on `platform` and `arch` (default: this machine). */
export function bridgeableKinds(platform?: string, arch?: string): Set<IpcKind>;
/** The function a kind is connected to with on `platform` without the bridge, if any. */
export function nativeConnect(kind: string, platform?: string): IpcConnect | undefined;

/** Turns the `[ipc]` log on standard error on or off (it starts on with `CLINK_IPC_TRACE=1`). */
export function setTrace(on: boolean): void;
export function traceEnabled(): boolean;

/** Connects to an endpoint and resolves to a duplex byte stream with half-close. */
export function connectStream(
  endpoint: string | Endpoint,
  options?: ConnectStreamOptions,
): Promise<IpcStream>;

export interface IpcOptions extends TcpOptions {
  /** The bridge command: default the environment variable `CLINK_BIN`, else `clink`. */
  clink?: string;
}

/** The links protocol over an IPC stream; capabilities name the endpoint kind. */
export class IpcTransport extends TcpTransport {
  static connect(endpoint: string | Endpoint | TcpAddress, options?: IpcOptions): Promise<IpcTransport>;
  /** Wraps a connected stream; `owned: false` leaves it open on close. */
  constructor(
    stream: SocketLike,
    options?: TcpOptions & { owned?: boolean; kind?: string; endpoint?: string },
  );
  /** The endpoint kind (`ipc` for a stream handed in without one). */
  readonly kind: string;
  /** The canonical endpoint text, when connected through `connect`. */
  readonly endpoint: string | undefined;
}

/** Connects a LinksClient to `clink --serve ENDPOINT` over any IPC endpoint. */
export function connectIpc(endpoint: string | Endpoint | TcpAddress, options?: IpcOptions): Promise<LinksClient>;
