// Ports the text parser of the Rust links-notation 0.31.6 crate, the one the
// Rust server reads LiNo requests with: `parse_lino_to_links` and
// `parse_lino_to_links_with_config` of src/lib.rs (`read_links`, the
// flattening of indented and nested links, and the `ParseError` messages),
// with src/parser_config.rs (comments on, at most 64 levels of nesting),
// src/parser.rs (protocol-lino-parser-grammar.js), src/quotes.rs,
// src/reference_literal.rs and src/comments.rs (protocol-lino-parser-text.js)
// and the errors of src/lib.rs (protocol-lino-parser-errors.js).
//
// A reference is a string, and a link is `{ id, values }` with `id` a string or
// null. Parsed links may share a node, as a parent repeated before each of its
// indented children; treat them as immutable.
import { LinoParseError, parseError } from './protocol-lino-parser-errors.js';
import { LinoGrammar } from './protocol-lino-parser-grammar.js';
import { DelimitedReferences, encodeUtf8, stripComments } from './protocol-lino-parser-text.js';

export { LinoParseError };

/** `DEFAULT_MAX_DEPTH`: how deep groups and indentation nest unless told otherwise. */
export const DEFAULT_MAX_DEPTH = 64;

/**
 * `parse_lino_to_links_with_config`: the links of a LiNo document, flattened
 * as the Rust crate does. Throws a `LinoParseError` when it does not parse.
 *
 * @param {string} text
 * @param {{ maxDepth?: number }} [options]
 */
export function parseLinoToLinks(text, { maxDepth = DEFAULT_MAX_DEPTH } = {}) {
  if (/^[ \t\n\r]*$/.test(text)) return [];
  const source = encodeUtf8(text);
  const references = new DelimitedReferences(source);
  const grammar = new LinoGrammar(stripComments(source, references), references, maxDepth);
  const parsed = grammar.document();
  if (parsed.err) throw parseError(source, grammar.failure(parsed));
  return flattenLinks(parsed.value);
}

/** `flatten_links`: indented children become links of their own. */
function flattenLinks(links) {
  const result = [];
  for (const link of links) flattenLink(link, null, result);
  return result;
}

/** A reference stands in a pair as the link `(reference)`. */
function wrapped(node) {
  return typeof node === 'string' ? { id: null, values: [node] } : node;
}

/** `flatten_link_recursive`: a child line is paired with its parent. */
function flattenLink(link, parent, result) {
  // An indented ID takes its children as its values.
  if (link.indented && link.children.length > 0) {
    const current = { id: link.id, values: link.children.map(transformIndentedValue) };
    result.push(parent === null ? current : { id: null, values: [wrapped(parent), current] });
    return;
  }
  const current = transformValue(link);
  const combined =
    parent === null ? current : { id: null, values: [wrapped(parent), wrapped(current)] };
  result.push(combined);
  for (const child of link.children) flattenLink(child, combined, result);
}

/**
 * `transform_nested`: a group is flattened like a document; a body of one link
 * is that link, unless it is a lone group, which keeps `((a b))` apart from
 * `(a b)`.
 */
function transformNested(body) {
  const links = flattenLinks(body);
  const wrapsSingleGroup =
    body.length === 1 && body[0].nested !== null && body[0].children.length === 0;
  if (links.length === 1 && !wrapsSingleGroup) return links[0];
  return { id: null, values: links };
}

/** `transform_value`: a link without its indented children. */
function transformValue(link) {
  if (link.nested !== null) return transformNested(link.nested);
  // Every parsed link without values and outside a group is a reference.
  if (link.values.length === 0) return link.id;
  return { id: link.id, values: link.values.map(transformValue) };
}

/** `transform_indented_value`: a child line and its descendants, as one value. */
function transformIndentedValue(link) {
  const children = link.children.map(transformIndentedValue);
  if (children.length > 0 && link.indented) return { id: link.id, values: children };
  const current = transformValue(link);
  if (children.length > 0) {
    if (typeof current !== 'string') {
      return { id: current.id, values: [...current.values, ...children] };
    }
    // Only a group such as `(a:)` reads as a reference here: a line of one
    // reference is an indented ID, taken above. Rust's arm for any other
    // reference, `(reference children...)`, is never reached.
    return { id: current, values: children };
  }
  // A value link of one value is that value.
  if (link.id === null && link.nested === null && link.values.length === 1) {
    return current.values[0];
  }
  return current;
}
