// Pattern matching for `QueryProcessor`: the read-only helpers that decide
// whether a stored link satisfies a pattern, plus the admission checks every
// match is charged against. Port of rust/src/query_processor/matching.rs and
// the matching helpers of rust/src/query_processor.rs.
//
// A solution is a `Map` from variable name to address; only a missing entry
// means a variable is unbound (null is a concrete stored reference).
import { QueryError } from './errors.js';
import { ANY, parseU32 } from './link.js';
import { name } from './query-parser.js';
import { utf8Length } from './query-options.js';

export const isAny = (value) => value === ANY;
export const isVariable = (identifier) => identifier.startsWith('$');
export const isNormalIndex = (value) => value !== 0 && !isAny(value);
export const isNumericOrWildcard = (identifier) =>
  identifier === '*' || parseU32(identifier) !== null;

export function assignVariable(id, value, assignments) {
  if (isVariable(id)) assignments.set(id, value);
}

/** A part the query left open keeps the stored value: `ANY` is not a reference. */
export const resolveUnspecified = (value, existing) => (isAny(value) ? existing : value);

// Owned key bytes plus a conservative entry/table allowance (64 + size of u32).
export const bindingCost = (key) => utf8Length(key) + 68;

export function solutionCost(solution) {
  let bytes = 0;
  for (const key of solution.keys()) bytes += bindingCost(key);
  return [solution.size, bytes];
}

function extendedSolutionCost(solution, key) {
  const [count, size] = solutionCost(solution);
  if (isVariable(key) && !solution.has(key)) return [count + 1, size + bindingCost(key)];
  return [count, size];
}

export function checkResultCount(storage, count) {
  const limits = storage.queryLimits();
  if (limits != null && count > limits.maxResults) {
    throw new QueryError('query result limit exceeded');
  }
}

export function checkSolutionCapacity(storage, results, bindings, bytes) {
  checkResultCount(storage, results);
  const limits = storage.queryLimits();
  if (limits != null && (bindings > limits.maxBindings || bytes > limits.maxBindingBytes)) {
    throw new QueryError('query binding memory limit exceeded');
  }
}

export function solutionsAreCompatible(existing, assignments) {
  for (const [key, value] of assignments) {
    if (existing.has(key) && existing.get(key) !== value) return false;
  }
  return true;
}

/** The stored links a resolved definition describes, `0` and `ANY` matching any address. */
export function linksMatchingDefinition(storage, definition) {
  return storage
    .allLinks()
    .filter(
      (link) =>
        (definition.index === 0 || isAny(definition.index) || link.index === definition.index) &&
        (isAny(definition.source) || link.source === definition.source) &&
        (isAny(definition.target) || link.target === definition.target),
    );
}

/**
 * Looks a doublet up with unspecified halves treated as wildcards, the way
 * C#'s `SearchOrDefault` does. `storage.search` matches literally on purpose
 * (it backs uniqueness resolution), so the wildcard pass belongs here.
 */
export function searchUnspecified(storage, source, target) {
  if (!isAny(source) && !isAny(target)) return storage.search(source, target);
  let found = null;
  for (const link of storage.allLinks()) {
    if (
      (isAny(source) || link.source === source) &&
      (isAny(target) || link.target === target) &&
      (found === null || link.index < found)
    ) {
      found = link.index;
    }
  }
  return found;
}

