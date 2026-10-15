import type { Capabilities, LinksClient, RequestOptions, Scope, Transport } from './index.js';
import type { TcpOptions, TcpTransport } from './tcp.js';

export interface ServerOptions extends RequestOptions {
  /** The clink executable (or e.g. `dotnet` with `args: ['clink.dll']`); never run through a shell. */
  command: string;
  args?: string[];
  /** The links database file; spaces need no quoting. */
  db: string;
  host?: string;
  /** 0 (the default) picks a free port. */
  port?: number;
  env?: Record<string, string | undefined>;
  cwd?: string;
}

export class ServerProcess {
  readonly address: string;
  readonly pid: number | undefined;
  readonly stderr: string;
  readonly exited: Promise<{ code: number | null; signal: string | null }>;
  close(options?: { timeout?: number }): Promise<{ code: number | null; signal: string | null }>;
  [Symbol.asyncDispose](): Promise<void>;
}
export function startServer(options: ServerOptions): Promise<ServerProcess>;

export class ProcessTransport implements Transport {
  constructor(transport: TcpTransport, server: ServerProcess);
  readonly transport: TcpTransport;
  readonly server: ServerProcess;
  readonly capabilities: Capabilities;
  send(text: string, scope?: Scope): Promise<string>;
  close(): Promise<void>;
}
export function connectProcess(options: ServerOptions & TcpOptions): Promise<LinksClient>;
