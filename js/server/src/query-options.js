// The shapes a query is processed with and passes through: the options of one
// query, the admission limits a checked store selects, the decoded-reference
// budget of a checked reply, the `Pattern` a restriction or substitution
// parses into and the `ResolvedLink` a pattern becomes once its variables are
// bound. Port of rust/src/query_options.rs, rust/src/query_types.rs,
// rust/src/query_address.rs and rust/src/query_reply_budget.rs.
import { link } from './link.js';

const U64_MAX = 0xffffffffffffffffn;

/** Options for query processing: `QueryOptions` in Rust. */
export class QueryOptions {
  constructor(query, trace = false) {
    this.query = String(query);
    this.trace = Boolean(trace);
  }
}

/**
 * Admission limits selected by a checked storage adapter. Existing CLI stores
 * retain their established behaviour when no limits are selected, which is
 * what `NamedTypeLinks.queryLimits()` returning `null` means. `maxAddress` is a
 * BigInt (a Rust `u128`); every other limit is a byte or item count.
 */
export class QueryLimits {
  constructor({
    maxAddress,
    maxBytes,
    maxDepth,
    maxNodes,
    maxResults,
    maxBindings,
    maxBindingBytes,
  }) {
    this.maxAddress = BigInt(maxAddress);
    this.maxBytes = maxBytes;
    this.maxDepth = maxDepth;
    this.maxNodes = maxNodes;
    this.maxResults = maxResults;
    this.maxBindings = maxBindings;
    this.maxBindingBytes = maxBindingBytes;
    Object.freeze(this);
  }

  /** Finite limits for a request over a complete u64 causal projection. */
  static causal() {
    return new QueryLimits({
      maxAddress: U64_MAX,
      maxBytes: 1024 * 1024,
      maxDepth: 64,
      maxNodes: 4096,
      maxResults: 4096,
      maxBindings: 16_384,
      maxBindingBytes: 4 * 1024 * 1024,
    });
  }
}

/** The UTF-8 length of `text`, which is what Rust's `str::len` counts. */
export function utf8Length(text) {
  let bytes = 0;
  for (const character of text) {
    const code = character.codePointAt(0);
    bytes += code < 0x80 ? 1 : code < 0x800 ? 2 : code < 0x10000 ? 3 : 4;
  }
  return bytes;
}

/** Shared decoded-reference byte admission for checked named query replies. */
export class QueryReplyBudget {
  constructor(limits) {
    this.remaining = limits == null ? null : limits.maxBytes;
  }

  /** The name of `id`, or its number, charged against the remaining budget. */
  reference(storage, id) {
    const name = storage.getName(id);
    const text = name == null ? String(id) : name;
    if (this.remaining !== null) {
      const length = utf8Length(text);
      if (length > this.remaining) throw new Error('query reply byte limit exceeded');
      this.remaining -= length;
    }
    return text;
  }
}

/** A parsed restriction or substitution: `{ index, source, target }`, the parts `null` for a leaf. */
export class Pattern {
  constructor(index, source = null, target = null) {
    this.index = index;
    this.source = source;
    this.target = target;
  }

  isLeaf() {
    return this.source === null && this.target === null;
  }
}

/** A pattern whose variables are bound: addresses plus the name it gives its link. */
export class ResolvedLink {
  constructor(index, source, target, name = null) {
    this.index = index;
    this.source = source;
    this.target = target;
    this.name = name;
  }

  toLink() {
    return link(this.index, this.source, this.target);
  }
}
