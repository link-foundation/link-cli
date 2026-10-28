import type { CausalValue } from './engine-causal-version.js';
import type { CausalSnapshot } from './engine-causal-replica.js';
export interface CausalMerkleOptions { maxVersions?: number; maxBytes?: number }
export interface CausalMerkleNode {
  root: string;
  prefix: string;
  children: string[];
  proof: string[][];
}
export interface CausalMerkleLeaf {
  root: string;
  address: string;
  versions: CausalValue[];
  proof: string[][];
}
export interface CausalMerkleSource {
  readonly root: string;
  node(prefix: string): CausalMerkleNode | Promise<CausalMerkleNode>;
  leaf(address: string): CausalMerkleLeaf | Promise<CausalMerkleLeaf>;
}
/** Caller-owned immutable visible-content snapshot. Select this module to pay
 * hashing/index costs; bound transport frames before constructing strings. */
export declare class CausalMerkleSnapshot implements CausalMerkleSource {
  private constructor();
  static create(values: readonly CausalValue[], options?: CausalMerkleOptions): Promise<CausalMerkleSnapshot>;
  readonly root: string;
  node(prefix: string): CausalMerkleNode;
  leaf(address: string): CausalMerkleLeaf;
  static verifyNode(expectedRoot: string, response: CausalMerkleNode): Promise<string[]>;
  static verifyLeaf(expectedRoot: string, response: CausalMerkleLeaf): Promise<CausalValue[]>;
  static encodeNode(response: CausalMerkleNode): Promise<string>;
  static decodeNode(expectedRoot: string, wire: string): Promise<CausalMerkleNode>;
  static encodeLeaf(response: CausalMerkleLeaf): Promise<string>;
  static decodeLeaf(expectedRoot: string, wire: string): Promise<CausalMerkleLeaf>;
}
/** Opaque owned frontier, scoped to a source root and the same monotone owner. */
export declare class CausalPullCursor {
  private constructor();
  private readonly brand;
  readonly root: string;
}
export interface CausalPullOptions {
  maxNodeRequests?: number;
  maxLeafRequests?: number;
  maxVersionBytes?: number;
  continuation?: CausalPullCursor | null;
}
export interface CausalPullProgress {
  root: string;
  nodeRequests: number;
  leafRequests: number;
  versionBytes: number;
  changedLeaves: number;
  complete: boolean;
  continuation: CausalPullCursor | null;
}
/** One finite attempt; partial progress can remain on failure. Resume a returned frontier on the same monotone owner; on error start a fresh
 * attempt; independent authors and local historical checks still apply. */
export declare function pullCausal(source: CausalMerkleSource, destination: {
  snapshot(): CausalSnapshot | Promise<CausalSnapshot>;
  receive(versions: readonly CausalValue[]): boolean | Promise<boolean>;
}, options?: CausalPullOptions): Promise<CausalPullProgress>;
