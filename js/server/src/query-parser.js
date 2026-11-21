// Query reference kinds layered over the notation parser's string-only model.
// Port of rust/src/query_parser.rs.
//
// Every name receives an internal quote envelope (including names containing
// quotes), removed only at name lookups and writes. Bare addresses, variables
// and wildcards keep their syntax. This prevents quoted numbers from reaching
// address parsing: `'1'` is the name "1", never the link 1.
import { QueryError } from './errors.js';
import { isU32Text } from './link.js';
import { escapeLinoReference } from './named-type-links.js';
import { encodeReferenceLiteral, quotedReferenceEnd, stripComments } from './notation.js';
import { hasValues, parseLino } from './parser.js';

// Rust `char::is_whitespace`: the Unicode White_Space property.
const WHITESPACE =
  '\\t\\n\\v\\f\\r \\u0085\\u00a0\\u1680\\u2000-\\u200a\\u2028\\u2029\\u202f\\u205f\\u3000';
const HAS_WHITESPACE_OR_CONTROL = new RegExp(`[${WHITESPACE}\\p{Cc}]`, 'u');
const HAS_CONTROL = /\p{Cc}/u;
const EDGE_WHITESPACE = new RegExp(`^[${WHITESPACE}]+|[${WHITESPACE}]+$`, 'gu');
const SEPARATORS = ' \t\r\n(:)';
const QUOTES = '"\'`';

/** Rust `str::trim`: strips White_Space from both ends. */
export const rustTrim = (text) => text.replace(EDGE_WHITESPACE, '');

/** The name inside a query identifier: one surrounding `"…"` envelope removed. */
export function name(identifier) {
  if (identifier.length >= 2 && identifier.startsWith('"') && identifier.endsWith('"')) {
    return identifier.slice(1, -1);
  }
  return identifier;
}

const startsWithMarker = (value) =>
  value.startsWith('$') || value.startsWith('#') || value.startsWith('~');

/**
 * The text an identifier is written back as. A normalised trigger is parsed
 * again when replayed, so names must stay names.
 */
export function formatIdentifier(identifier) {
  const value = name(identifier);
  if (value === identifier) return identifier;
  const safeBareName =
    value !== '' &&
    !isU32Text(value) &&
    value !== '*' &&
    !startsWithMarker(value) &&
    !HAS_WHITESPACE_OR_CONTROL.test(value) &&
    ![...'():"\'`'].some((character) => value.includes(character));
  if (safeBareName) return value;
  if (
    rustTrim(value) === '' ||
    value === '*' ||
    startsWithMarker(value) ||
    value.includes('`') ||
    HAS_CONTROL.test(value) ||
    (value.includes("'") && value.includes('"'))
  ) {
    return encodeReferenceLiteral(value);
  }
  // Trigger identity is stored as text. Keep the established quoting for
  // ordinary names so existing stored triggers can still be removed.
  return escapeLinoReference(value);
}

/** Parses a query with the established (unlimited) admission. */
export function parse(query) {
  return parseFor(query, null);
}

function countNodes(links, limits) {
  const pending = [...links];
  let nodes = 0;
  while (pending.length > 0) {
    const value = pending.pop();
    nodes += 1;
    if (nodes > limits.maxNodes) throw new QueryError('query node limit exceeded');
    if (value.values != null) pending.push(...value.values);
  }
}

/**
 * The reference a quoted or literal token stands for. The notation parser
 * wraps a lone reference in a link; Rust's `single_reference` takes the first
 * value of each link down to the reference.
 */
const singleReference = (node) => (hasValues(node) ? singleReference(node.values[0]) : node.id);

/** A quoted token or `~n{hex}` literal decoded; a bad one is a `ParseError`. */
const parseLiteral = (token) => singleReference(parseLino(token)[0]);

function isLiteralToken(token) {
  const match = /^~([0-9]+)/.exec(token);
  return match !== null && token[match[0].length] === '{';
}

function checkAddress(token, limits) {
  const digits = token.startsWith('+') || token.startsWith('-') ? token.slice(1) : token;
  if (digits === '' || !/^[0-9]+$/.test(digits)) return;
  if (!isU32Text(token) || BigInt(digits) > limits.maxAddress) {
    throw new QueryError(`query address out of range: ${token}`);
  }
}

/**
 * Parses `query` into `LinoLink` trees (`{ id, values }`), every name in its
 * quote envelope. `limits` is a `QueryLimits` or `null`.
 */
export function parseFor(query, limits) {
  const parseText = (text) =>
    limits == null ? parseLino(text) : parseLino(text, { maxDepth: limits.maxDepth });
  // Preserve diagnostics from the original source, before introducing envelopes.
  const original = parseText(query);
  if (limits != null) countNodes(original, limits);
  if (original.length === 0) return original;
  const text = stripComments(query);
  let rewritten = '';
  let position = 0;
  while (position < text.length) {
    const current = text[position];
    if (SEPARATORS.includes(current)) {
      rewritten += current;
      position += 1;
      continue;
    }
    const start = position;
    const quotedEnd = QUOTES.includes(current) ? quotedReferenceEnd(text, start) : null;
    const quoted = quotedEnd !== null;
    if (quoted) {
      position = quotedEnd;
    } else {
      // The notation grammar falls back to a bare word for an unmatched odd
      // quote prefix; that quote is part of the name.
      while (position < text.length && !SEPARATORS.includes(text[position])) position += 1;
    }
    const token = text.slice(start, position);
    const literal = isLiteralToken(token);
    if (limits != null && !quoted && !literal) checkAddress(token, limits);
    const isSyntax =
      !quoted && !literal && (isU32Text(token) || token === '*' || token.startsWith('$'));
    if (isSyntax) {
      rewritten += token;
    } else {
      const value = quoted || literal ? parseLiteral(token) : token;
      rewritten += encodeReferenceLiteral(`"${value}"`);
    }
  }
  return parseText(rewritten);
}
