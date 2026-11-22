// QueryProcessor: LiNo query parsing and execution, the port of
// rust/src/query_processor.rs (`AdvancedMixedQueryProcessor` in C#). Matching,
// resolution and the write side live in query-matching.js,
// query-resolution.js and query-mutations.js, as the Rust submodules do.
//
// A change is a `[before, after]` pair of links where `null` is no link:
// `[null, link]` is a creation and `[link, null]` a deletion.
import { simplifyChanges } from './changes-simplifier.js';
import { QueryError } from './errors.js';
import { isNull, NULL_LINK, sameLink } from './link.js';
import {
  debugString,
  LinkReferenceValidator,
  writeStderrLine,
} from './link-reference-validator.js';
import { isEmptyLino, valuesCount } from './parser.js';
import {
  bindingCost,
  checkResultCount,
  checkSolutionCapacity,
  isNormalIndex,
  linksMatchingDefinition,
  matchingMethods,
  resolveUnspecified,
  solutionCost,
  solutionsAreCompatible,
} from './query-matching.js';
import { mutationMethods } from './query-mutations.js';
import { Pattern, QueryReplyBudget, utf8Length } from './query-options.js';
import { name as nameOf, parseFor, rustTrim } from './query-parser.js';
import { resolutionMethods } from './query-resolution.js';

const debugOption = (value) => (value == null ? 'None' : `Some(${debugString(value)})`);

function createPatternFromLino(lino) {
  const index = lino.id ?? '';
  if (lino.values != null && lino.values.length === 2) {
    return new Pattern(
      index,
      createPatternFromLino(lino.values[0]),
      createPatternFromLino(lino.values[1]),
    );
  }
  return new Pattern(index);
}

/**
 * The C# `SimplifyChanges` pass: a missing side becomes the null link on the
 * way in and turns back into `null` on the way out.
 */
export function simplifyChangesList(changes) {
  return simplifyChanges(
    changes.map(([before, after]) => [before ?? NULL_LINK, after ?? NULL_LINK]),
  ).map(([before, after]) => [isNull(before) ? null : before, isNull(after) ? null : after]);
}

export class QueryProcessor {
  /**
   * `options.autoCreateMissingReferences` creates referenced links that do
   * not exist as point links; `options.stderr` receives each trace line
   * (default: process stderr).
   */
  constructor(trace = false, options = {}) {
    this.trace = Boolean(trace);
    this.autoCreateMissingReferences = Boolean(options.autoCreateMissingReferences);
    this.validateReferences = options.validateReferences ?? true;
    this.stderr = options.stderr ?? writeStderrLine;
  }

  withAutoCreateMissingReferences(autoCreateMissingReferences) {
    this.autoCreateMissingReferences = Boolean(autoCreateMissingReferences);
    return this;
  }

  /**
   * Processes a LiNo query and returns the simplified list of changes, as the
   * C# CLI does for every scenario.
   */
  processQuery(storage, query) {
    return simplifyChangesList(this.processQueryRaw(storage, query));
  }

  /** Applies `query` and reports the raw changes, in the order they happened. */
  processQueryRaw(storage, query) {
    const limits = storage.queryLimits();
    if (limits != null && utf8Length(query) > limits.maxBytes) {
      throw new QueryError('query byte limit exceeded');
    }
    this.traceMsg(`[ProcessQuery] Query: "${query}"`);
    const trimmed = rustTrim(query);
    if (trimmed === '') {
      this.traceMsg('[ProcessQuery] Query is empty, returning.');
      return [];
    }
    const parsedLinks = parseFor(trimmed, storage.queryLimits());
    this.traceMsg(`[ProcessQuery] Parser returned ${parsedLinks.length} top-level link(s).`);
    if (parsedLinks.length === 0) {
      this.traceMsg('[ProcessQuery] No top-level parsed links found, returning.');
      return [];
    }
    // Accept both the wrapped form `((restriction) (substitution))` and the
    // C# parser-compatible form `restriction substitution`.
    const first = parsedLinks[0];
    if (first.values != null && first.values.length >= 2) {
      return this.processLinoRaw(storage, first.values[0], first.values[1]);
    }
    if (parsedLinks.length >= 2) return this.processLinoRaw(storage, first, parsedLinks[1]);
    this.traceMsg('[ProcessQuery] Query has fewer than 2 links, returning.');
    return [];
  }

