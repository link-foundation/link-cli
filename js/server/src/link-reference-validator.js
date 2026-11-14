// Checking that every link a query refers to exists, and creating the ones
// that do not when the caller asked for that. Port of
// rust/src/link_reference_validator.rs (itself ported from the C#
// `LinkReferenceValidator`), public so a custom front end that resolves
// references its own way can reuse, or replace, exactly this step.
import { QueryError } from './errors.js';
import { link, parseU32 } from './link.js';
import { name as envelopedName } from './query-parser.js';

/** Writes one diagnostic line to stderr, as Rust's `diagnostics::stderr!`. */
export const writeStderrLine = (line) => {
  process.stderr.write(`${line}\n`);
};

/** Compares strings by Unicode code point, which is Rust's (UTF-8 byte) `str` order. */
export function compareCodePoints(left, right) {
  const a = [...left];
  const b = [...right];
  const length = Math.min(a.length, b.length);
  for (let position = 0; position < length; position += 1) {
    const difference = a[position].codePointAt(0) - b[position].codePointAt(0);
    if (difference !== 0) return difference;
  }
  return a.length - b.length;
}

/** Rust's `{:?}` of a `str`: quoted, with `char::escape_debug` escapes. */
export function debugString(text) {
  let out = '"';
  for (const character of text) {
    const code = character.codePointAt(0);
    if (character === '"' || character === '\\') out += `\\${character}`;
    else if (character === '\n') out += '\\n';
    else if (character === '\r') out += '\\r';
    else if (character === '\t') out += '\\t';
    else if (code === 0) out += '\\0';
    else if (code < 0x20 || (code >= 0x7f && code < 0xa0)) out += `\\u{${code.toString(16)}}`;
    else out += character;
  }
  return `${out}"`;
}

/**
 * Rust's `{:?}` of a `HashSet`. Rust's iteration order is randomised per
 * process; this prints the insertion order.
 */
const debugSet = (set, format) => `{${[...set].map(format).join(', ')}}`;

function concreteIdentifier(id) {
  if (id == null) return null;
  const identifier = id.replace(/:+$/, '');
  if (identifier === '' || identifier === '*' || identifier.startsWith('$')) return null;
  return identifier;
}

const concreteNumericIdentifier = (id) => {
  const identifier = concreteIdentifier(id);
  return identifier === null ? null : parseU32(identifier);
};

const isCompositeLino = (lino) => lino.values != null && lino.values.length === 2;

class LinkReferencePlan {
  constructor() {
    this.numericIdsToBeCreated = new Set();
    this.namesToBeCreated = new Set();
    // `(source, target)` pairs the substitution itself defines. A missing
    // numeric reference whose own point pair `(id, id)` appears here is left
    // as a `(id: 0 0)` placeholder, so the substitution about to write that
    // exact pair does not collide with it under uniqueness resolution.
    this.compositePairsToBeCreated = new Set();
    this.missingReferences = [];
    this.missingReferenceKeys = new Set();
  }

  addMissingReference(reference) {
    const key =
      reference.numericId === null ? `name:${reference.identifier}` : `id:${reference.numericId}`;
    if (this.missingReferenceKeys.has(key)) return;
    this.missingReferenceKeys.add(key);
    this.missingReferences.push(reference);
  }
}

export class LinkReferenceValidator {
  /**
   * `options.stderr` receives each trace line (default: process stderr).
   * `options.envelopedNames` is set by `forQuery`: identifiers carry the
   * query parser's quote envelope.
   */
  constructor(trace = false, autoCreateMissingReferences = false, options = {}) {
    this.trace = Boolean(trace);
    this.autoCreateMissingReferences = Boolean(autoCreateMissingReferences);
    this.stderr = options.stderr ?? writeStderrLine;
    this.envelopedNames = Boolean(options.envelopedNames);
  }

  static forQuery(trace, autoCreateMissingReferences, options = {}) {
    return new LinkReferenceValidator(trace, autoCreateMissingReferences, {
      ...options,
      envelopedNames: true,
    });
  }

  name(identifier) {
    return this.envelopedNames ? envelopedName(identifier) : identifier;
  }

  /** Validate already-decoded identifiers supplied by a custom front end. */
  validateLinksExistOrWillBeCreated(storage, restrictionPatterns, substitutionPatterns) {
    return this.validateWithPlan(storage, restrictionPatterns, substitutionPatterns, () => null);
  }

  /**
   * Returns the changes auto-creation made, as `[before, after]` pairs with
   * `null` for "no link". `reserve(storage)` returns a `Set` of the addresses
   * the query will allocate, or `null` to predict them from the store.
   */
  validateWithPlan(storage, restrictionPatterns, substitutionPatterns, reserve) {
    this.traceMsg('[ValidateLinksExistOrWillBeCreated] Starting validation');
    const created = [];
    for (;;) {
      const reserved = reserve(storage);
      const plan = this.planReferences(
        storage,
        restrictionPatterns,
        substitutionPatterns,
        reserved,
      );
      if (plan.missingReferences.length === 0) {
        this.traceMsg('[ValidateLinksExistOrWillBeCreated] Validation completed');
        return created;
      }
      if (!this.autoCreateMissingReferences) {
        const missing = this.name(plan.missingReferences[0].identifier);
        throw new QueryError(
          `Invalid reference to non-existent link '${missing}' in ${plan.missingReferences[0].patternType} pattern. Link '${missing}' does not exist and will not be created by this operation. Use --auto-create-missing-references to create missing references as point links.`,
        );
      }
      // An explicit ensure or named reference can change subsequent
      // allocations. Replan while retaining this session's owned prefix.
      created.push(...this.autoCreate(storage, plan));
    }
  }

