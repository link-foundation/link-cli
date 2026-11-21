// The optional version-control layer of rust/src/version_control/mod.rs:
// time travel (`checkout`), branching (`branch`, `switchBranch`) and tagging
// (`tag`) over the transitions log of a `TransactionsDecorator`, with the
// topology kept as names in a sidecar links store, and the `NamedTypeLinks`
// bridge of rust/src/named_type_links.rs.
//
// The durable movement intent lives in version-control-movement.js and the
// publication intent in version-control-publication.js.
import { trim } from './text-storage-format.js';
import { decodeMovement, MOVEMENT_PREFIX } from './version-control-movement.js';
import {
  decodePublication,
  PUBLICATION_PREFIX,
  VersionControlPublication,
} from './version-control-publication.js';
import {
  compareBig,
  compareStrings,
  decodeUtf16Hex,
  encodeName,
  I64_MAX,
  parseI64,
  sidecarPath,
} from './transactions-types.js';

export { decodeMovement, encodeMovement, movementChecksum } from './version-control-movement.js';
export { decodePublication, encodePublication } from './version-control-publication.js';

/** Default name of the initial branch (analogous to git's `main`). */
export const DEFAULT_BRANCH_NAME = 'main';

const BRANCH_PREFIX = '__vc:branch:';
const TAG_PREFIX = '__vc:tag:';
const CURRENT_PREFIX = '__vc:current=';
const APPLIED_PREFIX = '__vc:applied=';
const TRANSITION_PREFIX = '__vc:trans:';

/** `BranchInfo`: one branch of the version-control DAG (sequences as BigInt). */
export const branchInfo = (name, parent, forkSeq, head) => ({ name, parent, forkSeq, head });

/** The `__vc:branch:` marker of `info`, its names as UTF-16 hexadecimal. */
export function encodeBranchMarker(info) {
  const parent = info.parent === null ? '-' : encodeName(info.parent);
  return `${BRANCH_PREFIX}2|${encodeName(info.name)}|${parent}|${info.forkSeq}|${info.head}`;
}

/** The text before the first `delimiter` and the text after it, or `null` without one. */
function cut(text, delimiter) {
  const index = text.indexOf(delimiter);
  return index < 0 ? null : [text.slice(0, index), text.slice(index + delimiter.length)];
}

/** A version-two branch marker body, or `null`. */
function decodeVersionTwo(rest) {
  const fields = rest.split('|');
  if (fields.length !== 5) return null;
  const name = decodeUtf16Hex(fields[1]);
  const parent = fields[2] === '-' ? null : decodeUtf16Hex(fields[2]);
  const forkSeq = parseI64(fields[3]);
  const head = parseI64(fields[4]);
  if (name === undefined || parent === undefined || forkSeq === null || head === null) {
    return null;
  }
  return branchInfo(name, parent, forkSeq, head);
}

/** The branch of a version-two or legacy `__vc:branch:` marker, or `null`. */
export function decodeBranchMarker(text) {
  if (!text.startsWith(BRANCH_PREFIX)) return null;
  const rest = text.slice(BRANCH_PREFIX.length);
  // Legacy names can start with "2|"; their delimiter suffix prevents them
  // from being mistaken for a valid version-two record.
  const decoded = rest.startsWith('2|') ? decodeVersionTwo(rest) : null;
  if (decoded !== null) return decoded;
  const name = cut(rest, ':parent=');
  const parent = name && cut(name[1], ':fork=');
  const fork = parent && cut(parent[1], ':head=');
  if (fork === null) return null;
  const forkSeq = parseI64(fork[0]);
  const head = parseI64(fork[1]);
  if (forkSeq === null || head === null) return null;
  return branchInfo(name[0], parent[0] === '' ? null : parent[0], forkSeq, head);
}

/** Conventional sidecar filename for the version-control store. */
export const makeVersionControlDatabaseFilename = (databasePath) =>
  sidecarPath(databasePath, '.versioncontrol.links');

/**
 * Adds time travel, branching and tagging over a `TransactionsDecorator`
 * (`transactions`), keeping its metadata in `branchesStore`, a
 * `NamedTypesDecorator` or another named links store. Recovers interrupted
 * publications and movements before returning.
 */
