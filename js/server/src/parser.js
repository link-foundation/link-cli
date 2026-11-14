// Parses LiNo text into `LinoLink` trees: `{ id, values }`, where `id` is a
// string or null and `values` an array or null. The port of rust/src/parser.rs
// and rust/src/lino_link.rs over the links-notation package.
import { Parser as NotationParser } from 'links-notation';
import { ParseError } from './errors.js';

export const lino = (id, values = null) => ({ id: id ?? null, values: values ?? null });

export const isEmptyLino = (value) =>
  value.id == null && (value.values == null || value.values.length === 0);
export const hasValues = (value) => value.values != null && value.values.length > 0;
export const valuesCount = (value) => (value.values == null ? 0 : value.values.length);
export const isVariable = (value) => typeof value.id === 'string' && value.id.startsWith('$');
export const isWildcard = (value) => value.id === '*';

function convert(node) {
  const values = node.values ?? [];
  if (values.length === 0) return node.id == null ? lino(null) : lino(String(node.id));
  return lino(node.id == null ? null : String(node.id), values.map(convert));
}

/** Parses `text`; a syntax error becomes a `ParseError`, as `LinkError::ParseError` in Rust. */
export function parseLino(text, { maxDepth } = {}) {
  const parser = new NotationParser(maxDepth === undefined ? {} : { maxDepth });
  let parsed;
  try {
    parsed = parser.parse(text);
  } catch (error) {
    throw new ParseError(error.message, { cause: error });
  }
  return parsed.map(convert);
}
