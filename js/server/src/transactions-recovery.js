// Transition replay, crash recovery, log retention, the commit checkpoint and
// the exclusive unlogged replay of the transactions decorator:
// rust/src/transactions/recovery.rs, journal_validation.rs, commit.rs and
// version_replay.rs.
//
// `TransactionsReplay` is the base of `TransactionsDecorator`
// (transactions.js); its methods work on the decorator's state, as the Rust
// `impl` blocks of those files do.
import { closeSync, mkdirSync, openSync, writeSync } from 'node:fs';

import { ioErrorText } from './error-report.js';
import { InvalidFormatError, StorageError, TransactionError } from './errors.js';
import { NamedTypeLinks } from './named-type-links.js';
import {
  APPLIED_MARKER_PREFIX,
  COMMIT_MARKER_PREFIX,
  compareBig,
  hex32,
  parseI64,
  parseImage,
  parseRecord,
  parseU128Hex,
  ROLLBACK_MARKER_PREFIX,
  sameRecord,
  serializeImage,
  serializeRecord,
  TRANSITION_NAME_PREFIX,
  TransitionKind,
  validateRecord,
} from './transactions-types.js';
import { io } from './transactions-storage.js';

let transactionCounter = 0n;
const U64_MASK = (1n << 64n) - 1n;

/** Milliseconds since the Unix epoch, as an `i64`. */
export const nowUnixMs = () => BigInt(Date.now());

/** A candidate transaction id: the time in the high and a process counter in the low 64 bits. */
export function newTransactionId() {
  const count = transactionCounter;
  transactionCounter = (transactionCounter + 1n) & U64_MASK;
  return (nowUnixMs() << 64n) | count;
}

function parseOwner(value, kind) {
  const owner = parseU128Hex(value);
  if (owner === null || owner === 0n) throw new InvalidFormatError(`invalid ${kind} marker`);
  return owner;
}

const contradiction = (owner) =>
  new InvalidFormatError(`transaction ${hex32(owner)} has both commit and rollback decisions`);

/**
 * `RecoveryJournal::parse`: the validated journal, without changing live
 * history or storage. Entries matching no prefix are ignored.
 */
