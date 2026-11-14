// Pattern resolution for `QueryProcessor`: turning parsed patterns into
// resolved links, either read-only (for previews and matching) or while
// creating named leaves for substitutions. Port of
// rust/src/query_processor/resolution.rs.
import { InvalidFormatError } from './errors.js';
import { ANY, parseU32 } from './link.js';
import { isNumericOrWildcard, isVariable } from './query-matching.js';
import { ResolvedLink } from './query-options.js';
import { name } from './query-parser.js';
import { preserveExistingSubstitutionParts } from './query-substitution.js';

const defaultIndex = (isSubstitution) => (isSubstitution ? 0 : ANY);

/** The resolution methods, installed on `QueryProcessor.prototype`. */
export const resolutionMethods = {
  resolvePatternsReadonly(storage, patterns, solution, isSubstitution) {
    return patterns.map((pattern) =>
      this.resolvePatternReadonly(storage, pattern, solution, isSubstitution),
    );
  },

  resolvePatternReadonly(storage, pattern, solution, isSubstitution) {
    const fallback = defaultIndex(isSubstitution);
    if (pattern.isLeaf()) {
      const index = this.resolveIdentifierReadonly(storage, pattern.index, solution, fallback);
      return new ResolvedLink(index, ANY, ANY);
    }
    if (pattern.source == null) throw new InvalidFormatError('Invalid source pattern');
    if (pattern.target == null) throw new InvalidFormatError('Invalid target pattern');
    const source = this.resolvePatternReadonly(storage, pattern.source, solution, isSubstitution);
    const target = this.resolvePatternReadonly(storage, pattern.target, solution, isSubstitution);
    const index = this.resolveIdentifierReadonly(storage, pattern.index, solution, fallback);
    return new ResolvedLink(index, source.index, target.index);
  },

  resolveIdentifierReadonly(storage, identifier, solution, defaultValue) {
    return this.resolveIdentifier(storage, identifier, solution, defaultValue, false, null);
  },

  resolvePatterns(storage, patterns, solution, isSubstitution, changes) {
    const workingSolution = new Map(solution);
    const visitedIndexes = new Set();
    return patterns.map((pattern) =>
      this.resolvePattern(
        storage,
        pattern,
        workingSolution,
        isSubstitution,
        visitedIndexes,
        changes,
      ),
    );
  },

  resolvePattern(storage, pattern, solution, isSubstitution, visitedIndexes, changes) {
    const fallback = defaultIndex(isSubstitution);
    if (pattern.isLeaf()) {
      const index = this.resolveIdentifier(
        storage,
        pattern.index,
        solution,
        fallback,
        isSubstitution,
        changes,
      );
      return new ResolvedLink(index, ANY, ANY);
    }
    const parts = { source: 0, target: 0 };
    parts.source = this.resolvePattern(
      storage,
      pattern.source,
      solution,
      isSubstitution,
      visitedIndexes,
      changes,
    ).index;
    parts.target = this.resolvePattern(
      storage,
      pattern.target,
      solution,
      isSubstitution,
      visitedIndexes,
      changes,
    ).index;
    let index = this.resolveIdentifier(storage, pattern.index, solution, fallback, false, changes);
    let linkName = null;
    if (
      isSubstitution &&
      pattern.index !== '' &&
      !isNumericOrWildcard(pattern.index) &&
      !isVariable(pattern.index)
    ) {
      linkName = pattern.index;
      if (index === 0) index = storage.search(parts.source, parts.target) ?? index;
    }
    if (isSubstitution) {
      preserveExistingSubstitutionParts(storage, pattern, solution, index, parts, visitedIndexes);
    }
    return new ResolvedLink(index, parts.source, parts.target, linkName);
  },

  /**
   * Resolves one identifier: `defaultValue` when it is empty or an unbound
   * variable, `ANY` for `*`, a bound variable's value, an address, or the
   * link of that name (created as a point link when `createNamedLeaf`).
   */
  resolveIdentifier(storage, identifier, solution, defaultValue, createNamedLeaf, changes) {
    if (identifier === '') return defaultValue;
    if (identifier === '*') return ANY;
    if (solution.has(identifier)) return solution.get(identifier);
    if (isVariable(identifier)) return defaultValue;
    const parsed = parseU32(identifier);
    if (parsed !== null) return parsed;
    if (createNamedLeaf) return this.ensureNamedPointLink(storage, identifier, changes);
    return storage.getByName(name(identifier)) ?? defaultValue;
  },
};