export class VersionControlDecorator extends VersionControlPublication {
  constructor(transactions, branchesStore, trace = false, { stderr = process.stderr } = {}) {
    super();
    this.transactions = transactions;
    this.branchesStore = branchesStore;
    this.branches = new Map();
    this.tags = new Map();
    this.transitionBranches = new Map();
    this.branchLinks = new Map();
    this.tagLinks = new Map();
    this.currentBranchLink = 0;
    this.appliedLink = 0;
    this.currentBranch = DEFAULT_BRANCH_NAME;
    this.currentApplied = 0n;
    this.activeTransaction = null;
    this.activeMovement = null;
    this.trace = trace;
    this.stderr = stderr;
    this.recover();
    this.ensureDefaultBranch();
  }

  static makeVersionControlDatabaseFilename(databasePath) {
    return makeVersionControlDatabaseFilename(databasePath);
  }

  currentSequence() {
    return this.currentApplied;
  }

  /** Every branch, sorted by name. */
  listBranches() {
    return [...this.branches.values()]
      .map((info) => ({ ...info }))
      .sort((left, right) => compareStrings(left.name, right.name));
  }

  /** Every tag and its sequence, sorted by name. */
  listTags() {
    return new Map([...this.tags].sort(([left], [right]) => compareStrings(left, right)));
  }

  tryGetTag(name) {
    return this.tags.get(name) ?? null;
  }

  save() {
    this.finishMovement();
    if (this.hasPendingPublication()) this.commit();
    this.transactions.save();
    this.branchesStore.save();
  }

  /** Opens an explicit version-control transaction: `{ id, startedMs }`. */
  beginTransaction() {
    if (this.activeTransaction !== null || this.activeMovement !== null) {
      throw new Error('Nested version-control transactions are not supported.');
    }
    const beforeSequence = this.transactions.lastLoggedSequence();
    const branchName = this.currentBranch;
    const handle = this.transactions.beginTransaction();
    this.activeTransaction = {
      id: handle.id,
      branchName,
      beforeSequence,
      afterSequence: null,
      intentLink: 0,
    };
    return handle;
  }

  /** The open version-control transaction, or the error of not having one. */
  requireTransaction() {
    if (this.activeTransaction === null) {
      throw new Error('No version-control transaction is open.');
    }
    return { ...this.activeTransaction };
  }

  commit() {
    const open = this.requireTransaction();
    if (this.transactions.isTransactionRolledBack(open.id)) {
      return this.finishAbortedPublication(open);
    }
    const state = this.preparePublication();
    this.transactions.commit();
    this.attributeNewTransitionsForBranch(
      state.beforeSequence,
      state.afterSequence,
      state.branchName,
    );
    this.branchesStore.save();
    this.clearPublicationIntent(state.intentLink);
    this.activeTransaction = null;
  }

  rollback() {
    // An uncertain/durable decision must finish before publication.
    if (this.hasPendingCommit()) return this.commit();
    const state = this.requireTransaction();
    let failure = null;
    try {
      this.transactions.rollback();
    } catch (error) {
      failure = error;
    }
    if (failure === null || this.transactions.isTransactionRolledBack(state.id)) {
      this.finishAbortedPublication(state);
    }
    if (failure !== null) throw failure;
  }

  // ----- Write API (attribute new transitions to the current branch) -----

  create(source, target) {
    return this.runVersionWrite(() => this.transactions.create(source, target));
  }

  updateObserved(id, source, target, observer) {
    return this.runVersionWrite(() =>
      this.transactions.updateObserved(id, source, target, observer),
    );
  }

  deleteObserved(id, observer) {
    return this.runVersionWrite(() => this.transactions.deleteObserved(id, observer));
  }

  createAndUpdate(source, target) {
    return this.runVersionWrite(() => this.transactions.createAndUpdate(source, target));
  }

  setName(id, name) {
    return this.runVersionWrite(() => this.transactions.setName(id, name));
  }

  removeName(id) {
    this.runVersionWrite(() => this.transactions.removeName(id));
  }

  exists(id) {
    return this.transactions.exists(id);
  }

  get(id) {
    return this.transactions.get(id);
  }

  all() {
    return this.transactions.all();
  }

  search(source, target) {
    return this.transactions.search(source, target);
  }

  getOrCreate(source, target) {
    this.ensureWriteAllowed();
    return this.transactions.search(source, target) ?? this.create(source, target);
  }

