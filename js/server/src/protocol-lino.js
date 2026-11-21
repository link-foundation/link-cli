// The LiNo document model of the protocols: ports links-notation 0.31.6
// `binary::{format_document, format_link, format_reference, parse_document,
// canonical}` (src/binary/format.rs, src/reference_literal.rs) and, from
// rust/src/protocol/server.rs and server_query.rs, `error_document`,
// `error_message`, `query_document`, `admit_document` and
// `parse_document_with_limits`.
//
// A reference (`LiNo::Ref`) is a JavaScript string; a link (`LiNo::Link`) is
// `{ id: string | null, values: [...] }`. Byte lengths are UTF-8 lengths, as
// Rust's `str::len`.
import { LinkError } from './errors.js';
import { ProtocolError } from './protocol-errors.js';
import { parseLinoToLinks } from './protocol-lino-parser.js';

/** `LiNo::Link { id, values }`. */
export function linoLink(id, values) {
  return { id, values };
}

/** Whether `node` is a `LiNo::Ref`. */
export const isRef = (node) => typeof node === 'string';

/** The UTF-8 length of `text`, Rust's `str::len`. */
export const byteLength = (text) => Buffer.byteLength(text, 'utf8');

const WHITE_SPACE_EDGES = /^\p{White_Space}+|\p{White_Space}+$/gu;

/** Rust's `str::trim`: strips Unicode `White_Space` from both ends. */
export const rustTrim = (text) => text.replace(WHITE_SPACE_EDGES, '');

/** Whether two nodes (or documents) are equal, as Rust's `PartialEq`. */
export function sameNode(a, b) {
  if (isRef(a) || isRef(b)) return a === b;
  if (Array.isArray(a) || Array.isArray(b)) {
    return (
      Array.isArray(a) &&
      Array.isArray(b) &&
      a.length === b.length &&
      a.every((node, index) => sameNode(node, b[index]))
    );
  }
  return a.id === b.id && sameNode(a.values, b.values);
}

/** `encode_reference_literal`: `~1{hex of the UTF-8 bytes}`. */
export function encodeReferenceLiteral(text) {
  return `~1{${Buffer.from(text, 'utf8').toString('hex')}}`;
}

// `~<digits>{` at the start of a reference makes it a literal.
const LITERAL_PREFIX = /^~[0-9]+\{/;
// `char::is_whitespace`, the byte order mark and the characters the grammar
// reserves.
const NEEDS_QUOTES = /[\p{White_Space}\u{feff}():'"`]/u;
// eslint-disable-next-line no-control-regex
const CONTROL = /[\u0000-\u001f\u007f]/;
const QUOTES = ["'", '"', '`'];

/** `format_reference`: bare when it survives parsing, else quoted or a literal. */
export function formatReference(text) {
  if (text === '' || CONTROL.test(text)) return encodeReferenceLiteral(text);
  if (!LITERAL_PREFIX.test(text) && !text.startsWith('#') && !NEEDS_QUOTES.test(text)) {
    return text;
  }
  const first = String.fromCodePoint(text.codePointAt(0));
  let best = null;
  for (const quote of QUOTES) {
    if (quote === first) continue;
    let longest = 0;
    let current = 0;
    for (const character of text) {
      current = character === quote ? current + 1 : 0;
      longest = Math.max(longest, current);
    }
    const count = (longest + 1) | 1;
    if (best === null || count < best.count) best = { quote, count };
  }
  const delimiter = best.quote.repeat(best.count);
  return `${delimiter}${text}${delimiter}`;
}

function joinValues(values) {
  return values.map(formatLink).join(' ');
}

/** `format_link`: one link as it appears nested inside another link. */
export function formatLink(node) {
  if (isRef(node)) return formatReference(node);
  if (node.id === null) {
    // `(a)` parses back as the reference `a`, so a one-reference link needs
    // a second pair of parentheses.
    if (node.values.length === 1 && isRef(node.values[0])) {
      return `((${formatReference(node.values[0])}))`;
    }
    return `(${joinValues(node.values)})`;
  }
  if (node.values.length === 0) return `(${formatReference(node.id)}:)`;
  return `(${formatReference(node.id)}: ${joinValues(node.values)})`;
}

function formatTopLevel(node) {
  if (!isRef(node) && node.id === null && node.values.length >= 2) return joinValues(node.values);
  return formatLink(node);
}

/** `format_document`: canonical LiNo text, one top-level link per line. */
export function formatDocument(document) {
  return document.map(formatTopLevel).join('\n');
}

/** `canonical`: an unnamed group holding exactly one reference is that reference. */
export function canonical(node) {
  if (isRef(node)) return node;
  if (node.id === null && node.values.length === 1 && isRef(node.values[0])) {
    return node.values[0];
  }
  return { id: node.id, values: node.values.map(canonical) };
}

/** `binary::parse_document`: blank text (spaces, tabs, line ends) is the empty document. */
export function parseDocument(text) {
  if (/^[ \t\r\n]*$/.test(text)) return [];
  let links;
  try {
    links = parseLinoToLinks(text);
  } catch (error) {
    throw ProtocolError.invalidLino(error.message);
  }
  return links.map(canonical);
}

/** `admit_document`: checks the size of a document against `QueryLimits`. */
export function admitDocument(document, limits) {
  if (document.length > limits.maxNodes) throw new LinkError('protocol node limit exceeded');
  const pending = document.map((node) => [node, 0]);
  let admitted = document.length;
  let bytes = 0;
  while (pending.length > 0) {
    const [node, depth] = pending.pop();
    if (depth > limits.maxDepth) throw new LinkError('protocol depth limit exceeded');
    if (isRef(node)) {
      bytes += byteLength(node);
    } else {
      bytes += node.id === null ? 0 : byteLength(node.id);
      admitted += node.values.length;
      if (admitted > limits.maxNodes) throw new LinkError('protocol node limit exceeded');
      for (const value of node.values) pending.push([value, depth + 1]);
    }
    if (bytes > limits.maxBytes) throw new LinkError('protocol byte limit exceeded');
  }
}

/** `parse_document_with_limits`: bounded transport text before its document is built. */
export function parseDocumentWithLimits(text, limits) {
  if (byteLength(text) > limits.maxBytes) {
    throw ProtocolError.limitExceeded('protocol byte limit exceeded');
  }
  if (rustTrim(text) === '') return [];
  let document;
  try {
    document = parseLinoToLinks(text, { maxDepth: limits.maxDepth });
  } catch (error) {
    throw ProtocolError.invalidLino(error.message);
  }
  try {
    admitDocument(document, limits);
  } catch (error) {
    throw ProtocolError.limitExceeded(error.message);
  }
  return document.map(canonical);
}

/** `error_document`: the reply to a failed request, `(error: 'message')`. */
export function errorDocument(message) {
  return [{ id: 'error', values: [message] }];
}

/** `error_message`: the message of an `(error: 'message')` reply, or null. */
export function errorMessage(document) {
  if (document.length !== 1) return null;
  const [node] = document;
  if (isRef(node) || node.id !== 'error') return null;
  return node.values.length === 1 && isRef(node.values[0]) ? node.values[0] : null;
}

/** `query_document`: carries the exact original query text, `(query: text)`. */
export function queryDocument(text) {
  return [{ id: 'query', values: [text] }];
}
