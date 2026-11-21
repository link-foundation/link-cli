// Triplets in Links Notation: one `(index: type from to)` per line. Port of
// rust/src/triplets/lino.rs.
//
// The export lists every triplet in the order of its address, a part by its
// name when it has one and `0` for an empty from or to. The import reads the
// same lines back: a numeric index is kept, a named index is the triplet of
// that name (created when there is none), and a line without an index is a
// new triplet. Lines may refer to each other in any order, since every
// triplet is reserved before any of them gets its parts.
import { hasValues } from './parser.js';
import { formatIdentifier, parse } from './query-parser.js';
import { patternOf, substitute } from './triplets-query.js';
import { TripletQuery } from './triplets-restriction.js';

/** Every triplet as a line `(index: type from to)`, in the order of the addresses. */
export function tripletsToLino(triplets) {
  const reference = (id) => {
    if (id === 0) return '0';
    const name = triplets.getName(id);
    return name == null ? String(id) : formatIdentifier(`"${name}"`);
  };
  return triplets
    .eachTriplet(TripletQuery.any())
    .map(
      ({ index, typeId, from, to }) =>
        `(${reference(index)}: ${reference(typeId)} ${reference(from)} ${reference(to)})\n`,
    )
    .join('');
}

/** Writes the triplets of `text` and returns their indices, one per line. */
export function importTripletsLino(triplets, text) {
  const patterns = parse(text).map((line) => {
    // A document of one line is that line wrapped once more.
    const values = line.values ?? [];
    const only = line.id == null && values.length === 1 && hasValues(values[0]);
    const pattern = patternOf(only ? values[0] : line);
    for (const reference of [pattern.index, ...pattern.parts]) {
      if (reference != null && (reference.kind === 'Any' || reference.kind === 'Variable')) {
        throw new Error('A triplet in Links Notation has no variables or wildcards.');
      }
    }
    return pattern;
  });
  return substitute(triplets, patterns, new Map(), []);
}