  /**
   * Runs the complete execution path on an owned scratch store with reference
   * validation disabled; a checked store also charges the reply's references.
   */
  preview(storage, restrictionLink, substitutionLink) {
    const preview = new QueryProcessor(this.trace, {
      autoCreateMissingReferences: this.autoCreateMissingReferences,
      validateReferences: false,
      stderr: this.stderr,
    });
    const changes = preview.processLinoRaw(storage, restrictionLink, substitutionLink);
    const limits = storage.queryLimits();
    if (limits != null) {
      // The protocol names references from the final staged state.
      const budget = new QueryReplyBudget(limits);
      for (const [before, after] of simplifyChangesList(changes)) {
        for (const value of [before, after]) {
          if (value === null) continue;
          for (const id of [value.index, value.source, value.target]) budget.reference(storage, id);
        }
      }
    }
  }

  processLinoRaw(storage, restrictionLink, substitutionLink) {
    if (this.trace) {
      this.traceMsg(
        `[ProcessQuery] Restriction link => Id=${debugOption(restrictionLink.id)} Values.Count=${valuesCount(restrictionLink)}`,
      );
      this.traceMsg(
        `[ProcessQuery] Substitution link => Id=${debugOption(substitutionLink.id)} Values.Count=${valuesCount(substitutionLink)}`,
      );
    }
    const restrictionEmpty = isEmptyLino(restrictionLink);
    const substitutionEmpty = isEmptyLino(substitutionLink);
    if (restrictionEmpty && substitutionEmpty) {
      this.traceMsg(
        '[ProcessQuery] Restriction & substitution both empty => no operation, returning.',
      );
      return [];
    }
    if (restrictionEmpty) return this.processCreation(storage, restrictionLink, substitutionLink);
    if (substitutionEmpty) return this.processDeletion(storage, restrictionLink, substitutionLink);
    return this.processUpdate(storage, restrictionLink, substitutionLink);
  }

  processCreation(storage, restrictionLink, substitutionLink) {
    this.traceMsg(
      '[ProcessQuery] No restriction, but substitution is non-empty => creation scenario.',
    );
    const changes = [];
    if (substitutionLink.values != null) {
      changes.push(...this.validate(storage, restrictionLink, substitutionLink));
      for (const value of substitutionLink.values) {
        const createdId = this.ensureLinkCreated(storage, value, changes);
        this.traceMsg(`[ProcessQuery] Created link ID #${createdId} from substitution pattern.`);
      }
    }
    storage.save();
    return changes;
  }

  processDeletion(storage, restrictionLink, substitutionLink) {
    this.traceMsg('[ProcessQuery] Restriction non-empty, substitution empty => deletion scenario.');
    const changes = [...this.validate(storage, restrictionLink, substitutionLink)];
    const linksToDelete = [];
    for (const pattern of this.patternsFromLino(restrictionLink)) {
      linksToDelete.push(...this.matchedLinks(storage, pattern, new Map()));
    }
    for (const value of dedupByIndex(linksToDelete)) {
      if (!storage.exists(value.index)) continue;
      this.deleteObserved(storage, value.index, changes);
      this.traceMsg(`[ProcessQuery] Deleted link ID #${value.index}.`);
    }
    storage.save();
    return changes;
  }

  processUpdate(storage, restrictionLink, substitutionLink) {
    this.traceMsg(
      '[ProcessQuery] Both restriction and substitution non-empty => update/mixed scenario.',
    );
    const restrictionPatterns = this.patternsFromLino(restrictionLink);
    const substitutionPatterns = this.patternsFromLino(substitutionLink);
    const changes = [...this.validate(storage, restrictionLink, substitutionLink)];
    const solutions = this.findAllSolutions(storage, restrictionPatterns);
    if (solutions.length === 0) {
      this.traceMsg('[ProcessQuery] No solutions found => returning.');
      if (changes.length > 0) storage.save();
      return changes;
    }
    const allNoOperation = solutions.every((solution) =>
      this.solutionIsNoOperation(storage, solution, restrictionPatterns, substitutionPatterns),
    );
    if (allNoOperation) {
      for (const solution of solutions) {
        for (const pattern of restrictionPatterns) {
          for (const value of this.matchedLinks(storage, pattern, solution)) {
            const seen = changes.some((change) => change.every((side) => sameLink(side, value)));
            if (!seen) changes.push([value, value]);
          }
        }
      }
      return changes;
    }
    const operations = [];
    for (const solution of solutions) {
      const restrictions = this.resolvePatterns(
        storage,
        restrictionPatterns,
        solution,
        false,
        changes,
      );
      const substitutions = this.resolvePatterns(
        storage,
        substitutionPatterns,
        solution,
        true,
        changes,
      );
      const planned = this.determineOperations(restrictions, substitutions);
      checkResultCount(storage, operations.length + planned.length);
      operations.push(...planned);
    }
    const intendedFinalStates = intendedFinalStatesOf(operations);
    for (const [before, after] of operations) this.applyOperation(storage, before, after, changes);
    this.restoreUnexpectedDeletions(storage, intendedFinalStates, changes);
    storage.save();
    return changes;
  }

