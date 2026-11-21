// Recording of observed row and name changes, explicit name writes and the
// optional write-ahead undo image of the transactions decorator:
// rust/src/transactions/recording.rs, names.rs and row_undo.rs.
//
// `TransactionsRecording` sits between `TransactionsReplay`
// (transactions-recovery.js) and `TransactionsDecorator` (transactions.js).
import { StorageError, TransactionError } from './errors.js';
import { nowUnixMs, TransactionsReplay } from './transactions-recovery.js';
import {
  doublet,
  EMPTY_DOUBLET,
  hex32,
  I64_MAX,
  journalRecord,
  KIND_NAMES,
  parseImage,
  RowUndoPolicy,
  sameDoublet,
  serializeImage,
  serializeRecord,
  TRANSITION_NAME_PREFIX,
  TransitionKind,
} from './transactions-types.js';

/** The addresses of `left` and `right`, ascending and without repeats. */
const unionKeys = (left, right) =>
  [...new Set([...left.keys(), ...right.keys()])].sort((a, b) => a - b);

/** The name changes from `before` to `after` for `ids`. */
const nameChanges = (ids, before, after) =>
  ids
    .map((index) => ({ index, before: before.get(index) ?? null, after: after.get(index) ?? null }))
    .filter((change) => change.before !== change.after);

/** A name-only record of `names`. */
const nameOnlyRecord = (names) =>
  journalRecord(
    {
      transactionId: 0n,
      sequence: 0n,
      timestampMs: 0n,
      kind: TransitionKind.Update,
      before: EMPTY_DOUBLET,
      after: EMPTY_DOUBLET,
    },
    true,
    names,
  );

const sameNames = (left, right) =>
  left.size === right.size && [...left].every(([index, name]) => right.get(index) === name);

const recordIndex = (record) =>
  record.after.index !== 0 ? record.after.index : record.before.index;

export class TransactionsRecording extends TransactionsReplay {
  /**
   * Writes one transition per link a single logical write touched, the kind
   * derived from the observed pair: a cascade can delete a link during an
   * update.
   */
  recordObservedTransitions(observed, owns, namesBefore) {
    const changes = observed.map(([, before, after]) => {
      const kind =
        before.index === 0 && after.index !== 0
          ? TransitionKind.Create
          : before.index !== 0 && after.index === 0
            ? TransitionKind.Delete
            : TransitionKind.Update;
      return [kind, before, after];
    });
    this.recordTransitions(changes, owns, namesBefore);
  }

  recordTransition(kind, before, after, owns, namesBefore) {
    this.recordTransitions([[kind, before, after]], owns, namesBefore);
  }

  recordTransitions(changes, owns, namesBefore) {
    // Names may be changed by an inner decorator during a row operation.
    // Capture its actual result rather than predicting that every update
    // preserves the old name. Keep all observed undo even if this read fails.
    let namesAfter = null;
    let captureError = null;
    if (namesBefore !== null) {
      try {
        namesAfter = this.storage.captureNames();
        if (namesAfter === null) {
          throw new StorageError('Named storage lost its name-capture capability.');
        }
      } catch (error) {
        namesAfter = null;
        captureError = error;
      }
    }
    const records = changes.map(([kind, before, after]) => {
      const index = after.index !== 0 ? after.index : before.index;
      const oldName = namesBefore?.get(index) ?? null;
      // A failed capture permits undo only; fence the owner below.
      const newName =
        namesAfter !== null
          ? (namesAfter.get(index) ?? null)
          : kind !== TransitionKind.Delete
            ? oldName
            : null;
      const names =
        oldName !== null || newName !== null ? [{ index, before: oldName, after: newName }] : [];
      return journalRecord(
        { transactionId: 0n, sequence: 0n, timestampMs: 0n, kind, before, after },
        false,
        names,
      );
    });
    // An inner name reassignment can evict an owner without a row callback.
    // Prepare that undo in the SAME batch, before the first fallible append.
    if (namesBefore !== null && namesAfter !== null) {
      const observed = new Set(records.map(recordIndex));
      const names = nameChanges(
        unionKeys(namesBefore, namesAfter).filter((index) => !observed.has(index)),
        namesBefore,
        namesAfter,
      );
      if (names.length > 0) records.push(nameOnlyRecord(names));
    }
    this.recordRecords(records, owns);
    if (captureError !== null) {
      this.current.rollbackOnly = true;
      if (owns) this.rollback();
      throw captureError;
    }
  }

