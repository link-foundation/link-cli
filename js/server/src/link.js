// A doublet `(index: source target)`: the value every store layer passes
// around. Corresponds to `GenericLink<u32>` in rust/src/link.rs.

/** The wildcard marker a query uses internally for "any value" (`T::MAX`). */
export const ANY = 0xffffffff;

/** The largest address a u32 store can hold. */
export const MAX_ADDRESS = 0xffffffff;

/** A frozen `{ index, source, target }` record. */
export function link(index, source, target) {
  return Object.freeze({ index, source, target });
}

/** The null link `(0: 0 0)` that stands for "no link" in a change. */
export const NULL_LINK = link(0, 0, 0);

export const isNull = (value) => value.index === 0 && value.source === 0 && value.target === 0;

export const isFullPoint = (value) => value.index === value.source && value.source === value.target;

export const isPartialPoint = (value) =>
  value.index === value.source || value.index === value.target;

export const sameLink = (left, right) =>
  left === right ||
  (left != null &&
    right != null &&
    left.index === right.index &&
    left.source === right.source &&
    left.target === right.target);

/** `(index source target)`, the debug form of `GenericLink::format`. */
export const formatPlain = (value) => `(${value.index} ${value.source} ${value.target})`;

/** Whether `text` is what Rust's `str::parse::<u32>()` accepts. */
export function isU32Text(text) {
  if (typeof text !== 'string') return false;
  const digits = text.startsWith('+') ? text.slice(1) : text;
  if (!/^[0-9]+$/.test(digits)) return false;
  return Number(digits) <= MAX_ADDRESS;
}

/** Rust's `str::parse::<u32>()`: the value, or `null` when it does not parse. */
export function parseU32(text) {
  if (!isU32Text(text)) return null;
  return Number(text.startsWith('+') ? text.slice(1) : text);
}