  validate(storage, restriction, substitution) {
    if (!this.validateReferences) return [];
    return LinkReferenceValidator.forQuery(this.trace, this.autoCreateMissingReferences, {
      stderr: this.stderr,
    }).validateWithPlan(storage, restriction.values ?? [], substitution.values ?? [], (store) =>
      store.reserveQueryAllocations(this, restriction, substitution),
    );
  }

  patternsFromLino(lino) {
    const patterns = (lino.values ?? []).map(createPatternFromLino);
    if (lino.id != null) patterns.unshift(createPatternFromLino(lino));
    return patterns;
  }

  findAllSolutions(storage, patterns) {
    let partialSolutions = [new Map()];
    for (const pattern of patterns) {
      const newSolutions = [];
      let bindings = 0;
      let bytes = 0;
      for (const solution of partialSolutions) {
        for (const matched of this.matchPattern(storage, pattern, solution)) {
          if (!solutionsAreCompatible(solution, matched)) continue;
          // Admit the union before copying either keys or maps.
          let [count, size] = solutionCost(solution);
          for (const key of matched.keys()) {
            if (solution.has(key)) continue;
            count += 1;
            size += bindingCost(key);
          }
          checkSolutionCapacity(storage, newSolutions.length + 1, bindings + count, bytes + size);
          newSolutions.push(new Map([...solution, ...matched]));
          bindings += count;
          bytes += size;
        }
      }
      partialSolutions = newSolutions;
      if (partialSolutions.length === 0) break;
    }
    return partialSolutions;
  }

  determineOperations(restrictions, substitutions) {
    const restrictionByIndex = new Map();
    const substitutionByIndex = new Map();
    const wildcardRestrictions = [];
    const wildcardSubstitutions = [];
    for (const restriction of restrictions) {
      if (isNormalIndex(restriction.index)) restrictionByIndex.set(restriction.index, restriction);
      else wildcardRestrictions.push(restriction);
    }
    for (const substitution of substitutions) {
      if (isNormalIndex(substitution.index)) {
        substitutionByIndex.set(substitution.index, substitution);
      } else {
        wildcardSubstitutions.push(substitution);
      }
    }
    const indices = [...new Set([...restrictionByIndex.keys(), ...substitutionByIndex.keys()])];
    indices.sort((left, right) => left - right);
    return [
      ...indices.map((index) => [
        restrictionByIndex.get(index) ?? null,
        substitutionByIndex.get(index) ?? null,
      ]),
      ...wildcardRestrictions.map((restriction) => [restriction, null]),
      ...wildcardSubstitutions.map((substitution) => [null, substitution]),
    ];
  }

  applyOperation(storage, before, after, changes) {
    if (before !== null && after === null) {
      for (const value of dedupByIndex(linksMatchingDefinition(storage, before))) {
        if (storage.exists(value.index)) this.deleteObserved(storage, value.index, changes);
      }
    } else if (before === null && after !== null) {
      this.createOrUpdateResolvedLink(storage, after, changes);
    } else if (before !== null) {
      if (before.index === after.index && storage.exists(before.index)) {
        const stored = storage.getLink(before.index);
        // A part the query left open keeps its value.
        const source = resolveUnspecified(after.source, stored.source);
        const target = resolveUnspecified(after.target, stored.target);
        if (stored.source !== source || stored.target !== target) {
          // A merge into a duplicate deletes `before.index`; the restore pass
          // puts it back when the query asked for it, as C# does.
          this.updateObserved(storage, before.index, source, target, changes);
        } else {
          changes.push([stored, stored]);
        }
        if (after.name != null) storage.setName(before.index, nameOf(after.name));
      } else {
        this.applyOperation(storage, before, null, changes);
        this.applyOperation(storage, null, after, changes);
      }
    }
  }

  traceMsg(message) {
    if (this.trace) this.stderr(message);
  }
}

/** Sorted by address, one link per address (the first of each). */
function dedupByIndex(links) {
  const sorted = [...links].sort((left, right) => left.index - right.index);
  return sorted.filter(
    (value, position) => position === 0 || sorted[position - 1].index !== value.index,
  );
}

/**
 * The final state every planned operation asks for, keyed by address in
 * planning order; `null` marks a link the query deliberately deletes.
 */
function intendedFinalStatesOf(operations) {
  const states = new Map();
  for (const [before, after] of operations) {
    if (after !== null && isNormalIndex(after.index)) states.set(after.index, after);
    else if (before !== null && after === null && isNormalIndex(before.index)) {
      states.set(before.index, null);
    }
  }
  return [...states];
}

Object.assign(QueryProcessor.prototype, matchingMethods, resolutionMethods, mutationMethods);
