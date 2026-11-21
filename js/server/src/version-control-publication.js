// Durable, retryable attribution of one journal decision to its branch, the
// port of rust/src/version_control/publication.rs: the `__vc:pending:`
// intent written before a version-control transaction commits, and its
// completion or abandonment after a crash.
import { trim } from './text-storage-format.js';
import { VersionControlMovement } from './version-control-movement.js';
import { decodeUtf16Hex, encodeName, hex32, parseI64, parseU128Hex } from './transactions-types.js';

export const PUBLICATION_PREFIX = '__vc:pending:';

/** `PublicationIntent::encode`: `{ id, before, after, branch }` as its marker. */
export const encodePublication = (intent) =>
  `${PUBLICATION_PREFIX}2|${hex32(intent.id)}|${intent.before}|${intent.after}|${encodeName(
    intent.branch,
  )}`;

/** `PublicationIntent::decode`: the intent of a marker, or `null`. */
export function decodePublication(marker) {
  if (!marker.startsWith(PUBLICATION_PREFIX)) return null;
  const fields = marker.slice(PUBLICATION_PREFIX.length).split('|');
  if (fields.length !== 5 || fields[0] !== '2' || fields[1].length !== 32) return null;
  if (!/^[0-9a-fA-F]{32}$/.test(fields[1])) return null;
  const id = parseU128Hex(fields[1]);
  if (id === 0n || !fields.slice(2, 4).every((field) => /^[0-9]+$/.test(field))) return null;
  const before = parseI64(fields[2]);
  const after = parseI64(fields[3]);
  const branch = decodeUtf16Hex(fields[4]);
  if (before === null || after === null || branch === undefined) return null;
  if (after < before || trim(branch) === '') return null;
  return { id, before, after, branch };
}

/** The commit half of `VersionControlDecorator`. */
export class VersionControlPublication extends VersionControlMovement {
  /** Freezes the published range and makes its intent durable before the decision. */
  preparePublication() {
    const state = this.activeTransaction;
    // Freeze and fence before any metadata operation can fail.
    state.afterSequence ??= this.transactions.lastLoggedSequence();
    const intent = {
      id: state.id,
      before: state.beforeSequence,
      after: state.afterSequence,
      branch: state.branchName,
    };
    const marker = encodePublication(intent);
    if (state.intentLink === 0 || !this.branchesStore.exists(state.intentLink)) {
      state.intentLink = this.branchesStore.getByName(marker) ?? this.branchesStore.tryCreate(0, 0);
    }
    if (this.branchesStore.getName(state.intentLink) !== marker) {
      this.branchesStore.setName(state.intentLink, marker);
    }
    // Persist topology/current selection along with the intent, before the
    // journal decision can require its attribution after an abrupt exit.
    this.branchesStore.save();
    this.traceLine(
      `Prepared version publication ${hex32(intent.id)}: ${intent.before}..${intent.after} on '${intent.branch}'.`,
    );
    return { ...state };
  }

  clearPublicationIntent(link) {
    if (link !== 0) {
      this.branchesStore.removeName(link);
      if (this.branchesStore.exists(link)) this.branchesStore.delete(link);
    }
    // Even a removal that took effect before returning an error must have
    // its final flush retried before admission is released.
    this.branchesStore.save();
  }

  finishAbortedPublication(state) {
    if (state.afterSequence !== null) this.clearPublicationIntent(state.intentLink);
    this.activeTransaction = null;
  }

  /** Completes or abandons the one publication a crash interrupted: `[[link, intent]]`. */
  recoverPublications(pending) {
    if (pending.length === 0) return;
    if (pending.length !== 1) throw new Error('Multiple version publication intents.');
    const [[link, intent]] = pending;
    const branch = this.branches.get(intent.branch);
    if (
      branch === undefined ||
      this.currentBranch !== intent.branch ||
      branch.head > intent.after ||
      this.currentApplied > intent.after
    ) {
      throw new Error('Version publication intent contradicts branch metadata.');
    }
    const committed = this.transactions.isTransactionCommitted(intent.id);
    if (committed && this.transactions.isTransactionRolledBack(intent.id)) {
      throw new Error('Version publication has conflicting transaction decisions.');
    }
    const inRange = (sequence) => sequence > intent.before && sequence <= intent.after;
    const records = this.transactions.log().filter((transition) => inRange(transition.sequence));
    if (
      records.some((transition) => transition.transactionId !== intent.id) ||
      [...this.transitionBranches].some(
        ([sequence, owner]) => inRange(sequence) && owner !== intent.branch,
      ) ||
      (committed && BigInt(records.length) !== intent.after - intent.before)
    ) {
      throw new Error('Version publication intent contradicts its transition range.');
    }
    if (committed) {
      this.attributeNewTransitionsForBranch(intent.before, intent.after, intent.branch);
      this.branchesStore.save();
    }
    // The transaction constructor has already durably redone committed
    // records and undone pending records before sidecar recovery starts.
    this.clearPublicationIntent(link);
    this.traceLine(
      `Recovered ${committed ? 'committed' : 'aborted'} version publication ${hex32(intent.id)}.`,
    );
  }
}