  recordRecords(records, owns) {
    if (this.current === null) {
      throw new TransactionError('internal: missing open transaction while recording transition');
    }
    const transactionId = this.current.id;
    const exhausted = BigInt(records.length) > I64_MAX - this.sequenceCounter;
    const prepared = [];
    for (const record of records) {
      let sequence = 0n;
      if (!exhausted) {
        this.sequenceCounter += 1n;
        sequence = this.sequenceCounter;
      }
      const transition = { ...record, transactionId, sequence, timestampMs: nowUnixMs() };
      this.current.transitions.push(transition);
      if (!exhausted) this.records.push(transition);
      prepared.push(transition);
    }
    if (exhausted) {
      // Keep the entire unnumbered undo batch only in pending memory. It must
      // never become invalid serialized journal/history entries.
      this.current.rollbackOnly = true;
      if (owns) this.rollback();
      throw new TransactionError('Transaction sequence space is exhausted.');
    }
    this.appendTransitions(prepared, owns);
  }

  appendTransitions(transitions, owns) {
    // Save every undo record before the first fallible journal write.
    for (const transition of transitions) {
      try {
        this.writeTransitionToLog(transition);
      } catch (error) {
        this.current.rollbackOnly = true;
        if (owns) this.rollback();
        throw error;
      }
      const { before: b, after: a } = transition;
      this.traceLine(
        `Recorded ${KIND_NAMES[transition.kind]} seq=${transition.sequence} tx=${hex32(
          transition.transactionId,
        )}: (${b.index},${b.source},${b.target}) -> (${a.index},${a.source},${a.target}).`,
      );
    }
  }

  writeTransitionToLog(transition) {
    this.transitionLog.appendLogEntry(`${TRANSITION_NAME_PREFIX}${serializeRecord(transition)}`);
  }

  // ----- Names (names.rs) -----------------------------------------------

  setName(index, name) {
    return this.writeName(index, name);
  }

  removeName(index) {
    this.writeName(index, null);
  }

  /** Explicit name writes are journaled before changing the logical name map. */
  writeName(index, name) {
    this.ensureWriteAllowed();
    const before = this.storage.captureNames();
    if (before === null) throw new StorageError('storage does not support names');
    const after = new Map(before);
    if (name !== null) {
      for (const [id, previous] of before) {
        if (id !== index && previous === name) after.delete(id);
      }
      after.set(index, name);
    } else after.delete(index);
    const names = nameChanges(unionKeys(before, after), before, after);
    if (names.length === 0 && this.rowUndo !== RowUndoPolicy.WriteAheadImage) {
      return this.storage.setLinkName(index, name);
    }
    const owns = this.ensureOpenTransaction();
    const changed = names.length > 0;
    if (changed) this.recordRecords([nameOnlyRecord(names)], owns);
    // A name's undo record must survive before a name sidecar can change.
    // Keep an explicit owner rollback-only when this pre-effect flush fails.
    let result;
    try {
      if (changed) this.transitionLog.flushLog();
      result = this.storage.setLinkName(index, name);
    } catch (error) {
      this.current.rollbackOnly = true;
      if (owns) this.rollback();
      throw error;
    }
    this.completeImageObservations(owns);
    if (owns) this.commitCurrent();
    return result;
  }

  replayNames(record, undo) {
    if (record.names.length === 0) return;
    // Clear every changed owner first; otherwise a moved name can erase the
    // owner that was restored earlier in address order.
    for (const change of record.names) this.storage.setLinkName(change.index, null);
    for (const change of record.names) {
      const value = undo ? change.before : change.after;
      if (value !== null) this.storage.setLinkName(change.index, value);
    }
  }

  // ----- Row undo (row_undo.rs) -----------------------------------------

  /** Selects row undo while idle; returns the decorator. */
  withRowUndo(policy) {
    this.setRowUndoPolicy(policy);
    return this;
  }

  /** Changes the policy while idle, preserving the caller's owner on rejection. */
  setRowUndoPolicy(policy) {
    if (this.hasOpenTransaction()) {
      throw new TransactionError('Select row undo before opening a transaction.');
    }
    this.rowUndo = policy;
  }

  rowUndoPolicy() {
    return this.rowUndo;
  }

