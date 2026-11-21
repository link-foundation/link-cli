// The substitution half of `QueryProcessor`: turning a matched restriction
// plus a substitution pattern into the links to write. Port of
// rust/src/query_processor_substitution.rs.
import { NotFoundError } from './errors.js';
import { ANY, isFullPoint, isPartialPoint } from './link.js';

const isVariable = (identifier) => identifier.startsWith('$');
const isNormalIndex = (value) => value !== 0 && value !== ANY;

const shouldPreserveExistingPart = (pattern, solution) =>
  pattern != null && pattern.isLeaf() && isVariable(pattern.index) && !solution.has(pattern.index);

function resolvedVariablePart(pattern, solution) {
  if (pattern != null && pattern.isLeaf() && isVariable(pattern.index)) {
    return solution.has(pattern.index) ? solution.get(pattern.index) : null;
  }
  return null;
}

const canPreserveExistingPart = (existing, part, visitedIndexes) =>
  isFullPoint(existing) || isPartialPoint(existing) || !visitedIndexes.has(part);

/**
 * Keeps the stored halves of link `index` for substitution parts that are
 * unbound variables (binding them in `solution`), and writes bound variables
 * into `parts` (`{ source, target }`, updated in place).
 */
export function preserveExistingSubstitutionParts(
  storage,
  pattern,
  solution,
  index,
  parts,
  visitedIndexes,
) {
  if (!isNormalIndex(index) || !storage.exists(index)) return;
  if (visitedIndexes.has(index)) return;
  visitedIndexes.add(index);
  const existing = storage.getLink(index);
  if (existing == null) throw new NotFoundError(index);
  for (const part of ['source', 'target']) {
    if (
      shouldPreserveExistingPart(pattern[part], solution) &&
      canPreserveExistingPart(existing, existing[part], visitedIndexes)
    ) {
      parts[part] = existing[part];
      solution.set(pattern[part].index, parts[part]);
    } else {
      const bound = resolvedVariablePart(pattern[part], solution);
      if (bound !== null) parts[part] = bound;
    }
  }
  visitedIndexes.delete(index);
}