/** The matching methods, installed on `QueryProcessor.prototype`. */
export const matchingMethods = {
  matchPattern(storage, pattern, currentSolution) {
    const resolvedIndex = this.resolveMatchId(storage, pattern.index, currentSolution);
    if (pattern.isLeaf()) {
      const matches = [];
      let bindings = 0;
      let bytes = 0;
      const variable = isVariable(pattern.index);
      const count = variable ? 1 : 0;
      const size = variable ? bindingCost(pattern.index) : 0;
      for (const link of storage.allLinks()) {
        if (!isAny(resolvedIndex) && link.index !== resolvedIndex) continue;
        checkSolutionCapacity(storage, matches.length + 1, bindings + count, bytes + size);
        const assignments = new Map();
        assignVariable(pattern.index, link.index, assignments);
        matches.push(assignments);
        bindings += count;
        bytes += size;
      }
      return matches;
    }
    if (
      !isVariable(pattern.index) &&
      isNormalIndex(resolvedIndex) &&
      storage.exists(resolvedIndex)
    ) {
      const link = storage.getLink(resolvedIndex);
      return this.matchLinkAgainstPattern(storage, pattern, link, currentSolution);
    }
    const results = [];
    let bindings = 0;
    let bytes = 0;
    for (const link of storage.allLinks()) {
      const matched = this.matchLinkAgainstPattern(storage, pattern, link, currentSolution);
      for (const solution of matched) {
        const [count, size] = solutionCost(solution);
        bindings += count;
        bytes += size;
      }
      checkSolutionCapacity(storage, results.length + matched.length, bindings, bytes);
      results.push(...matched);
    }
    return results;
  },

  matchLinkAgainstPattern(storage, pattern, link, currentSolution) {
    if (!this.checkIdMatch(storage, pattern.index, link.index, currentSolution)) return [];
    const results = [];
    let bindings = 0;
    let bytes = 0;
    const sourceMatches = this.recursiveMatchSubpattern(
      storage,
      pattern.source,
      link.source,
      currentSolution,
    );
    for (const sourceSolution of sourceMatches) {
      const targetMatches = this.recursiveMatchSubpattern(
        storage,
        pattern.target,
        link.target,
        sourceSolution,
      );
      for (const targetSolution of targetMatches) {
        const [count, size] = extendedSolutionCost(targetSolution, pattern.index);
        checkSolutionCapacity(storage, results.length + 1, bindings + count, bytes + size);
        assignVariable(pattern.index, link.index, targetSolution);
        bindings += count;
        bytes += size;
        results.push(targetSolution);
      }
    }
    return results;
  },

  recursiveMatchSubpattern(storage, pattern, linkId, currentSolution) {
    if (pattern == null) {
      const [count, size] = solutionCost(currentSolution);
      checkSolutionCapacity(storage, 1, count, size);
      return [new Map(currentSolution)];
    }
    if (pattern.isLeaf()) {
      if (!this.checkIdMatch(storage, pattern.index, linkId, currentSolution)) return [];
      const [count, size] = extendedSolutionCost(currentSolution, pattern.index);
      checkSolutionCapacity(storage, 1, count, size);
      const solution = new Map(currentSolution);
      assignVariable(pattern.index, linkId, solution);
      return [solution];
    }
    const link = storage.getLink(linkId);
    if (link == null) return [];
    return this.matchLinkAgainstPattern(storage, pattern, link, currentSolution);
  },

  checkIdMatch(storage, patternId, candidateId, currentSolution) {
    if (patternId === '' || patternId === '*') return true;
    if (isVariable(patternId)) {
      return !currentSolution.has(patternId) || currentSolution.get(patternId) === candidateId;
    }
    const parsed = parseU32(patternId);
    if (parsed !== null) return parsed === candidateId;
    const named = storage.getByName(name(patternId));
    return named != null && named === candidateId;
  },

  resolveMatchId(storage, identifier, currentSolution) {
    if (identifier === '' || identifier === '*') return ANY;
    if (currentSolution.has(identifier)) return currentSolution.get(identifier);
    if (isVariable(identifier)) return ANY;
    const parsed = parseU32(identifier);
    if (parsed !== null) return parsed;
    return storage.getByName(name(identifier)) ?? 0;
  },

  matchedLinks(storage, pattern, solution) {
    if (pattern.isLeaf()) {
      const resolvedIndex = this.resolveMatchId(storage, pattern.index, solution);
      return storage
        .allLinks()
        .filter((link) => isAny(resolvedIndex) || link.index === resolvedIndex);
    }
    const links = [];
    for (const matched of this.matchPattern(storage, pattern, solution)) {
      const definition = this.resolvePatternReadonly(storage, pattern, matched, false);
      links.push(...linksMatchingDefinition(storage, definition));
    }
    return links;
  },

  solutionIsNoOperation(storage, solution, restrictions, substitutions) {
    const resolve = (patterns, isSubstitution) =>
      this.resolvePatternsReadonly(storage, patterns, solution, isSubstitution)
        .map((definition) => definition.toLink())
        .sort((left, right) => left.index - right.index);
    const restrictionLinks = resolve(restrictions, false);
    const substitutionLinks = resolve(substitutions, true);
    return (
      restrictionLinks.length === substitutionLinks.length &&
      restrictionLinks.every(
        (link, position) =>
          link.index === substitutionLinks[position].index &&
          link.source === substitutionLinks[position].source &&
          link.target === substitutionLinks[position].target,
      )
    );
  },
};
