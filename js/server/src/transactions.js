// The optional transactions layer of rust/src/transactions/mod.rs: the
// decorator records every create, update and delete as a reversible
// transition in a sidecar log, with explicit transactions, sync commits,
// three retention policies and crash recovery (R1-R7, R10), and serves as a
// `NamedTypeLinks` store (the bridge of rust/src/named_type_links.rs).
//
// Its replay and recovery live in transactions-recovery.js, its recording,
// names and row undo in transactions-recording.js, the codecs in
// transactions-types.js and the storage adapters in transactions-storage.js
// and transactions-log.js.
import { TransactionError } from './errors.js';
import { TransactionsRecording } from './transactions-recording.js';
import { newTransactionId, nowUnixMs } from './transactions-recovery.js';
import { TransactionStorage, transitionLogStore } from './transactions-storage.js';
import {
  CommitMode,
  doublet,
  EMPTY_DOUBLET,
  I64_MAX,
  INFINITE_RETENTION,
  makeTransitionsDatabaseFilename,
  ROLLBACK_MARKER_PREFIX,
  RowUndoPolicy,
  transitionOf,
  TransitionKind,
  hex32,
} from './transactions-types.js';

export { FileTransitionLog, truncateTornTail } from './transactions-log.js';
export { parseJournal } from './transactions-recovery.js';
export { NamedTransitionLog, TransactionStorage } from './transactions-storage.js';
export * from './transactions-types.js';

/**
 * Folds one `(before, after)` callback into `observed`: repeated callbacks
 * for an address keep the first `before` and the last `after`, in the order
 * the addresses were first seen.
 */
function recordObserved(observed, before, after) {
  const key = before.index !== 0 ? before.index : after.index;
  if (key === 0) return;
  const state = doublet(after.index, after.source, after.target);
  const entry = observed.find(([index]) => index === key);
  // A null first before means the address did not exist. A later update must
  // not replace it with the intermediate created row.
  if (entry !== undefined) entry[2] = state;
  else observed.push([key, doublet(before.index, before.source, before.target), state]);
}

/** A pending transaction: an explicit one or a per-write auto-transaction. */
const pendingTransaction = (id, autoCommit) => ({
  id,
  transitions: [],
  autoCommit,
  rollbackOnly: false,
  commitAttempted: false,
  startedMs: nowUnixMs(),
  image: null,
});

/**
 * Wraps a `LinkStorage` or `NamedTypesDecorator` (`inner`) and records every
 * write in `logStore`: a `FileTransitionLog`, a links store whose names hold
 * the entries, or any object with `appendLogEntry`, `readLogEntries` and
 * `flushLog`. Runs crash recovery before returning.
 */
export class TransactionsDecorator extends TransactionsRecording {
  constructor(
    inner,
    logStore,
    retentionPolicy = INFINITE_RETENTION,
    commitMode = CommitMode.Sync,
    trace = false,
    { stderr = process.stderr } = {},
  ) {
    super();
    this.innerStore = inner;
    this.storage = new TransactionStorage(inner);
    this.rawLogStore = logStore;
    this.transitionLog = transitionLogStore(logStore);
    this.records = [];
    this.committed = new Set();
    this.rolledBack = new Set();
    this.applied = new Set();
    this.current = null;
    this.sequenceCounter = 0n;
    this.appliedSeq = 0n;
    this.retention = retentionPolicy;
    this.mode = commitMode;
    this.versionReplay = false;
    this.recoveryPending = false;
    this.reloadPending = false;
    this.trace = trace;
    this.stderr = stderr;
    this.rowUndo = RowUndoPolicy.Observed;
    this.recover();
  }

  /** Conventional sidecar filename for the transitions log. */
  static makeTransitionsDatabaseFilename(databasePath) {
    return makeTransitionsDatabaseFilename(databasePath);
  }

  retentionPolicy() {
    return this.retention;
  }

  setRetentionPolicy(policy) {
    this.retention = policy;
  }

  commitMode() {
    return this.mode;
  }

  setCommitMode(mode) {
    this.mode = mode;
  }

  appliedSequence() {
    return this.appliedSeq;
  }

  lastLoggedSequence() {
    return this.sequenceCounter;
  }

  isTransactionCommitted(id) {
    return this.committed.has(id);
  }

  isTransactionRolledBack(id) {
    return this.rolledBack.has(id);
  }

  hasOpenTransaction() {
    return this.current !== null || this.versionReplay || this.recoveryPending;
  }

  mustFinishCommit(id) {
    return (
      this.isTransactionCommitted(id) ||
      (this.current !== null && this.current.id === id && this.current.commitAttempted)
    );
  }

  /** The transitions log in sequence order. */
  log() {
    return this.records.map(transitionOf);
  }

  /** Complete retained records, including optional name payloads. */
  journal() {
    return [...this.records];
  }