export function parseJournal(entries) {
  const transitions = new Map();
  const committed = new Set();
  const rolledBack = new Set();
  const applied = new Set();
  let sequence = 0n;
  let appliedSequence = 0n;
  const images = [];
  const imageOwners = new Map();
  for (const entry of entries) {
    if (entry.startsWith(TRANSITION_NAME_PREFIX)) {
      const payload = entry.slice(TRANSITION_NAME_PREFIX.length);
      if (payload.startsWith('v3|')) {
        const image = parseImage(payload);
        const index = imageOwners.get(image.owner);
        if (index !== undefined) {
          if (serializeImage(images[index]) !== serializeImage(image)) {
            throw new InvalidFormatError('conflicting write-ahead undo images');
          }
        } else {
          if (image.base > sequence) sequence = image.base;
          imageOwners.set(image.owner, images.length);
          images.push(image);
        }
        continue;
      }
      // Compare parsed values: identical retries may retain older lexical
      // forms and extra fields accepted by the public codec.
      const transition = parseRecord(payload);
      if (transition.transactionId === 0n || transition.sequence <= 0n) {
        throw new InvalidFormatError(
          'transaction records require a nonzero owner and positive sequence',
        );
      }
      const previous = transitions.get(transition.sequence);
      if (previous !== undefined && !sameRecord(previous, transition)) {
        throw new InvalidFormatError(
          `conflicting transaction records for sequence ${transition.sequence}`,
        );
      }
      if (transition.sequence > sequence) sequence = transition.sequence;
      transitions.set(transition.sequence, transition);
    } else if (entry.startsWith(COMMIT_MARKER_PREFIX)) {
      const owner = parseOwner(entry.slice(COMMIT_MARKER_PREFIX.length), 'commit');
      if (rolledBack.has(owner)) throw contradiction(owner);
      committed.add(owner);
    } else if (entry.startsWith(ROLLBACK_MARKER_PREFIX)) {
      const owner = parseOwner(entry.slice(ROLLBACK_MARKER_PREFIX.length), 'rollback');
      if (committed.has(owner)) throw contradiction(owner);
      rolledBack.add(owner);
    } else if (entry.startsWith(APPLIED_MARKER_PREFIX)) {
      const seq = parseI64(entry.slice(APPLIED_MARKER_PREFIX.length));
      if (seq === null) throw new InvalidFormatError('invalid applied marker');
      if (seq <= 0n) throw new InvalidFormatError('nonpositive applied sequence');
      applied.add(seq);
      if (seq > appliedSequence) appliedSequence = seq;
      // Checkpoints reserve sequences even after payload retention.
      if (seq > sequence) sequence = seq;
    }
  }
  const log = [...transitions.values()].sort((left, right) =>
    compareBig(left.sequence, right.sequence),
  );
  const firstSequences = new Map();
  if (images.length > 0) {
    for (const record of log) {
      if (!firstSequences.has(record.transactionId)) {
        firstSequences.set(record.transactionId, record.sequence);
      }
    }
  }
  const pendingBases = new Set();
  for (const image of images) {
    const first = firstSequences.get(image.owner);
    if (first !== undefined && first <= image.base) {
      throw new InvalidFormatError("undo image follows its owner's writes");
    }
    if (!committed.has(image.owner) && !rolledBack.has(image.owner)) {
      if (pendingBases.has(image.base)) {
        throw new InvalidFormatError('ambiguous pending undo images at the same base');
      }
      pendingBases.add(image.base);
    }
  }
  return { log, committed, rolledBack, applied, sequence, appliedSequence, images };
}

/** `Path::join` of a file name onto a directory. */
const joinPath = (directory, name) =>
  directory.endsWith('/') ? `${directory}${name}` : `${directory}/${name}`;

/** Replay, recovery, retention and the commit checkpoint of the decorator. */
export class TransactionsReplay extends NamedTypeLinks {
  /** Writes a `[Transactions]` diagnostic line when tracing. */
  traceLine(text) {
    if (this.trace) this.stderr.write(`[Transactions] ${text}\n`);
  }

  writeMarker(name) {
    this.transitionLog.appendLogEntry(name);
  }

  /** Applies a single transition without writing a new log entry. */
  applyTransition(transition) {
    this.ensureReplayAllowed();
    this.tryApplyTransition(transition);
  }

  /** Reverts one transition, reporting storage errors to the caller. */
  revertTransition(transition) {
    this.ensureReplayAllowed();
    this.tryRevertTransition(transition);
  }

  /** Replays a complete record, including names, without logging another write. */
  applyRecord(record) {
    this.ensureReplayAllowed();
    this.validateRecordBackend(record);
    this.tryApplyRecord(record, false);
  }

  /** Undoes a complete record, including names, without logging another write. */
  revertRecord(record) {
    this.ensureReplayAllowed();
    this.validateRecordBackend(record);
    this.tryRevertRecord(record);
  }

  validateRecordBackend(record) {
    validateRecord(record);
    if (record.names.length > 0 && this.storage.captureNames() === null) {
      throw new StorageError('journal requires a named storage adapter');
    }
  }

  tryApplyRecord(record, recordApplied) {
    if (!record.nameOnly) this.tryApplyTransition(record);
    this.replayNames(record, false);
    if (recordApplied) {
      this.storage.flush();
      this.markApplied(record);
      this.transitionLog.flushLog();
    }
  }

  tryRevertRecord(record) {
    if (!record.nameOnly) this.tryRevertTransition(record);
    this.replayNames(record, true);
  }

  ensureReplayAllowed() {
    if (this.hasOpenTransaction()) {
      throw new TransactionError(
        'Complete the current transaction or its cleanup before unlogged replay.',
      );
    }
  }

