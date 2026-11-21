// Write-side operations for `QueryProcessor`: deletion, creation, update and
// the restore pass that undoes cascades a query did not ask for, mirroring the
// C# `AdvancedMixedQueryProcessor.Mutations.cs`. Port of
// rust/src/query_processor/mutations.rs.
import { InvalidFormatError } from './errors.js';
import { ANY, isNull, link, parseU32 } from './link.js';
import { hasValues, valuesCount } from './parser.js';
import {
  isNormalIndex,
  isVariable,
  resolveUnspecified,
  searchUnspecified,
} from './query-matching.js';
import { name } from './query-parser.js';

/** A store change as `--changes` reports it: a null link is no link at all. */
const change = (before, after) => [isNull(before) ? null : before, isNull(after) ? null : after];

/** The mutation methods, installed on `QueryProcessor.prototype`. */
export const mutationMethods = {
  /** Deletes `id`, appending every resulting change (the cascade too) to `changes`. */
  deleteObserved(storage, id, changes) {
    return storage.deleteObserved(id, (before, after) => changes.push(change(before, after)));
  },

  /** Updates `id`, appending every resulting change (a merge too) to `changes`. */
  updateObserved(storage, id, source, target, changes) {
    return storage.updateObserved(id, source, target, (before, after) =>
      changes.push(change(before, after)),
    );
  },

  /**
   * Recreates links that a resolved write removed as a side effect (the
   * C# `RestoreUnexpectedLinkDeletions`): when the query itself asked for a
   * link to exist, its deletion by uniqueness resolution is undone.
   */
  restoreUnexpectedDeletions(storage, intendedFinalStates, changes) {
    for (const [index, intended] of intendedFinalStates) {
      if (intended === null) {
        this.traceMsg(
          `[RestoreUnexpectedLinkDeletions] Link<T> ${index} was intended-deletion => skip restore.`,
        );
        continue;
      }
      if (storage.exists(index)) continue;
      this.traceMsg(
        `[RestoreUnexpectedLinkDeletions] Recreating link ${index} => was unexpected deletion.`,
      );
      this.createOrUpdateResolvedLink(storage, intended, changes);
    }
  },

  /**
   * Creates or updates the link a resolved definition asks for, the C#
   * `CreateOrUpdateLink`. A definition that already matches its stored state
   * reports that state unchanged and does not write.
   */
  createOrUpdateResolvedLink(storage, definition, changes) {
    let id;
    if (isNormalIndex(definition.index)) {
      this.ensureAddress(storage, definition.index, changes);
      const existing = storage.getLink(definition.index) ?? link(definition.index, 0, 0);
      const source = resolveUnspecified(definition.source, existing.source);
      const target = resolveUnspecified(definition.target, existing.target);
      if (existing.source !== source || existing.target !== target) {
        this.traceMsg(
          `[CreateOrUpdateLink] Updating link ${definition.index}: ${existing.source}->${source}, ${existing.target}->${target}.`,
        );
        this.updateObserved(storage, definition.index, source, target, changes);
      } else {
        this.traceMsg(
          `[CreateOrUpdateLink] Link<T> ${definition.index} is already S=${source}, T=${target} => no change.`,
        );
        changes.push([existing, existing]);
      }
      id = definition.index;
    } else {
      const existingId = searchUnspecified(storage, definition.source, definition.target);
      if (existingId !== null) {
        this.traceMsg(
          `[CreateOrUpdateLink] Link<T> already found => ID=${existingId}, no changes.`,
        );
        const existing =
          storage.getLink(existingId) ?? link(existingId, definition.source, definition.target);
        changes.push([existing, existing]);
        id = existingId;
      } else {
        const source = resolveUnspecified(definition.source, 0);
        const target = resolveUnspecified(definition.target, 0);
        this.traceMsg(`[CreateOrUpdateLink] Creating new link => (S=${source},T=${target}).`);
        id = storage.tryCreate(source, target);
        changes.push([null, storage.getLink(id)]);
      }
    }
    if (definition.name != null) storage.setName(id, name(definition.name));
    return id;
  },

  /**
   * Ensures a link is created from a LiNo pattern, recursing into its parts
   * (`EnsureNestedLinkCreatedRecursively` in C#). Every doublet it touches,
   * nested ones and named leaves included, is reported in `changes`.
   */
  ensureLinkCreated(storage, lino, changes) {
    if (!hasValues(lino)) {
      if (lino.id == null) return 0;
      if (lino.id === '*' || isVariable(lino.id)) return ANY;
      const number = parseU32(lino.id);
      if (number !== null) return number;
      return this.ensureNamedPointLink(storage, lino.id, changes);
    }
    if (valuesCount(lino) !== 2) throw new InvalidFormatError('Invalid link structure');
    const sourceId = this.ensureLinkCreated(storage, lino.values[0], changes);
    const targetId = this.ensureLinkCreated(storage, lino.values[1], changes);
    const id = lino.id;
    if (id == null || id === '*' || isVariable(id)) {
      return this.ensureDoublet(storage, sourceId, targetId, changes);
    }
    const number = parseU32(id);
    if (number !== null)
      return this.ensureIndexedLink(storage, number, sourceId, targetId, changes);
    const named = storage.getByName(name(id));
    if (named != null) return this.ensureIndexedLink(storage, named, sourceId, targetId, changes);
    // A doublet is unique, so a new name for an existing `(source, target)`
    // names that link instead of copying it.
    const doublet = this.ensureDoublet(storage, sourceId, targetId, changes);
    storage.setName(doublet, name(id));
    return doublet;
  },

  /** `EnsureLinkCreated` for a definition that names its own address. */
  ensureIndexedLink(storage, index, source, target, changes) {
    this.ensureAddress(storage, index, changes);
    const stored = storage.getLink(index) ?? link(index, 0, 0);
    const resolvedSource = resolveUnspecified(source, stored.source);
    const resolvedTarget = resolveUnspecified(target, stored.target);
    if (stored.source !== resolvedSource || stored.target !== resolvedTarget) {
      this.traceMsg(
        `[EnsureLinkCreated] Updating link ${index} => ${stored.source}->${resolvedSource}, ${stored.target}->${resolvedTarget}.`,
      );
      this.updateObserved(storage, index, resolvedSource, resolvedTarget, changes);
    } else {
      this.traceMsg(`[EnsureLinkCreated] Link<T> ${index} is already correct => no-op.`);
      changes.push([stored, stored]);
    }
    return index;
  },

  /** `EnsureLinkCreated` for a definition with no address of its own. */
  ensureDoublet(storage, source, target, changes) {
    const existingId = searchUnspecified(storage, source, target);
    if (existingId !== null) {
      this.traceMsg(`[EnsureLinkCreated] Link<T> already found => ID=${existingId} => no-op.`);
      const existing = storage.getLink(existingId) ?? link(existingId, source, target);
      changes.push([existing, existing]);
      return existingId;
    }
    const resolvedSource = resolveUnspecified(source, 0);
    const resolvedTarget = resolveUnspecified(target, 0);
    this.traceMsg(
      `[EnsureLinkCreated] Creating link for (S=${resolvedSource}, T=${resolvedTarget}).`,
    );
    const created = storage.tryCreate(resolvedSource, resolvedTarget);
    changes.push([null, storage.getLink(created)]);
    return created;
  },

  /** Fills `index` in when the store lacks it, reporting the empty `(index: 0 0)`. */
  ensureAddress(storage, index, changes) {
    if (storage.exists(index)) return;
    storage.tryEnsureCreated(index);
    changes.push([null, link(index, 0, 0)]);
  },

  /** The link called `identifier`, created as the point link `(n: n n)` when missing. */
  ensureNamedPointLink(storage, identifier, changes) {
    const named = storage.getByName(name(identifier));
    if (named != null) return named;
    const created = storage.getOrCreateNamed(name(identifier));
    changes.push([null, storage.getLink(created)]);
    return created;
  },
};
