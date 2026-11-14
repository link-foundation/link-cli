// An optional permission layer with permissions stored in links (issue #149),
// the port of rust/src/permissions.rs and rust/src/permissions/named_links.rs
// (itself the port of `createPolicy` and `withPermissions` of
// `@link-foundation/ui-links`; the C# counterparts are
// `PermissionPolicy<TLinkAddress>` and `PermissionsDecorator<TLinkAddress>`).
//
// A rule is a pair `(g: Subject Object)` marked as `(r: AllowRead g)` or
// `(r: AllowWrite g)`: the subjects that are `Subject` (the subject itself,
// or a link of that type) may read, or read and write, the links of type
// `Object` -- its values and sub-types, not the link of `Object` itself.
// `Anything` in either place matches every subject or every link. Types are
// those of `TypesDecorator` (issue #148).
//
// Referring to a link is a right of its own (issue #161). Without it an
// address the subject may not read answers `Link not found`, exactly as a
// missing one. A subject may refer to an address when it may read the link
// there, when it is that link, when a rule `(r: AllowReference (g: Subject
// Object))` covers it (`Object` itself and the links of type `Object`), or
// when it is the `Object` of an `AllowRead` or `AllowWrite` rule of the
// subject, so `User writes Note` is enough to type a link as `Note`.
// `AllowReference` does not allow reading. A rule from `Anything` to
// `Anything` lets every subject refer to every link.
//
// The markers are points of the data, so every address a rule holds is an
// address of the data. The rules live in the data itself or in a store of
// their own. Rules in the data are links like any other: a subject that may
// write them may change the rules, so give `Anything` writes to
// administrators only, or keep the rules apart.
import { ensurePoint, TypeMarkers, TypesDecorator } from './types.js';

export { LinksTokenTable } from './permissions-tokens.js';
export { Forbidden, PermissionsDecorator } from './permissions-named-links.js';

/** What a rule allows: reading, writing (with reading), or referring. */
export const PermissionAccess = Object.freeze({
  Read: 'read',
  Write: 'write',
  Reference: 'reference',
});

/** Parses `read`, `write` or `reference`. */
export function parsePermissionAccess(access) {
  if (Object.values(PermissionAccess).includes(access)) return access;
  throw new Error(`Unknown access: ${access}`);
}

/**
 * The four points of the data permission rules are made of: the markers
 * `AllowRead`, `AllowWrite` and `AllowReference` of rules and `Anything`,
 * which matches every subject or every link.
 */
export class PermissionMarkers {
  static ALLOW_READ_NAME = 'AllowRead';
  static ALLOW_WRITE_NAME = 'AllowWrite';
  static ANYTHING_NAME = 'Anything';
  static ALLOW_REFERENCE_NAME = 'AllowReference';

  constructor(allowRead, allowWrite, anything, allowReference) {
    this.allowRead = allowRead;
    this.allowWrite = allowWrite;
    this.anything = anything;
    this.allowReference = allowReference;
    Object.freeze(this);
  }

  /**
   * The points named `AllowRead`, `AllowWrite`, `Anything` and
   * `AllowReference`, each created as a point `(x: x x)` and named when
   * missing. `AllowReference` comes last, so the others keep their addresses
   * in a store made before it was added.
   */
  static ensure(links) {
    const allowRead = ensurePoint(links, PermissionMarkers.ALLOW_READ_NAME);
    const allowWrite = ensurePoint(links, PermissionMarkers.ALLOW_WRITE_NAME);
    const anything = ensurePoint(links, PermissionMarkers.ANYTHING_NAME);
    const allowReference = ensurePoint(links, PermissionMarkers.ALLOW_REFERENCE_NAME);
    return new PermissionMarkers(allowRead, allowWrite, anything, allowReference);
  }

  /** The marker of the rules of `access`. */
  of(access) {
    if (access === PermissionAccess.Read) return this.allowRead;
    if (access === PermissionAccess.Write) return this.allowWrite;
    return this.allowReference;
  }
}

/**
 * Who may read, write and refer to what (`AccessPolicy` in Rust).
 *
 * A subject is a link address; the null link (0) is a guest, which no rule
 * but one for `Anything` matches. A link is given as `{index, source,
 * target}`, so a link that is not stored yet, or not any more, can be asked
 * about too. `data` is the store the links are in, for their types. A policy
 * implements `markers()`, `canRead(data, subject, link)`, `canWrite(data,
 * subject, link)` and `canCreate(data, subject)`; `canReference` is optional.
 */
export class AccessPolicy {
  /**
   * Whether `subject` may refer to `link` although it need not read it. A
   * policy without this right allows references to readable links only.
   */
  canReference(_data, _subject, _link) {
    return false;
  }
}

/** Whether `subject` is `typeId` or of it; a guest is only `Anything`. */
function isSubject(types, anything, subject, typeId) {
  return typeId === anything || (subject !== 0 && types.isA(subject, typeId));
}

