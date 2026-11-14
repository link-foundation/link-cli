// Durable ownership of an unlogged checkout or branch switch, the port of
// rust/src/version_control/movement.rs: the checksummed `__vc:move:` intent
// that names both endpoints and every record to undo and redo, written
// before any row moves, and its replay after a crash.
import { NamedTypeLinks } from './named-type-links.js';
import { trim } from './text-storage-format.js';
import {
  decodeUtf16Hex,
  encodeName,
  MAX_RECORD_MS,
  MIN_RECORD_MS,
  parseI64,
  parseRecord,
  parseU64Hex,
  sameRecord,
  serializeRecord,
  TransitionKind,
} from './transactions-types.js';

export const MOVEMENT_PREFIX = '__vc:move:';

const FNV_OFFSET = 0xcbf29ce484222325n;
const FNV_PRIME = 0x100000001b3n;
const U64_MASK = (1n << 64n) - 1n;

/** FNV-1a 64 over the UTF-8 bytes of `body`. */
export function movementChecksum(body) {
  let hash = FNV_OFFSET;
  for (const byte of new TextEncoder().encode(body)) {
    hash = ((hash ^ BigInt(byte)) * FNV_PRIME) & U64_MASK;
  }
  return hash;
}

const encodeRecords = (records) =>
  records.map((record) => encodeName(serializeRecord(record))).join(',');

/**
 * `MovementIntent::encode`: `{ fromBranch, from, toBranch, to, undo, redo }`
 * (sequences as BigInt, undo and redo as journal records) as its marker.
 */
export function encodeMovement(intent) {
  const body = [
    '1',
    encodeName(intent.fromBranch),
    intent.from,
    encodeName(intent.toBranch),
    intent.to,
    encodeRecords(intent.undo),
    encodeRecords(intent.redo),
  ].join('|');
  return `${MOVEMENT_PREFIX}${body}|${movementChecksum(body).toString(16).padStart(16, '0')}`;
}

/** A canonical record a movement may replay, or `null`. */
function decodeMovementRecord(encoded) {
  const text = decodeUtf16Hex(encoded);
  if (text === undefined) return null;
  let record;
  try {
    record = parseRecord(text);
  } catch {
    return null;
  }
  if (
    serializeRecord(record) !== text ||
    record.transactionId === 0n ||
    record.sequence <= 0n ||
    record.timestampMs < MIN_RECORD_MS ||
    record.timestampMs > MAX_RECORD_MS
  ) {
    return null;
  }
  const { before, after } = record;
  const valid =
    record.kind === TransitionKind.Create
      ? before.index === 0 && after.index !== 0
      : record.kind === TransitionKind.Update
        ? record.nameOnly || (before.index !== 0 && before.index === after.index)
        : before.index !== 0 && after.index === 0;
  return valid ? record : null;
}

function decodeMovementRecords(text) {
  if (text === '') return [];
  const records = [];
  for (const encoded of text.split(',')) {
    const record = decodeMovementRecord(encoded);
    if (record === null) return null;
    records.push(record);
  }
  return records;
}

/** `MovementIntent::decode`: the intent of a canonical marker, or `null`. */
export function decodeMovement(marker) {
  if (!marker.startsWith(MOVEMENT_PREFIX)) return null;
  const text = marker.slice(MOVEMENT_PREFIX.length);
  const split = text.lastIndexOf('|');
  if (split < 0) return null;
  const body = text.slice(0, split);
  const hash = text.slice(split + 1);
  if (hash.length !== 16 || parseU64Hex(hash) !== movementChecksum(body)) return null;
  const fields = body.split('|');
  if (fields.length !== 7 || fields[0] !== '1') return null;
  const fromBranch = decodeUtf16Hex(fields[1]);
  const from = parseI64(fields[2]);
  const toBranch = decodeUtf16Hex(fields[3]);
  const to = parseI64(fields[4]);
  const undo = decodeMovementRecords(fields[5]);
  const redo = decodeMovementRecords(fields[6]);
  if ([fromBranch, toBranch].includes(undefined) || [from, to, undo, redo].includes(null)) {
    return null;
  }
  const intent = { fromBranch, from, toBranch, to, undo, redo };
  if (
    from < 0n ||
    to < 0n ||
    trim(fromBranch) === '' ||
    trim(toBranch) === '' ||
    encodeMovement(intent) !== marker
  ) {
    return null;
  }
  return intent;
}

/** The length of the common prefix of two sequence paths. */
function commonPrefix(left, right) {
  let common = 0;
  while (common < left.length && common < right.length && left[common] === right[common]) {
    common++;
  }
  return common;
}

/** The sequences of `records`, in order, equal those of `sequences`. */
const sameSequences = (records, sequences) =>
  records.length === sequences.length &&
  records.every((record, index) => record.sequence === sequences[index]);

/** The checkout and branch switch half of `VersionControlDecorator`. */
export class VersionControlMovement extends NamedTypeLinks {
  /** The retained records by sequence. */
  availableRecords() {
    return new Map(this.transactions.journal().map((record) => [record.sequence, record]));
  }