  /** Fallible explicit-address allocation, attributed like other writes. */
  ensureCreated(id) {
    this.ensureWriteAllowed();
    // Existing and zero addresses remain read-only no-ops: do not open a
    // transaction or write a decision marker for them.
    if (id === 0 || this.transactions.exists(id)) return id;
    return this.runVersionWrite(() => this.transactions.ensureCreated(id));
  }

  runVersionWrite(write) {
    this.ensureWriteAllowed();
    const automatic = this.activeTransaction === null;
    if (automatic) this.beginTransaction();
    let result;
    try {
      result = write();
    } catch (error) {
      // Pre-commit failures still undo automatic effects. An undo failure
      // leaves the facade owner available for retry.
      if (automatic) this.rollback();
      throw error;
    }
    // Never undo a commit error: its decision may already be durable.
    if (automatic) this.commit();
    return result;
  }

  attributeNewTransitionsForBranch(beforeSeq, afterSeq, branchName) {
    if (afterSeq <= beforeSeq) return;
    for (let sequence = beforeSeq + 1n; sequence <= afterSeq; sequence++) {
      this.transitionBranches.set(sequence, branchName);
      this.writeImmutableMarker(`${TRANSITION_PREFIX}${sequence}:branch=${branchName}`);
    }
    const info = this.branches.get(branchName);
    if (info !== undefined) {
      const updated = { ...info, head: afterSeq };
      this.branches.set(branchName, updated);
      this.updateBranchLink(updated);
    }
    if (this.currentBranch === branchName) {
      this.currentApplied = afterSeq;
      this.setApplied(afterSeq);
    }
  }

  // ----- Branching ---------------------------------------------------------

  /** Creates `name` from the current branch at `from` (a BigInt) or the applied sequence. */
  branch(name, from = null) {
    this.ensureNoOpenTransaction('branch');
    if (trim(name) === '') throw new Error('Branch name must not be empty.');
    if (this.branches.has(name)) throw new Error(`Branch '${name}' already exists.`);
    const parent = this.currentBranch;
    const forkSeq = from ?? this.currentApplied;
    if (forkSeq < 0n) throw new Error('Fork point cannot be negative.');
    if (forkSeq > 0n && !this.buildBranchSeqs(parent).includes(forkSeq)) {
      throw new Error(`Fork point ${forkSeq} is not reachable on branch '${parent}'.`);
    }
    this.createBranch(name, parent, forkSeq, forkSeq);
    this.traceLine(`Created branch '${name}' from '${this.currentBranch}' at seq ${forkSeq}.`);
  }

  switchBranch(name) {
    this.ensureNoOpenTransaction('switch_branch');
    if (!this.branches.has(name)) throw new Error(`Unknown branch '${name}'.`);
    this.startMovement(this.buildBranchSeqs(name), name);
    this.traceLine(`Switched to branch '${name}' at seq ${this.currentApplied}.`);
  }

  checkout(sequence) {
    this.ensureNoOpenTransaction('checkout');
    if (sequence < 0n) throw new Error('Sequence must be non-negative.');
    const current = this.currentBranch;
    const path = this.buildBranchSeqs(current);
    if (sequence > 0n && !path.includes(sequence)) {
      throw new Error(`Sequence ${sequence} is not reachable on branch '${current}'.`);
    }
    this.startMovement(
      path.filter((step) => step <= sequence),
      current,
    );
    this.traceLine(`Checked out seq ${sequence} on branch '${current}'.`);
  }

  tag(name, sequence = null) {
    this.ensureNoOpenTransaction('tag');
    if (trim(name) === '') throw new Error('Tag name must not be empty.');
    const seq = sequence ?? this.currentApplied;
    if (seq < 0n) throw new Error('Tag sequence must be non-negative.');
    this.tags.set(name, seq);
    this.updateTagLink(name, seq);
    this.traceLine(`Created tag '${name}' at seq ${seq}.`);
  }

  // ----- Path helpers --------------------------------------------------------

  ensureNoOpenTransaction(operation) {
    if (
      this.activeTransaction !== null ||
      this.activeMovement !== null ||
      this.transactions.hasOpenTransaction()
    ) {
      throw new Error(`${operation} is not allowed while a version-control transaction is open.`);
    }
  }

  hasPendingCommit() {
    return (
      this.activeTransaction !== null &&
      this.transactions.mustFinishCommit(this.activeTransaction.id)
    );
  }

  hasPendingPublication() {
    return (
      (this.activeTransaction !== null && this.activeTransaction.afterSequence !== null) ||
      this.hasPendingCommit()
    );
  }

