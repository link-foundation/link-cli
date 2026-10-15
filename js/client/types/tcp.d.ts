import type { Capabilities, LinksClient, RequestOptions, Scope, Transport } from './index.js';

export interface TcpOptions extends RequestOptions {
  /** Check the peer answers LinksOperation documents (default true). */
  verify?: boolean;
  maxRequestBytes?: number;
  maxReplyBytes?: number;
  /** Requests written ahead of their replies (default 64). */
  maxInFlight?: number;
  capabilities?: Partial<Capabilities>;
}
/** The parts of a `node:net` Socket the transport uses. */
export interface SocketLike {
  on(event: string, listener: (...args: any[]) => void): unknown;
  once(event: string, listener: (...args: any[]) => void): unknown;
  off(event: string, listener: (...args: any[]) => void): unknown;
  write(data: Uint8Array): boolean;
  end(): unknown;
  destroy(): unknown;
}
export type TcpAddress = string | { host?: string; port: number };

export function parseAddress(address: TcpAddress): { host: string; port: number };

export class TcpTransport implements Transport {
  static connect(address: TcpAddress, options?: TcpOptions): Promise<TcpTransport>;
  /** Wraps a connected `node:net` socket; `owned: false` leaves it open on close. */
  constructor(socket: SocketLike, options?: TcpOptions & { owned?: boolean });
  readonly capabilities: Capabilities;
  readonly stats: { requests: number; bytesSent: number; bytesReceived: number };
  readonly closed: boolean;
  send(text: string, scope?: Scope): Promise<string>;
  close(): Promise<void>;
  destroy(): void;
}
export function verifyProtocol(transport: Transport, scope?: Scope): Promise<void>;
export function connectTcp(address: TcpAddress, options?: TcpOptions): Promise<LinksClient>;
