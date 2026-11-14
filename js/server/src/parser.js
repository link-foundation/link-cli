// Parses LiNo text into `LinoLink` trees: `{ id, values }`, where `id` is a
// string or null and `values` an array or null. The port of rust/src/parser.rs
// and rust/src/lino_link.rs over protocol-lino-parser.js, the port of the
// links-notation crate the Rust `clink` parses with, so the same text gives
// the same links and the same syntax errors.
import { ParseError } from './errors.js';
import { parseLinoToLinks } from './protocol-lino-parser.js';

export const lino = (id, values = null) => ({ id: id ?? null, values: values ?? null });

export const isEmptyLino = (value) =>
  value.id == null && (value.values == null || value.values.length === 0);
export const hasValues = (value) => value.values != null && value.values.length > 0;
export const valuesCount = (value) => (value.values == null ? 0 : value.values.length);
export const isVariable = (value) => typeof value.id === 'string' && value.id.startsWith('$');
export const isWildcard = (value) => value.id === '*';

/** `Parser::convert_link`: a reference, or a link with or without values. */
function convert(node) {
  if (typeof node === 'string') return lino(node);
  if (node.values.length === 0) return lino(node.id);
  return lino(node.id, node.values.map(convert));
}

/** Parses `text`; a syntax error becomes a `ParseError`, as `LinkError::ParseError` in Rust. */
export function parseLino(text, { maxDepth } = {}) {
  let parsed;
  try {
    parsed = parseLinoToLinks(text, maxDepth === undefined ? {} : { maxDepth });
  } catch (error) {
    // Rust's `LinkError::ParseError(String)` has no source, so no `cause`
    // (it would print as `Caused by:`).
    throw new ParseError(error.message);
  }
  return parsed.map(convert);
}
