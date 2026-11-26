// Bounds the durable journal by the retention policy (issue #195):
// rust/src/transactions/compaction.rs.
//
// Retention trims the in-memory history after each commit or rollback.
// Without compaction the journal on disk kept every entry ever written, so
// each process start re-read the complete history and every write grew a
// file that no policy could shrink. When the log supports it, the journal is
// rewritten to the entries that still matter once it has grown to about twice
// that size, so the rewrite is amortized over many appends.
//
// A compacted journal keeps, in order:
//
// 1. a `__transactions:sequence:<n>` watermark, so sequence numbers (which
//    version-control tags and branches reference) are never reused;
// 2. an applied marker for the highest applied sequence;
// 3. every retained record, unchanged;
// 4. the commit or rollback decision of every retained record's owner and
//    the newest decision, which version-control publication recovery reads;
// 5. the applied markers of retained records.
//
// Undo images belong to undecided owners only while a transaction is open or
// recovery is pending; compaction never runs then, so decided images are
// dropped. A journal with any undecided record is left untouched.
import {
  APPLIED_MARKER_PREFIX,
  COMMIT_MARKER_PREFIX,
  hex32,
  ROLLBACK_MARKER_PREFIX,
  SEQUENCE_MARKER_PREFIX,
  serializeRecord,
  TRANSITION_NAME_PREFIX,
} from './transactions-types.js';

/**
 * Entries a journal may grow by beyond twice its live size before it is
 * compacted, so small retention windows do not rewrite on every commit.
 */
export const COMPACTION_SLACK = 256;

/** The retained journal of `tx`, or `null` while a retained owner is undecided. */
export function compactedJournal(tx) {
  const decisions = [];
  const seen = new Set();
  for (const { transactionId: owner } of tx.records) {
    if (seen.has(owner)) continue;
    seen.add(owner);
    let prefix;
    if (tx.committed.has(owner)) prefix = COMMIT_MARKER_PREFIX;
    else if (tx.rolledBack.has(owner)) prefix = ROLLBACK_MARKER_PREFIX;
    else return null;
    decisions.push(`${prefix}${hex32(owner)}`);
  }
  if (tx.lastDecision !== null && !decisions.includes(tx.lastDecision)) {
    decisions.push(tx.lastDecision);
  }
  const entries = [];
  if (tx.sequenceCounter > 0n) entries.push(`${SEQUENCE_MARKER_PREFIX}${tx.sequenceCounter}`);
  const appliedRetained = tx.records.some((record) => record.sequence === tx.appliedSeq);
  if (tx.appliedSeq > 0n && !appliedRetained) {
    entries.push(`${APPLIED_MARKER_PREFIX}${tx.appliedSeq}`);
  }
  for (const record of tx.records) {
    entries.push(`${TRANSITION_NAME_PREFIX}${serializeRecord(record)}`);
  }
  entries.push(...decisions);
  for (const record of tx.records) {
    if (tx.applied.has(record.sequence)) {
      entries.push(`${APPLIED_MARKER_PREFIX}${record.sequence}`);
    }
  }
  return entries;
}

/**
 * `compact_journal_if_needed`: rewrites the durable journal of `tx` to its
 * retained entries when it has outgrown them.
 */
export function compactJournalIfNeeded(tx) {
  const log = tx.transitionLog;
  if (
    tx.retention.kind === 'infinite' ||
    typeof log.supportsCompaction !== 'function' ||
    !log.supportsCompaction() ||
    tx.hasOpenTransaction()
  ) {
    return;
  }
  // Each retained record needs at most itself, a decision and an applied
  // marker; the estimate avoids building entries every commit.
  const live = tx.records.length * 3 + 3;
  if (tx.journalEntries <= live * 2 + COMPACTION_SLACK) return;
  const entries = compactedJournal(tx);
  if (entries === null) return;
  // Appends are durable before their replacement is published.
  log.flushLog();
  const before = tx.journalEntries;
  log.compactLogEntries(entries);
  tx.journalEntries = entries.length;
  tx.traceLine(`Compacted journal from ${before} to ${entries.length} entries.`);
}
