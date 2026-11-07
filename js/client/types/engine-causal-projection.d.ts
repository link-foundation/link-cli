import type { CausalValue } from './engine-causal-version.js';

export type CausalConflictPolicy = 'reject' | 'max-dot';
export interface CausalProjectionOptions {
  /** Complete supplied input, including duplicates and dominated versions. 1..4096. */
  maxVersions?: number;
  /** Sum of canonical ASCII version encodings, 1..8388608 bytes. */
  maxBytes?: number;
}
export interface CausalProjectedRow {
  index: string;
  source: string;
  target: string;
  name: string | null;
}
/** Owned selected snapshot only; no I/O, address allocation, commits or synchronization.
 * Choose from retained siblings including deletions before deciding visibility. */
export declare class CausalProjection {
  constructor(values: readonly CausalValue[], policy: CausalConflictPolicy,
    options?: CausalProjectionOptions);
  versions(): CausalValue[];
  selected(): CausalValue[];
  rows(): CausalProjectedRow[];
}