  startMovement(targetPath, newBranch) {
    const to = targetPath.length === 0 ? 0n : targetPath[targetPath.length - 1];
    if (newBranch === this.currentBranch && to === this.currentApplied) return;
    const currentPath = this.buildBranchSeqs(this.currentBranch).filter(
      (sequence) => sequence <= this.currentApplied,
    );
    const common = commonPrefix(currentPath, targetPath);
    const available = this.availableRecords();
    const required = (sequence) => {
      const record = available.get(sequence);
      if (record === undefined) {
        throw new Error(
          `Transition ${sequence} is no longer retained; checkout requires complete changed history.`,
        );
      }
      return record;
    };
    // Resolve all changed history before claiming ownership or mutating rows.
    const undo = currentPath.slice(common).reverse().map(required);
    const redo = targetPath.slice(common).map(required);
    this.activeMovement = {
      intent: {
        fromBranch: this.currentBranch,
        from: this.currentApplied,
        toBranch: newBranch,
        to,
        undo,
        redo,
      },
      link: 0,
      prepared: false,
      rowsFlushed: false,
      published: false,
    };
    this.finishMovement();
  }

  finishMovement() {
    const state = this.activeMovement;
    if (state === null) return;
    if (!state.prepared) {
      this.transactions.beginVersionReplay();
      const marker = encodeMovement(state.intent);
      if (state.link === 0 || !this.branchesStore.exists(state.link)) {
        state.link = this.branchesStore.getByName(marker) ?? this.branchesStore.tryCreate(0, 0);
      }
      this.branchesStore.setName(state.link, marker);
      this.branchesStore.save();
      state.prepared = true;
      this.traceLine('Prepared durable version movement.');
    }
    if (!state.rowsFlushed) {
      for (const record of state.intent.undo) {
        this.transactions.replayVersionTransition(record, true);
      }
      for (const record of state.intent.redo) {
        this.transactions.replayVersionTransition(record, false);
      }
      this.transactions.flushVersionReplay();
      state.rowsFlushed = true;
    }
    if (!state.published) {
      this.setCurrentBranch(state.intent.toBranch);
      this.setApplied(state.intent.to);
      this.branchesStore.save();
      this.currentBranch = state.intent.toBranch;
      this.currentApplied = state.intent.to;
      state.published = true;
    }
    this.clearPublicationIntent(state.link);
    this.transactions.finishVersionReplay();
    this.activeMovement = null;
    this.traceLine('Completed durable version movement.');
  }

  /** Finishes the one movement a crash interrupted: `[[link, intent]]`. */
  recoverMovements(pending) {
    if (pending.length === 0) return;
    if (pending.length !== 1) throw new Error('Multiple version movement intents.');
    const [[link, intent]] = pending;
    this.validateMovementAncestry(intent.fromBranch);
    this.validateMovementAncestry(intent.toBranch);
    const fromPath = this.buildBranchSeqs(intent.fromBranch);
    const toPath = this.buildBranchSeqs(intent.toBranch);
    const reachable = (path, sequence) => sequence === 0n || path.includes(sequence);
    if (
      !reachable(fromPath, intent.from) ||
      !reachable(toPath, intent.to) ||
      (this.currentBranch !== intent.fromBranch && this.currentBranch !== intent.toBranch) ||
      (this.currentApplied !== intent.from && this.currentApplied !== intent.to) ||
      (intent.fromBranch !== intent.toBranch &&
        intent.from !== intent.to &&
        this.currentBranch === intent.fromBranch &&
        this.currentApplied === intent.to)
    ) {
      throw new Error('Version movement intent contradicts selected history.');
    }
    const from = fromPath.filter((sequence) => sequence <= intent.from);
    const to = toPath.filter((sequence) => sequence <= intent.to);
    const common = commonPrefix(from, to);
    if (
      !sameSequences(intent.undo, from.slice(common).reverse()) ||
      !sameSequences(intent.redo, to.slice(common))
    ) {
      throw new Error('Version movement intent contradicts its transition paths.');
    }
    const available = this.availableRecords();
    for (const record of [...intent.undo, ...intent.redo]) {
      const stored = available.get(record.sequence);
      if (
        !this.transactions.isTransactionCommitted(record.transactionId) ||
        this.transactions.isTransactionRolledBack(record.transactionId) ||
        (stored !== undefined && !sameRecord(stored, record))
      ) {
        throw new Error('Version movement intent contradicts its committed records.');
      }
    }
    this.activeMovement = {
      intent,
      link,
      prepared: true,
      rowsFlushed: false,
      published: false,
    };
    this.transactions.beginVersionReplay();
    this.traceLine('Recovering interrupted version movement.');
    this.finishMovement();
  }

  /** Rejects broken chains before `buildBranchSeqs` follows them. */
  validateMovementAncestry(branch) {
    const visited = new Set();
    let current = branch;
    while (current !== null) {
      const info = this.branches.get(current);
      if (
        info === undefined ||
        visited.has(current) ||
        info.forkSeq < 0n ||
        info.head < info.forkSeq
      ) {
        throw new Error('Version movement has invalid branch ancestry.');
      }
      visited.add(current);
      current = info.parent;
    }
  }
}