  inner() {
    return this.innerStore;
  }

  logStore() {
    return this.rawLogStore;
  }

  /** Makes both the data store and the transitions log durable. */
  flush() {
    if (this.recoveryPending) return this.recover();
    this.ensureNoVersionReplay();
    if (this.current?.commitAttempted) return this.commitCurrent();
    this.storage.flush();
    this.transitionLog.flushLog();
  }

  save() {
    this.flush();
  }

  /** Whether another process has written to the wrapped store since it was read. */
  hasExternalChanges() {
    return this.storage.hasExternalChanges();
  }

  /** Re-reads the wrapped store and rebuilds the transactions state from the log. */
  reload() {
    if (this.current !== null || this.versionReplay) {
      throw new TransactionError('Cannot reload while a transaction is open.');
    }
    this.recoveryPending = true;
    this.reloadPending = true;
    this.recover();
  }

  // ----- Write API ------------------------------------------------------

  /** Ends a failed write: the owner is dropped when nothing needs undoing. */
  abandonWrite(observed, owns) {
    if (!owns) return;
    if (observed.length === 0 && (this.current === null || this.current.image === null)) {
      this.current = null;
    } else this.rollback();
  }

  create(source, target) {
    const namesBefore = this.storage.captureNames();
    const owns = this.ensureOpenTransaction();
    const observed = [];
    let id;
    let failure = null;
    try {
      id = this.storage.createLinkObserved(source, target, (before, after) =>
        recordObserved(observed, before, after),
      );
    } catch (error) {
      failure = error;
    }
    this.recordObservedTransitions(observed, owns, namesBefore);
    if (failure !== null) {
      this.fenceFailedImage();
      this.abandonWrite(observed, owns);
      throw failure;
    }
    this.completeImageObservations(owns);
    if (owns) this.commitCurrent();
    return id;
  }

  /** `update`, reporting every change the underlying store made to `observer`. */
  updateObserved(id, source, target, observer) {
    const before = this.snapshot(id);
    const namesBefore = this.storage.captureNames();
    const owns = this.ensureOpenTransaction();
    const observed = [];
    let previous;
    try {
      previous = this.storage.updateLinkObserved(id, source, target, (from, to) => {
        observer(from, to);
        recordObserved(observed, from, to);
      });
    } catch (error) {
      this.fenceFailedImage();
      // Callbacks describe changes already made, even if a later part of the
      // storage operation fails.
      this.recordObservedTransitions(observed, owns, namesBefore);
      this.abandonWrite(observed, owns);
      throw error;
    }
    if (observed.length === 0) {
      const value = this.storage.getLink(id);
      const after =
        value === null
          ? doublet(id, source, target)
          : doublet(value.index, value.source, value.target);
      this.recordTransition(TransitionKind.Update, before, after, owns, namesBefore);
    } else this.recordObservedTransitions(observed, owns, namesBefore);
    this.completeImageObservations(owns);
    if (owns) this.commitCurrent();
    return previous;
  }

  /** `delete`, reporting every change the underlying store made to `observer`. */
  deleteObserved(id, observer) {
    const before = this.snapshot(id);
    const namesBefore = this.storage.captureNames();
    const owns = this.ensureOpenTransaction();
    const observed = [];
    let deleted;
    try {
      deleted = this.storage.deleteLinkObserved(id, (from, to) => {
        observer(from, to);
        recordObserved(observed, from, to);
      });
    } catch (error) {
      this.fenceFailedImage();
      this.recordObservedTransitions(observed, owns, namesBefore);
      this.abandonWrite(observed, owns);
      throw error;
    }
    if (observed.length === 0) {
      this.recordTransition(TransitionKind.Delete, before, EMPTY_DOUBLET, owns, namesBefore);
    } else this.recordObservedTransitions(observed, owns, namesBefore);
    this.completeImageObservations(owns);
    if (owns) this.commitCurrent();
    return deleted;
  }

  /** A create followed by an update, as one transaction of two transitions. */
  createAndUpdate(source, target) {
    const owns = this.ensureOpenTransaction();
    let id;
    try {
      id = this.create(0, 0);
      this.update(id, source, target);
    } catch (error) {
      this.fenceFailedImage();
      if (owns) this.rollback();
      throw error;
    }
    if (owns) this.commitCurrent();
    return id;
  }

  exists(id) {
    return this.storage.linkExists(id);
  }

  search(source, target) {
    return this.storage.searchLink(source, target);
  }

  getOrCreate(source, target) {
    return this.storage.searchLink(source, target) ?? this.create(source, target);
  }

