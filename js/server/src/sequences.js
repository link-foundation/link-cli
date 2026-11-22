// The Unicode sequence converters of rust/src/sequences/: a string is a
// balanced tree of pairs over its UTF-16 code units, each code unit a symbol
// link `(external(code unit) UnicodeSymbol)`, read back by walking the tree
// left to right. The Rust converter objects, matchers, stack and cache only
// hold the two type addresses, so they are functions over those here.
import { externalReference, externalReferenceValue } from './hybrid-reference.js';

/** `BalancedVariantConverter::convert`: pairs neighbours layer by layer down to one link. */
export function balancedVariant(links, elements) {
  if (elements.length === 0) return 0;
  if (elements.length === 1) return elements[0];
  let layer = elements;
  while (layer.length > 2) {
    const next = [];
    for (let i = 0; i + 1 < layer.length; i += 2)
      next.push(links.getOrCreate(layer[i], layer[i + 1]));
    if (layer.length % 2 === 1) next.push(layer[layer.length - 1]);
    layer = next;
  }
  return links.getOrCreate(layer[0], layer[1]);
}

/**
 * `StringToUnicodeSequenceConverter::convert`: the sequence link of a
 * non-empty string, `(tree UnicodeSequence)`.
 */
export function stringToUnicodeSequence(links, content, symbolType, sequenceType) {
  const symbols = [];
  for (let i = 0; i < content.length; i++) {
    symbols.push(links.getOrCreate(externalReference(content.charCodeAt(i)), symbolType));
  }
  return links.getOrCreate(balancedVariant(links, symbols), sequenceType);
}

/**
 * `RightSequenceWalker::try_walk`: the symbol links under `sequence`, left to
 * right, rejecting missing links and cycles.
 */
export function walkSequence(links, sequence, symbolType) {
  const output = [];
  const stack = [[sequence, false]];
  const path = new Set();
  while (stack.length > 0) {
    const [element, exit] = stack.pop();
    if (exit) {
      path.delete(element);
      continue;
    }
    const found = links.get(element);
    if (found === null) throw new Error(`Sequence link ${element} does not exist.`);
    if (found.target === symbolType) {
      output.push(element);
      continue;
    }
    if (path.has(element)) throw new Error(`Unicode sequence cycle at link ${element}.`);
    path.add(element);
    stack.push([element, true], [found.target, false], [found.source, false]);
  }
  return output;
}

// `UnicodeSymbolToCharConverter::convert` of a link the walker returned, so
// it is a symbol: the raw number of its source, which has to fit a code unit.
function symbolCodeUnit(links, symbol) {
  const { source } = links.get(symbol);
  const value = externalReferenceValue(source) ?? source;
  if (value > 0xffff) throw new Error('out of range integral type conversion attempted');
  return value;
}

// Rust `String::from_utf16`, which rejects unpaired surrogates.
function fromUtf16(codeUnits) {
  for (let i = 0; i < codeUnits.length; i++) {
    const unit = codeUnits[i];
    if (
      unit >= 0xd800 &&
      unit <= 0xdbff &&
      codeUnits[i + 1] >= 0xdc00 &&
      codeUnits[i + 1] <= 0xdfff
    ) {
      i++;
    } else if (unit >= 0xd800 && unit <= 0xdfff) {
      throw new Error('invalid utf-16: lone surrogate found');
    }
  }
  return codeUnits.map((unit) => String.fromCharCode(unit)).join('');
}

/** `UnicodeSequenceToStringConverter::convert`. */
export function unicodeSequenceToString(links, sequence, symbolType, sequenceType) {
  if (sequence === sequenceType) return '';
  const found = links.get(sequence);
  if (found === null || found.target !== sequenceType) {
    throw new Error(`Specified link ${sequence} is not a Unicode sequence.`);
  }
  return fromUtf16(
    walkSequence(links, found.source, symbolType).map((symbol) => symbolCodeUnit(links, symbol)),
  );
}