  ensureWriteAllowed() {
    if (this.hasPendingPublication() || this.activeMovement !== null) {
      throw new Error(
        'Version publication is incomplete; retry commit, rollback or save before writing.',
      );
    }
    this.transactions.ensureWriteAllowed();
  }

  /** The sequences visible on `branchName`, root branch first. */
  buildBranchSeqs(branchName) {
    // Walk leaf to root with the minimum descendant fork cutoff. A branch's
    // head limits its own records, not the records inherited from its parent.
    const positions = new Map();
    const chain = [];
    let current = branchName;
    let cutoff = I64_MAX;
    while (current !== null) {
      const info = this.branches.get(current);
      if (info === undefined || positions.has(current)) break;
      positions.set(current, chain.length);
      chain.push({ limit: cutoff < info.head ? cutoff : info.head, records: [] });
      if (info.forkSeq < cutoff) cutoff = info.forkSeq;
      current = info.parent;
    }
    // Scan all attribution once rather than once per ancestor in sequence
    // order; then emit root first.
    for (const [sequence, owner] of this.sortedTransitionBranches()) {
      const position = positions.get(owner);
      if (position === undefined) continue;
      const entry = chain[position];
      if (sequence <= entry.limit) entry.records.push(sequence);
    }
    return chain.reverse().flatMap((entry) => entry.records);
  }

  sortedTransitionBranches() {
    return [...this.transitionBranches].sort(([left], [right]) => compareBig(left, right));
  }

  // ----- Persistence helpers -------------------------------------------------

  ensureDefaultBranch() {
    if (!this.branches.has(DEFAULT_BRANCH_NAME)) {
      let existing = 0n;
      // Aborted and recovered-pending records have sequence numbers, but
      // only a known committed decision makes them versions.
      for (const transition of this.transactions.log()) {
        if (!this.transactions.isTransactionCommitted(transition.transactionId)) continue;
        const sequence = transition.sequence;
        if (sequence > existing) existing = sequence;
        if (!this.transitionBranches.has(sequence)) {
          this.transitionBranches.set(sequence, DEFAULT_BRANCH_NAME);
          this.writeImmutableMarker(
            `${TRANSITION_PREFIX}${sequence}:branch=${DEFAULT_BRANCH_NAME}`,
          );
        }
      }
      this.createBranch(DEFAULT_BRANCH_NAME, null, 0n, existing);
      this.currentBranch = DEFAULT_BRANCH_NAME;
      this.currentApplied = existing;
      this.setCurrentBranch(DEFAULT_BRANCH_NAME);
      this.setApplied(existing);
    } else if (this.currentBranchLink === 0) {
      this.setCurrentBranch(this.currentBranch);
    }
  }

  createBranch(name, parent, forkSeq, head) {
    const info = branchInfo(name, parent, forkSeq, head);
    this.branches.set(name, info);
    this.updateBranchLink(info);
  }

  /** The metadata link of `key` in `links`, created on first use. */
  metadataLink(links, key) {
    let link = links.get(key);
    if (link === undefined) {
      link = this.branchesStore.tryCreate(0, 0);
      links.set(key, link);
    }
    return link;
  }

  updateBranchLink(info) {
    const link = this.metadataLink(this.branchLinks, info.name);
    this.branchesStore.setName(link, encodeBranchMarker(info));
  }

  updateTagLink(name, seq) {
    const link = this.metadataLink(this.tagLinks, name);
    this.branchesStore.setName(link, `${TAG_PREFIX}${name}=${seq}`);
  }

  setCurrentBranch(name) {
    this.currentBranch = name;
    if (this.currentBranchLink === 0) this.currentBranchLink = this.branchesStore.tryCreate(0, 0);
    this.branchesStore.setName(this.currentBranchLink, `${CURRENT_PREFIX}${name}`);
  }

  setApplied(seq) {
    if (this.appliedLink === 0) this.appliedLink = this.branchesStore.tryCreate(0, 0);
    this.branchesStore.setName(this.appliedLink, `${APPLIED_PREFIX}${seq}`);
  }

  writeImmutableMarker(name) {
    if (this.branchesStore.getByName(name) !== null) return;
    const link = this.branchesStore.tryCreate(0, 0);
    this.branchesStore.setName(link, name);
  }