  tryApplyTransition(transition) {
    const { before, after } = transition;
    switch (transition.kind) {
      case TransitionKind.Create:
        if (after.index !== 0) {
          this.storage.ensureLinkCreated(after.index);
          this.storage.restoreLink(after.index, after.source, after.target);
        }
        break;
      case TransitionKind.Update:
        if (after.index !== 0 && this.storage.linkExists(after.index)) {
          this.storage.restoreLink(after.index, after.source, after.target);
        }
        break;
      default:
        if (before.index !== 0 && this.storage.linkExists(before.index)) {
          this.storage.deleteLink(before.index);
        }
    }
  }

  tryRevertTransition(transition) {
    const { before, after } = transition;
    switch (transition.kind) {
      case TransitionKind.Create:
        if (after.index !== 0 && this.storage.linkExists(after.index)) {
          this.storage.deleteLink(after.index);
        }
        break;
      case TransitionKind.Update:
        if (before.index !== 0 && this.storage.linkExists(before.index)) {
          this.storage.restoreLink(before.index, before.source, before.target);
        }
        break;
      default:
        if (before.index !== 0) {
          // Restoring a higher mapped address can allocate this address as an
          // empty intermediate slot. Existence alone does not mean its
          // original contents were restored.
          this.storage.ensureLinkCreated(before.index);
          this.storage.restoreLink(before.index, before.source, before.target);
        }
    }
  }

  markApplied(transition) {
    if (!this.applied.has(transition.sequence)) {
      this.writeMarker(`${APPLIED_MARKER_PREFIX}${transition.sequence}`);
      this.applied.add(transition.sequence);
      if (transition.sequence > this.appliedSeq) this.appliedSeq = transition.sequence;
    }
  }

  // ----- Recovery -------------------------------------------------------

  ensureRecoveryComplete() {
    if (this.recoveryPending) {
      throw new TransactionError('Retry recovery before further transaction writes or replay.');
    }
  }

  /**
   * Validates the sidecar journal before rebuilding history and replaying
   * it. Unrelated names are ignored; malformed or contradictory transaction
   * records fail before rows or live indexes change. A failure retains write
   * admission until recovery succeeds.
   */
  recover() {
    if (this.current !== null || this.versionReplay) {
      throw new TransactionError('Cannot recover while a transaction is open.');
    }
    this.recoveryPending = true;
    try {
      this.recoverJournal();
    } catch (error) {
      this.traceLine(`Recovery failed; admission retained: ${error.message}.`);
      throw error;
    }
    this.recoveryPending = false;
  }