  /**
   * Completes the logical journal before publishing a successful write: the
   * correction records make committed redo and version replay include
   * effects the storage did not report.
   */
  completeImageObservations(owns) {
    const owner = this.current;
    if (owner === null || owner.image === null) return;
    const image = owner.image;
    const predicted = new Map(image.rows.map((row) => [row.index, row]));
    const names = image.names === null ? null : new Map(image.names);
    for (const record of owner.transitions) {
      if (!record.nameOnly) {
        if (record.kind === TransitionKind.Delete) predicted.delete(record.before.index);
        else predicted.set(record.after.index, record.after);
      }
      if (names !== null) {
        for (const change of record.names) names.delete(change.index);
        for (const change of record.names) {
          if (change.after !== null) names.set(change.index, change.after);
        }
      }
    }
    try {
      const actual = new Map();
      for (const row of this.storage.allLinks()) {
        if (row.index === 0 || actual.has(row.index)) {
          throw new StorageError('invalid row enumeration after a write');
        }
        actual.set(row.index, row);
      }
      const changes = [];
      for (const index of unionKeys(predicted, actual)) {
        const before = predicted.get(index);
        const after = actual.get(index);
        if (before !== undefined && after !== undefined && sameDoublet(before, after)) continue;
        const kind =
          before === undefined
            ? TransitionKind.Create
            : after === undefined
              ? TransitionKind.Delete
              : TransitionKind.Update;
        const state = (value) =>
          value === undefined ? EMPTY_DOUBLET : doublet(value.index, value.source, value.target);
        changes.push([kind, state(before), state(after)]);
      }
      this.recordTransitions(changes, owns, names);
    } catch (error) {
      this.fenceFailedImage();
      if (owns && this.current !== null) this.rollback();
      throw error;
    }
  }

  fenceFailedImage() {
    if (this.current !== null && this.current.image !== null) this.current.rollbackOnly = true;
  }

  prepareRowUndo() {
    if (this.rowUndo !== RowUndoPolicy.WriteAheadImage || this.current.image !== null) return;
    const owner = this.current.id;
    const rows = [...this.storage.allLinks()]
      .sort((left, right) => left.index - right.index)
      .map((row) => doublet(row.index, row.source, row.target));
    const image = { owner, base: this.sequenceCounter, rows, names: this.storage.captureNames() };
    const payload = serializeImage(image);
    // Validate before any append, including adapters returning duplicate rows.
    parseImage(payload);
    this.current.image = image;
    this.transitionLog.appendLogEntry(`${TRANSITION_NAME_PREFIX}${payload}`);
    this.transitionLog.flushLog();
    this.traceLine(`Prepared row undo image tx=${hex32(owner)}.`);
  }

  restoreImage(image) {
    // Break references and uniqueness conflicts before deleting allocations.
    // Never bypass the logical storage API.
    for (const row of this.storage.allLinks()) {
      if (row.source !== 0 || row.target !== 0) this.storage.updateLink(row.index, 0, 0);
    }
    const wanted = new Set(image.rows.map((row) => row.index));
    const deleteUnwanted = () => {
      for (const row of this.storage.allLinks()) {
        if (!wanted.has(row.index)) this.storage.deleteLink(row.index);
      }
    };
    deleteUnwanted();
    for (const row of image.rows) this.storage.ensureLinkCreated(row.index);
    // Dense allocators may have filled holes while reserving old addresses.
    deleteUnwanted();
    for (const row of image.rows) this.storage.restoreLink(row.index, row.source, row.target);
    if (image.names !== null) {
      const current = this.storage.captureNames();
      if (current === null) {
        throw new StorageError('undo image requires a named storage adapter');
      }
      for (const index of current.keys()) this.storage.setLinkName(index, null);
      for (const [index, name] of image.names) this.storage.setLinkName(index, name);
    }
    // A decorator with unsupported replay effects must fail closed rather
    // than acknowledge a partial restoration as a successful rollback.
    const actual = [...this.storage.allLinks()].sort((left, right) => left.index - right.index);
    if (
      actual.length !== image.rows.length ||
      actual.some((row, i) => !sameDoublet(row, image.rows[i])) ||
      (image.names !== null && !sameNames(this.storage.captureNames(), image.names))
    ) {
      throw new StorageError('storage did not restore the complete undo image');
    }
  }
}
