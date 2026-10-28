/** Canonical decimal u64 fields are strings to preserve their full range. */
export interface CausalDot { author: string; counter: string }
export interface CausalPayload { source: string; target: string; name: string | null }
export interface CausalValue {
  address: string;
  dot: CausalDot;
  clock: CausalDot[];
  payload: CausalPayload | null;
}
/** Selected semantic operations only. No I/O, owner fencing or replication. */
export declare class CausalVersion {
  static decode(wire: string): CausalValue;
  static encode(value: CausalValue): string;
  static merge(left: readonly CausalValue[], right: readonly CausalValue[]): CausalValue[];
  /** Caller must persist counter and mutation atomically and fence duplicate
   * author owners before publishing the result. Does not retry or perform I/O. */
  static advance(address: string, author: string, lastCounter: string,
    observed: readonly CausalValue[], payload: CausalPayload | null): CausalValue;
}