  recoverJournal() {
    const journal = parseJournal(this.transitionLog.readLogEntries());
    // Parsing already validates every payload. Probe optional adapter
    // capability once, avoiding a full sidecar scan per retained record.
    const named = journal.log.find((record) => record.names.length > 0);
    if (named !== undefined) this.validateRecordBackend(named);
    if (
      journal.images.some((image) => image.names !== null) &&
      this.storage.captureNames() === null
    ) {
      throw new StorageError('undo image requires a named storage adapter');
    }
    if (this.reloadPending) {
      this.storage.reload();
      this.reloadPending = false;
    }
    this.records = journal.log;
    this.committed = journal.committed;
    this.rolledBack = journal.rolledBack;
    this.applied = journal.applied;
    this.sequenceCounter = journal.sequence;
    this.appliedSeq = journal.appliedSequence;

    // An older failed checkpoint can precede a later applied commit. Redo the
    // whole committed suffix in order, including already-applied rows.
    const snapshot = [...this.records];
    const decided = (id) => this.committed.has(id) || this.rolledBack.has(id);
    let replayFrom = snapshot.findIndex(
      (record) => this.committed.has(record.transactionId) && !this.applied.has(record.sequence),
    );
    if (replayFrom === -1) replayFrom = snapshot.length;
    const pendingImages = journal.images.filter((image) => !decided(image.owner));
    if (pendingImages.length > 0) {
      const base = pendingImages.reduce(
        (min, image) => (image.base < min ? image.base : min),
        pendingImages[0].base,
      );
      const point = snapshot.filter((record) => record.sequence <= base).length;
      replayFrom = Math.min(replayFrom, point);
    }
    const pendingIds = [];
    const pendingSeen = new Set();
    const pend = (id) => {
      if (!pendingSeen.has(id)) {
        pendingSeen.add(id);
        pendingIds.push(id);
      }
    };
    if (pendingImages.length > 0) {
      // Undo newest effects first, interleaving legacy records and complete
      // images by their base sequence. The parser rejects ambiguous equal-base
      // owners, independent of physical order.
      const imaged = new Set(pendingImages.map((image) => image.owner));
      const images = [...pendingImages].sort((left, right) => compareBig(left.base, right.base));
      const records = snapshot
        .filter((record) => !decided(record.transactionId) && !imaged.has(record.transactionId))
        .reverse();
      let next = 0;
      for (const image of images.reverse()) {
        while (next < records.length && records[next].sequence > image.base) {
          const record = records[next++];
          this.tryRevertRecord(record);
          pend(record.transactionId);
        }
        this.restoreImage(image);
        pend(image.owner);
      }
      for (const record of records.slice(next)) {
        this.tryRevertRecord(record);
        pend(record.transactionId);
      }
    }
    for (const transition of snapshot.slice(replayFrom)) {
      if (this.committed.has(transition.transactionId)) this.tryApplyRecord(transition, false);
    }
    // Auto-rollback transitions written but never committed and never rolled back (R10).
    if (pendingImages.length === 0) {
      for (const transition of [...snapshot].reverse()) {
        if (decided(transition.transactionId)) continue;
        this.tryRevertRecord(transition);
        pend(transition.transactionId);
      }
    }
    this.storage.flush();
    // The gap must remain a restart anchor until the complete final state
    // (including undo) is durable, even if a later marker append fails.
    for (const transition of snapshot.slice(replayFrom)) {
      if (this.committed.has(transition.transactionId)) this.markApplied(transition);
    }
    for (const id of pendingIds) {
      this.rolledBack.add(id);
      this.writeMarker(`${ROLLBACK_MARKER_PREFIX}${hex32(id)}`);
    }
    this.transitionLog.flushLog();
  }

  enforceRetention() {
    const policy = this.retention;
    if (policy.kind === 'sized') this.enforceSized(policy.maxTransitions);
    else if (policy.kind === 'chunked') {
      this.enforceChunked(policy.chunkSize, policy.archiveDirectory);
    }
  }

  enforceSized(maxTransitions) {
    if (maxTransitions === 0n) return;
    while (BigInt(this.records.length) > maxTransitions) {
      const head = this.records[0];
      if (!this.rolledBack.has(head.transactionId) && !this.applied.has(head.sequence)) {
        // A pending transaction is not reclaimable.
        if (!this.committed.has(head.transactionId)) break;
        // R7: never drop an un-applied transition. The checkpoint marks it
        // applied or throws.
        this.tryApplyRecord(head, true);
      }
      this.records.shift();
      this.traceLine(`Dropped completed transition seq=${head.sequence} per sized retention.`);
    }
  }

