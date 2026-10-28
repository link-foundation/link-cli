// Selected-prefix discovery before higher-level semantic validation.
import { PAGE, MAX_PAGES, decodeMetadata, same, fail } from './engine-page-format.js';
import { bankIdentity, selectDecision } from './engine-checkpoint-page-format.js';
import { readPageRecords } from './engine-page-reader.js';

export function inspectCheckpointPages(root, banks, { maxPages = MAX_PAGES } = {}) {
  if (!Array.isArray(banks) || banks.length !== 2 || !root || banks.some(bank => !bank)
    || new Set([root, ...banks]).size !== 3) throw TypeError('three distinct exclusive devices required');
  if (!Number.isSafeInteger(maxPages) || maxPages < 1 || maxPages > MAX_PAGES) throw RangeError('inspection page bound');
  const flags = banks[0].metadataFlags ?? 0, rootFlags = root.metadataFlags ?? 0;
  if ((rootFlags !== 0 && rootFlags !== (2 | flags)) || (flags !== 0 && flags !== 1) || (banks[1].metadataFlags ?? 0) !== flags)
    fail('inspection requires matching root and bank modes');
  if (root.dataLength !== 0) fail('checkpoint root data must be empty');
  const decision = selectDecision([root.readMetadata(0), root.readMetadata(1)], rootFlags | flags);
  if (decision === undefined) {
    for (const bank of banks) {
      if (bank.dataLength !== 0) fail('unrooted checkpoint bank contains data');
      for (let slot = 0; slot < 2; slot++) {
        const state = decodeMetadata(bank.readMetadata(slot), flags);
        if (state !== null && state !== undefined && state.pages !== 0) fail('unrooted checkpoint bank contains committed metadata');
      }
    }
    return [];
  }
  const bank = banks[Number(decision.epoch % 2n)], expected = bankIdentity(decision.identity, decision.epoch);
  const raw = [bank.readMetadata(0), bank.readMetadata(1)];
  const states = raw.map(bytes => decodeMetadata(bytes, flags)).filter(state => state !== null && state !== undefined);
  if (states.length === 0) fail('checkpoint selected bank has no valid metadata');
  if (states.some(state => !same(state.identity, expected))) fail('checkpoint bank epoch identity changed');
  if (states.length === 2 && states[0].generation === states[1].generation && !same(raw[0], raw[1]))
    fail('conflicting metadata decisions');
  const state = states.reduce((left, right) => left.generation > right.generation ? left : right);
  if (state.pages > maxPages) fail('committed pages exceed selected limit');
  if (!Number.isSafeInteger(bank.dataLength) || bank.dataLength < state.pages * PAGE) fail('truncated committed pages');
  // Checked mirror reads may stage volatile repairs. The owner must discard
  // those wrappers on higher-level failure; inspection never completes repair.
  return readPageRecords(bank, state, true);
}