  planReferences(storage, restrictionPatterns, substitutionPatterns, reserved) {
    const plan = new LinkReferencePlan();
    const counter = { anonymous: 0 };
    for (const pattern of substitutionPatterns) collectDefinitions(pattern, plan, counter);
    const ids = reserved ?? storage.nextCreatedAddresses(counter.anonymous);
    for (const id of ids) plan.numericIdsToBeCreated.add(id);
    if (this.trace) {
      this.traceMsg(
        `[ValidateLinksExistOrWillBeCreated] Numeric links to be created: ${debugSet(plan.numericIdsToBeCreated, String)}`,
      );
      this.traceMsg(
        `[ValidateLinksExistOrWillBeCreated] Named links to be created: ${debugSet(plan.namesToBeCreated, debugString)}`,
      );
    }
    for (const pattern of restrictionPatterns) {
      this.collectMissing(storage, plan, pattern, false, 'restriction');
    }
    for (const pattern of substitutionPatterns) {
      this.collectMissing(storage, plan, pattern, true, 'substitution');
    }
    return plan;
  }

  collectMissing(storage, plan, pattern, isSubstitution, patternType) {
    const identifier = concreteIdentifier(pattern.id);
    const patternIdIsDefinition = isSubstitution && isCompositeLino(pattern) && identifier !== null;
    if (!patternIdIsDefinition && identifier !== null) {
      this.validateReferenceIdentifier(storage, plan, identifier, patternType);
    }
    for (const subPattern of pattern.values ?? []) {
      this.collectMissing(storage, plan, subPattern, isSubstitution, patternType);
    }
  }

  validateReferenceIdentifier(storage, plan, identifier, patternType) {
    const linkId = parseU32(identifier);
    if (linkId !== null) {
      if (!storage.referenceExists(linkId) && !plan.numericIdsToBeCreated.has(linkId)) {
        plan.addMissingReference({ identifier, patternType, numericId: linkId });
        return;
      }
      this.traceMsg(
        `[ValidateReferencesInPattern] Link ${linkId} reference validated in ${patternType} pattern`,
      );
      return;
    }
    if (
      storage.getByName(this.name(identifier)) == null &&
      !plan.namesToBeCreated.has(identifier)
    ) {
      plan.addMissingReference({ identifier, patternType, numericId: null });
      return;
    }
    this.traceMsg(
      `[ValidateReferencesInPattern] Named link '${identifier}' reference validated in ${patternType} pattern`,
    );
  }

  /**
   * Creates every missing reference as a point link and reports each creation
   * the way it happens: a numeric reference is first filled in as the empty
   * `(id: 0 0)` and then pointed at itself, a named one is created as
   * `(name: name name)` directly. A numeric reference the substitution defines
   * as `(id: id id)` itself is only filled in: the query writes the point link.
   */
  autoCreate(storage, plan) {
    const created = [];
    const numericReferences = [
      ...new Set(
        plan.missingReferences
          .filter((reference) => reference.numericId !== null)
          .map((reference) => reference.numericId),
      ),
    ].sort((left, right) => left - right);
    for (const linkId of numericReferences) {
      if (storage.referenceExists(linkId)) continue;
      this.traceMsg(
        `[ValidateLinksExistOrWillBeCreated] Auto-creating missing numeric reference ${linkId}.`,
      );
      storage.tryEnsureCreated(linkId);
      const placeholder = link(linkId, 0, 0);
      created.push([null, placeholder]);
      if (plan.compositePairsToBeCreated.has(`${linkId},${linkId}`)) {
        this.traceMsg(
          `[ValidateLinksExistOrWillBeCreated] Link ${linkId} exists as a placeholder because (${linkId}, ${linkId}) is defined by the substitution.`,
        );
        continue;
      }
      storage.update(linkId, linkId, linkId);
      created.push([placeholder, storage.getLink(linkId)]);
    }
    const namedReferences = [
      ...new Set(
        plan.missingReferences
          .filter((reference) => reference.numericId === null)
          .map((reference) => reference.identifier),
      ),
    ].sort(compareCodePoints);
    for (const identifier of namedReferences) {
      if (storage.getByName(this.name(identifier)) != null) continue;
      this.traceMsg(
        `[ValidateLinksExistOrWillBeCreated] Auto-creating missing named reference '${identifier}' as point link.`,
      );
      const linkId = storage.getOrCreateNamed(this.name(identifier));
      created.push([null, storage.getLink(linkId)]);
    }
    return created;
  }

  traceMsg(message) {
    if (this.trace) this.stderr(message);
  }
}

/**
 * Collects the ids and names the substitution defines, the `(source, target)`
 * pairs it writes under them, and how many links it writes without an id.
 */
function collectDefinitions(pattern, plan, counter) {
  for (const subPattern of pattern.values ?? []) collectDefinitions(subPattern, plan, counter);
  if (!isCompositeLino(pattern)) return;
  const identifier = concreteIdentifier(pattern.id);
  if (identifier === null) {
    counter.anonymous += 1;
    return;
  }
  const linkId = parseU32(identifier);
  if (linkId === null) plan.namesToBeCreated.add(identifier);
  else plan.numericIdsToBeCreated.add(linkId);
  const source = concreteNumericIdentifier(pattern.values[0].id);
  const target = concreteNumericIdentifier(pattern.values[1].id);
  if (source !== null && target !== null) plan.compositePairsToBeCreated.add(`${source},${target}`);
}
