// The parts of the links notation the CLI needs beyond `Parser`. They come
// from the `links-notation` package, the same 0.31.5 release the Rust crate
// pins (rust/Cargo.toml), so names are quoted and decoded identically.
import {
  decodeReferenceLiteral,
  encodeReferenceLiteral,
  escapeReference,
  stripComments,
} from 'links-notation';

export { decodeReferenceLiteral, encodeReferenceLiteral, stripComments };

const QUOTES = ['"', "'", '`'];

/**
 * Rust `escape_reference` (`reference_literal::format_reference`): how
 * `LiNo::Ref(text).to_string()` writes a reference.
 */
export const formatReference = (text) => escapeReference(text);

function runLength(text, start, quote) {
  let end = start;
  while (end < text.length && text[end] === quote) end++;
  return end - start;
}

function isSubstantiveBody(content) {
  let depth = 0;
  let hasVisible = false;
  for (const character of content) {
    if (character === '(') depth++;
    else if (character === ')') {
      depth--;
      if (depth < 0) return false;
    }
    if (!/[ \t\n\r]/.test(character)) hasVisible = true;
  }
  return hasVisible && depth === 0;
}

/**
 * Reads the delimited reference that starts at `start`, with any number of
 * delimiters: `{ value, length, closed }`, or `null` when there is none. The
 * same reading as `readReference` in links-notation (src/quotes.js), which the
 * package does not export.
 */
export function readReference(document, start) {
  const quote = document[start];
  if (!QUOTES.includes(quote)) return null;
  const count = runLength(document, start, quote);
  let end = null;
  let position = document.indexOf(quote, start + count);
  while (position !== -1) {
    const length = runLength(document, position, quote);
    if (length >= count && Math.floor(length / count) % 2 === 1) {
      end = position + length;
      break;
    }
    position = document.indexOf(quote, position + length);
  }
  if (end === null) return count % 2 === 0 ? { value: '', length: count, closed: false } : null;
  const parts = [];
  let position2 = start + count;
  let run = document.indexOf(quote, position2);
  while (run !== -1 && run < end) {
    const length = runLength(document, run, quote);
    const escaped = Math.floor(length / (2 * count)) * count;
    const closes = run + length === end ? count : 0;
    parts.push(document.slice(position2, run), quote.repeat(length - escaped - closes));
    position2 = run + length;
    run = document.indexOf(quote, position2);
  }
  const value = parts.join('');
  if (count % 2 === 0 && !isSubstantiveBody(value)) {
    return { value: '', length: count, closed: true };
  }
  return { value, length: end - start, closed: true };
}

/** Rust `links_notation::parser::quoted_reference_end`. */
export function quotedReferenceEnd(document, start) {
  const reading = readReference(document, start);
  return reading === null ? null : start + reading.length;
}