  ensureCreated(id) {
    if (id === 0 || this.storage.linkExists(id)) return id;
    const namesBefore = this.storage.captureNames();
    const owns = this.ensureOpenTransaction();
    const observed = [];
    let created;
    let failure = null;
    try {
      created = this.storage.ensureLinkCreatedObserved(id, (before, after) =>
        recordObserved(observed, before, after),
      );
    } catch (error) {
      failure = error;
    }
    this.recordObservedTransitions(observed, owns, namesBefore);
    if (failure !== null) {
      this.fenceFailedImage();
      if (owns) this.rollback();
      throw failure;
    }
    this.completeImageObservations(owns);
    if (owns) this.commitCurrent();
    return created;
  }

  /** The current state of `id` as a doublet, or an empty one at `id`. */
  snapshot(id) {
    const value = this.storage.getLink(id);
    return value === null ? doublet(id, 0, 0) : doublet(value.index, value.source, value.target);
  }

  ensureWriteAllowed() {
    this.ensureNoVersionReplay();
    this.ensureRecoveryComplete();
    if (this.current?.commitAttempted) {
      throw new TransactionError(
        'Retry transaction commit before further writes; its outcome may be unknown.',
      );
    }
    if (this.current?.rollbackOnly) {
      throw new TransactionError('Transaction requires rollback before further writes.');
    }
  }

  ensureOpenTransaction() {
    this.ensureWriteAllowed();
    if (this.sequenceCounter === I64_MAX) {
      throw new TransactionError('Transaction sequence space is exhausted.');
    }
    const owns = this.current === null;
    if (owns) this.current = pendingTransaction(this.allocateTransactionId(), true);
    try {
      this.prepareRowUndo();
    } catch (error) {
      this.current.rollbackOnly = true;
      if (owns) this.rollback();
      throw error;
    }
    return owns;
  }

  // ----- Transaction handle ----------------------------------------------

  /**
   * Realtime can repeat after restart while the process counter resets, so
   * the recovered decisions are consulted before assigning an id.
   */
  allocateTransactionId() {
    let collided = false;
    for (;;) {
      const id = newTransactionId();
      if (id !== 0n && !this.committed.has(id) && !this.rolledBack.has(id)) {
        if (collided) this.traceLine('Skipped previously decided transaction IDs.');
        return id;
      }
      collided = true;
    }
  }

  /** Opens an explicit transaction: `{ id, startedMs }`. */
  beginTransaction() {
    this.ensureNoVersionReplay();
    this.ensureRecoveryComplete();
    if (this.current !== null) {
      throw new TransactionError('Nested transactions are not supported.');
    }
    this.current = pendingTransaction(this.allocateTransactionId(), false);
    return { id: this.current.id, startedMs: this.current.startedMs };
  }

  /**
   * Finishes the transaction. An append or flush error can have an unknown
   * outcome: retry commit, rollback or save to finish the attempted decision.
   */
  commit() {
    if (this.recoveryPending) return this.recover();
    if (this.current === null) return;
    this.commitCurrent();
  }

  /**
   * Undoes an open transaction, or finishes its attempted commit if a
   * previous commit failed with a possibly durable decision.
   */
  rollback() {
    if (this.recoveryPending) return this.recover();
    if (this.current?.commitAttempted) return this.commitCurrent();
    const pending = this.current;
    if (pending === null) return;
    // Once undo begins, an error can leave the data partially restored. Keep
    // the abort decision until every rollback step succeeds.
    pending.rollbackOnly = true;
    if (pending.image !== null) this.restoreImage(pending.image);
    else {
      for (const transition of [...pending.transitions].reverse()) {
        this.tryRevertRecord(transition);
      }
    }
    this.storage.flush();
    this.writeMarker(`${ROLLBACK_MARKER_PREFIX}${hex32(pending.id)}`);
    this.transitionLog.flushLog();
    this.rolledBack.add(pending.id);
    this.current = null;
    this.traceLine(
      `Rolled back tx ${hex32(pending.id)} (${pending.transitions.length} transitions).`,
    );
    this.enforceRetention();
  }

  get(id) {
    return this.storage.getLink(id);
  }

  all() {
    return this.storage.allLinks();
  }

  query(index = null, source = null, target = null) {
    return this.storage.queryLinks(index, source, target);
  }

  // ----- NamedTypeLinks bridge --------------------------------------------

  reserveQueryAllocations(processor, restriction, substitution) {
    this.ensureWriteAllowed();
    return this.innerStore.reserveQueryAllocations(processor, restriction, substitution);
  }

  tryGetOrCreate(source, target) {
    return this.getOrCreate(source, target);
  }

  tryCreate(source, target) {
    return this.create(source, target);
  }

  getLink(id) {
    return this.storage.getLink(id);
  }

  allLinks() {
    return this.storage.allLinks();
  }

  getName(id) {
    return this.innerStore.getName(id);
  }

  getByName(name) {
    return this.innerStore.getByName(name);
  }
}
