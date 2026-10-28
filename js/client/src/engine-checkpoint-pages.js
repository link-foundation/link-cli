// Optional two-bank replacement. Devices stay exclusive and caller-owned.
import { PagedCommitLog } from './engine-paged-log.js';
import { MAX_PAGES, CELLS, encoder, metadata, decodeMetadata, finish, same, fail } from './engine-page-format.js';
import { bankIdentity, decisionBytes, selectDecision } from './engine-checkpoint-page-format.js';
import { inspectCheckpointPages } from './engine-checkpoint-inspection.js';

export class CheckpointPagedCommitLog {
  /** Owned committed records; no initialization, repair completion or truncation. */
  static inspect(root, banks, options) { return inspectCheckpointPages(root, banks, options); }
  #root; #banks; #active; #decision; #maxPages;
  #pending = false;
  #fenced = false;
  constructor(root, banks, { maxPages = MAX_PAGES } = {}) {
    if (!Array.isArray(banks) || banks.length !== 2 || !root || banks.some((bank) => !bank)
      || new Set([root, ...banks]).size !== 3) throw new TypeError('checkpoint devices must be three distinct exclusive owners');
    if (!Number.isSafeInteger(maxPages) || maxPages < 1 || maxPages > MAX_PAGES) throw new RangeError('invalid checkpoint page limit');
    this.#root = root; this.#banks = banks.slice(); this.#maxPages = maxPages;
    const flags = banks[0].metadataFlags ?? 0, rootFlags = root.metadataFlags ?? 0;
    if ((rootFlags !== 0 && rootFlags !== (2 | flags)) || (flags !== 0 && flags !== 1) || (banks[1].metadataFlags ?? 0) !== flags)
      fail('checkpoint requires matching root and bank modes (flags)');
    if (root.dataLength !== 0) fail('checkpoint root data must be empty');
    let decision = selectDecision([root.readMetadata(0), root.readMetadata(1)], rootFlags | flags);
    if (decision !== undefined) {
      const bank = banks[Number(decision.epoch % 2n)], expected = bankIdentity(decision.identity, decision.epoch);
      let found = false;
      for (let slot = 0; slot < 2; slot++) {
        const state = decodeMetadata(bank.readMetadata(slot), flags);
        if (state !== null && state !== undefined) {
          if (!same(state.identity, expected)) fail('checkpoint bank epoch identity changed');
          found = true;
        }
      }
      if (!found) fail('checkpoint selected bank has no valid metadata');
    } else {
      for (const bank of banks) {
        if (bank.dataLength !== 0) fail('unrooted checkpoint bank contains data');
        for (let slot = 0; slot < 2; slot++) {
          const state = decodeMetadata(bank.readMetadata(slot), flags);
          if (state !== null && state !== undefined && state.pages !== 0) fail('unrooted checkpoint bank contains committed metadata');
        }
      }
      const identity = root.newDatabaseId();
      if (!(identity instanceof Uint8Array) || identity.length !== 16) fail('checkpoint root identity length');
      decision = { identity: identity.slice(), epoch: 0n, flags: rootFlags | flags };
      for (let i = 0; i < 2; i++) this.#reset(banks[i], bankIdentity(identity, BigInt(i)), flags);
      finish(root.syncData()); finish(root.writeMetadata(0, decisionBytes(decision))); finish(root.syncMetadata(0));
    }
    this.#decision = decision;
    this.#active = new PagedCommitLog(banks[Number(decision.epoch % 2n)], { maxPages });
    if ((rootFlags & 2) !== 0) finish(root.completeRecovery());
  }
  #reset(device, identity, flags) {
    finish(device.truncateData(0)); finish(device.syncData());
    const empty = metadata({ flags, identity, generation: 0, pages: 0, rows: 0, records: 0, root: 0, lastHash: 0n });
    for (let slot = 0; slot < 2; slot++) { finish(device.writeMetadata(slot, empty)); finish(device.syncMetadata(slot)); }
  }
  get epoch() { return this.#decision.epoch; }
  get supportsReplacement() { return true; }
  get supportsDurableFlush() { return this.#root.supportsDurableFlush === true && this.#banks.every((bank) => bank.supportsDurableFlush === true); }
  #check() { if (this.#fenced) throw new Error('uncertain checkpoint page owner is fenced; reopen all devices'); }
  read() {
    this.#check();
    try { return this.#active.read(); } catch (error) { this.#fenced = true; throw error; }
  }
  append(record) { this.#check(); this.#active.append(record); this.#pending = true; }
  flush() {
    this.#check();
    try { this.#active.flush(); this.#pending = false; } catch (error) { this.#fenced = true; throw error; }
  }
  replace(records) {
    this.#check();
    if (this.#pending) fail('checkpoint replacement rejects pending physical records');
    if (!Array.isArray(records)) throw new TypeError('checkpoint records must be an array');
    const copy = records.slice(); let rows = 0;
    for (const record of copy) {
      if (typeof record !== 'string' || !record.isWellFormed() || /[\r\n]/.test(record))
        throw new TypeError('checkpoint records must be well-formed strings without line breaks');
      rows += Math.ceil(encoder.encode(record).length / 7) + 1;
      // Bound every addition by the portable page space, before encoding any
      // following record. A string/typed-array length is at most a safe integer.
      if (rows > this.#maxPages * CELLS) throw new RangeError('checkpoint exceeds selected page limit');
    }
    const before = JSON.parse(this.read());
    if (before.length === copy.length && before.every((record, i) => record === copy[i])) return false;
    if (this.#decision.epoch === 0xffffffffffffffffn) throw new RangeError('checkpoint epoch exhausted');
    const next = { ...this.#decision, epoch: this.#decision.epoch + 1n }, slot = Number(next.epoch % 2n);
    this.#fenced = true;
    this.#reset(this.#banks[slot], bankIdentity(next.identity, next.epoch), next.flags & 1);
    const prepared = new PagedCommitLog(this.#banks[slot], { maxPages: this.#maxPages });
    for (const record of copy) prepared.append(record);
    prepared.flush(); finish(this.#root.writeMetadata(slot, decisionBytes(next))); finish(this.#root.syncMetadata(slot));
    this.#active = prepared; this.#decision = next; this.#fenced = false; return true;
  }
}