  enforceChunked(chunkSize, archiveDirectory) {
    if (chunkSize === 0n || BigInt(this.records.length) < chunkSize) return;
    const chunk = this.records.slice(0, Number(chunkSize));
    // Rollback has already durably undone these writes. Reapplying them would
    // resurrect aborted data; archives contain committed changes only.
    const archived = chunk.filter((transition) => !this.rolledBack.has(transition.transactionId));
    for (const transition of archived) {
      if (!this.applied.has(transition.sequence)) {
        // Never apply or discard a pending chunk.
        if (!this.committed.has(transition.transactionId)) return;
        // Never drop un-applied: the checkpoint marks it applied or throws.
        this.tryApplyRecord(transition, true);
      }
    }
    if (archived.length === 0) {
      this.records.splice(0, chunk.length);
      return;
    }
    try {
      mkdirSync(archiveDirectory, { recursive: true });
    } catch (error) {
      throw new StorageError(
        `failed to create archive dir ${archiveDirectory}: ${ioErrorText(error)}`,
      );
    }
    const timestamp = nowUnixMs();
    // Independent journals may share this directory and restart their
    // counters at the same realtime. Claim the path in the open itself.
    let fd;
    let path;
    for (;;) {
      path = joinPath(
        archiveDirectory,
        `transitions-chunk-${timestamp}-${hex32(newTransactionId())}.log`,
      );
      try {
        fd = openSync(path, 'wx');
        break;
      } catch (error) {
        if (error.code !== 'EEXIST') {
          throw new StorageError(`failed to create archive file ${path}: ${ioErrorText(error)}`);
        }
        this.traceLine(`Archive entry already owned: ${path}.`);
      }
    }
    try {
      for (const transition of archived) {
        io(() => writeSync(fd, `${serializeRecord(transition)}\n`));
      }
    } finally {
      closeSync(fd);
    }
    this.traceLine(`Archived ${archived.length} transitions to ${path}.`);
    this.records.splice(0, chunk.length);
  }

  // ----- Commit checkpoint (commit.rs) ----------------------------------

  commitCurrent() {
    const current = this.current;
    if (current === null) return;
    if (current.rollbackOnly) {
      throw new TransactionError(
        'Cannot commit a rollback-only transaction; rollback is required.',
      );
    }
    const id = current.id;
    if (!this.committed.has(id)) {
      const retry = current.commitAttempted;
      current.commitAttempted = true;
      const marker = `${COMMIT_MARKER_PREFIX}${hex32(id)}`;
      // Only uncertain retries read the journal. An append can persist and
      // then fail; avoid another physical decision for the same ID.
      if (!retry || !this.transitionLog.readLogEntries().includes(marker)) {
        this.writeMarker(marker);
      }
      // A links-backed log buffers appends; a commit is durable only after
      // its explicit flush, even if the file log syncs appends.
      this.transitionLog.flushLog();
      this.committed.add(id);
    }
    this.current = null;
    try {
      this.finishCommitCheckpoint(current.transitions);
    } catch (error) {
      this.current = current;
      throw error;
    }
    this.traceLine(
      `Committed tx ${hex32(current.id)} (mode=${this.mode}, transitions=${current.transitions.length}).`,
    );
  }

  finishCommitCheckpoint(transitions) {
    // Never persist an applied marker before its data is durable.
    this.storage.flush();
    for (const transition of transitions) this.markApplied(transition);
    this.transitionLog.flushLog();
    this.enforceRetention();
  }

  // ----- Unlogged replay owned by version control (version_replay.rs) ----

  ensureNoVersionReplay() {
    if (this.versionReplay) {
      throw new TransactionError(
        'Complete version movement before accessing transaction writes or persistence.',
      );
    }
  }

  beginVersionReplay() {
    this.ensureRecoveryComplete();
    if (this.current !== null) {
      throw new TransactionError('A transaction still owns the data store.');
    }
    this.versionReplay = true;
    // Commits checkpoint synchronously in both modes. Flush again before a
    // durable movement can rely on those applied markers.
    this.flushVersionReplay();
  }

  replayVersionTransition(transition, undo) {
    this.requireVersionReplay();
    if (undo) this.tryRevertRecord(transition);
    else this.tryApplyRecord(transition, false);
  }

  flushVersionReplay() {
    this.requireVersionReplay();
    this.storage.flush();
    this.transitionLog.flushLog();
  }

  finishVersionReplay() {
    this.versionReplay = false;
  }

  requireVersionReplay() {
    if (!this.versionReplay) {
      throw new TransactionError('Version movement does not own replay.');
    }
  }
}