  /** Rebuilds the state from the metadata store, finishing interrupted intents. */
  recover() {
    this.ensureNoOpenTransaction('recover');
    const uncommitted = new Set(
      this.transactions
        .log()
        .filter((transition) => !this.transactions.isTransactionCommitted(transition.transactionId))
        .map((transition) => transition.sequence),
    );
    this.branches.clear();
    this.tags.clear();
    this.transitionBranches.clear();
    this.branchLinks.clear();
    this.tagLinks.clear();
    this.currentBranch = DEFAULT_BRANCH_NAME;
    this.currentBranchLink = 0;
    this.appliedLink = 0;
    this.currentApplied = 0n;

    const pending = [];
    const movements = [];
    let ambiguous = false;
    for (const { index } of this.branchesStore.allLinks()) {
      const name = this.branchesStore.getName(index);
      if (name === null) continue;
      if (name.startsWith(MOVEMENT_PREFIX)) {
        const intent = decodeMovement(name);
        if (intent === null) throw new Error('Invalid version movement intent.');
        movements.push([index, intent]);
      } else if (name.startsWith(PUBLICATION_PREFIX)) {
        const intent = decodePublication(name);
        if (intent === null) throw new Error('Invalid version publication intent.');
        pending.push([index, intent]);
      } else if (name.startsWith(BRANCH_PREFIX)) {
        const info = decodeBranchMarker(name);
        if (info === null) ambiguous = true;
        else {
          ambiguous ||= this.branches.has(info.name);
          this.branches.set(info.name, info);
          this.branchLinks.set(info.name, index);
        }
      } else if (name.startsWith(CURRENT_PREFIX)) {
        ambiguous ||= this.currentBranchLink !== 0;
        this.currentBranch = name.slice(CURRENT_PREFIX.length);
        this.currentBranchLink = index;
      } else if (name.startsWith(APPLIED_PREFIX)) {
        const seq = parseI64(name.slice(APPLIED_PREFIX.length));
        if (seq === null) ambiguous = true;
        else {
          ambiguous ||= this.appliedLink !== 0 || seq < 0n;
          this.currentApplied = seq;
          this.appliedLink = index;
        }
      } else if (name.startsWith(TAG_PREFIX)) {
        this.recoverTag(name.slice(TAG_PREFIX.length), index);
      } else if (name.startsWith(TRANSITION_PREFIX)) {
        const attribution = cut(name.slice(TRANSITION_PREFIX.length), ':branch=');
        const seq = attribution && parseI64(attribution[0]);
        if (seq === null) ambiguous = true;
        else {
          if (uncommitted.has(seq)) {
            throw new Error(`Version metadata references uncommitted transition ${seq}.`);
          }
          ambiguous ||= seq <= 0n || this.transitionBranches.has(seq);
          this.transitionBranches.set(seq, attribution[1]);
        }
      }
    }
    if (pending.length > 0 && movements.length > 0) {
      throw new Error('Version publication and movement intents overlap.');
    }
    if (
      movements.length > 0 &&
      (ambiguous || this.currentBranchLink === 0 || this.appliedLink === 0)
    ) {
      throw new Error('Ambiguous version movement metadata.');
    }
    this.recoverPublications(pending);
    this.recoverMovements(movements);
  }

  /** A `name=seq` tag body; malformed ones are ignored. */
  recoverTag(rest, link) {
    const split = rest.lastIndexOf('=');
    if (split <= 0) return;
    const seq = parseI64(rest.slice(split + 1));
    if (seq === null) return;
    const name = rest.slice(0, split);
    this.tags.set(name, seq);
    this.tagLinks.set(name, link);
  }

  traceLine(message) {
    if (this.trace) this.stderr.write(`[VersionControl] ${message}\n`);
  }

  // ----- NamedTypeLinks bridge -----------------------------------------------

  reserveQueryAllocations(processor, restriction, substitution) {
    this.ensureWriteAllowed();
    return this.transactions.reserveQueryAllocations(processor, restriction, substitution);
  }

  tryGetOrCreate(source, target) {
    return this.getOrCreate(source, target);
  }

  tryCreate(source, target) {
    return this.create(source, target);
  }

  getLink(id) {
    return this.transactions.getLink(id);
  }

  allLinks() {
    return this.transactions.allLinks();
  }

  getName(id) {
    return this.transactions.inner().getName(id);
  }

  getByName(name) {
    return this.transactions.inner().getByName(name);
  }
}