/** Permissions stored as links; see the module comment. */
export class PermissionPolicy extends AccessPolicy {
  #markers;
  #types;
  #rules;

  /**
   * The policy of the rules marked by `markers`, with the types of `types`:
   * in the data when `rules` is `null`, else in `rules`.
   */
  constructor(markers, types, rules = null) {
    super();
    this.#markers = markers;
    this.#types = types;
    this.#rules = rules;
  }

  /**
   * The policy of the rules in `data`, whose points named `Type`, `Extends`,
   * `AllowRead`, `AllowWrite`, `Anything` and `AllowReference` are created
   * when missing.
   */
  static inData(data) {
    const types = TypeMarkers.ensure(data);
    const markers = PermissionMarkers.ensure(data);
    return new PermissionPolicy(markers, types, null);
  }

  /**
   * The policy of the rules in `rules`, a store of their own, over `data`,
   * whose points are created when missing as by `inData`.
   */
  static withRules(data, rules) {
    const types = TypeMarkers.ensure(data);
    const markers = PermissionMarkers.ensure(data);
    return new PermissionPolicy(markers, types, rules);
  }

  markers() {
    return this.#markers;
  }

  /** The type markers of the data. */
  types() {
    return this.#types;
  }

  /** The store of the rules when they are kept apart from the data, else `null`. */
  rulesMut() {
    return this.#rules;
  }

  /** The store of the rules: `data` itself, or the rules' own store. */
  rulesOf(data) {
    return this.#rules ?? data;
  }

  /** The rules `[Subject, Object]` that `marker` marks, in the order of their marks. */
  granted(data, marker) {
    const rules = this.rulesOf(data);
    const marks = rules
      .queryLinks(null, marker, null)
      // The marker point (x: x x) itself is no mark; in a store of rules of
      // their own a mark may sit at the marker's address.
      .filter((mark) => mark.index !== marker || mark.target !== marker)
      .map((mark) => [mark.index, mark.target])
      .sort((left, right) => left[0] - right[0]);
    const granted = [];
    for (const [, rule] of marks) {
      if (rule === 0) continue;
      const value = rules.getLink(rule);
      if (value != null) granted.push([value.source, value.target]);
    }
    return granted;
  }

  /** Whether a rule of `marker` lets `subject` at `link`. */
  allows(data, marker, subject, link) {
    const granted = this.granted(data, marker);
    if (granted.length === 0) return false;
    const anything = this.#markers.anything;
    const types = TypesDecorator.withMarkers(data, this.#types);
    return granted.some(
      ([ruleSubject, ruleObject]) =>
        isSubject(types, anything, subject, ruleSubject) &&
        (ruleObject === anything || types.isOfLink(link, ruleObject)),
    );
  }

  /**
   * Writes a rule into `rules`: `subject` may `access` the links of
   * `object`. The rule is the pair `(subject object)` marked by `(marker
   * rule)`; returns the pair.
   */
  static allow(rules, markers, access, subject, object) {
    if (
      subject === markers.allowRead ||
      subject === markers.allowWrite ||
      subject === markers.allowReference
    ) {
      // A pair from a marker would read as a mark.
      throw new Error('A marker cannot be a subject');
    }
    // A rule given twice is the same rule: the pair and its mark are found
    // before they are made.
    const rule = rules.tryGetOrCreate(subject, object);
    rules.tryGetOrCreate(markers.of(access), rule);
    return rule;
  }

  /** Parses `read`, `write` or `reference`. */
  static parseAccess(access) {
    return parsePermissionAccess(access);
  }

  canRead(data, subject, link) {
    const markers = this.#markers;
    return (
      this.allows(data, markers.allowRead, subject, link) ||
      this.allows(data, markers.allowWrite, subject, link)
    );
  }

  canWrite(data, subject, link) {
    return this.allows(data, this.#markers.allowWrite, subject, link);
  }

  canCreate(data, subject) {
    const granted = this.granted(data, this.#markers.allowWrite);
    const anything = this.#markers.anything;
    const types = TypesDecorator.withMarkers(data, this.#types);
    return granted.some(([ruleSubject]) => isSubject(types, anything, subject, ruleSubject));
  }

  canReference(data, subject, link) {
    const markers = this.#markers;
    if (
      (subject !== 0 && link.index === subject) ||
      this.allows(data, markers.allowReference, subject, link)
    ) {
      return true;
    }
    // The object of a rule of the subject is named by that rule: a readable
    // link of that type shows its address anyway.
    return [markers.allowRead, markers.allowWrite, markers.allowReference].some((marker) => {
      const granted = this.granted(data, marker);
      const types = TypesDecorator.withMarkers(data, this.#types);
      return granted.some(
        ([ruleSubject, ruleObject]) =>
          ruleObject === link.index && isSubject(types, markers.anything, subject, ruleSubject),
      );
    });
  }
}
