// Root decisions are independent from the unchanged native bank page format.
import { hash, same, view, encoder, fail } from './engine-page-format.js';
const MAGIC = encoder.encode('CLNKBK01'), DOMAIN = encoder.encode('BANK0001');
export const bankIdentity = (identity, epoch) => {
  const mask = new Uint8Array(16); view(mask).setBigUint64(0, epoch, true); mask.set(DOMAIN, 8);
  return identity.map((value, i) => value ^ mask[i]);
};
export function decisionBytes({ identity, epoch, flags }) {
  const bytes = new Uint8Array(128), data = view(bytes), bank = Number(epoch % 2n);
  bytes.set(MAGIC); bytes.set(identity, 8); data.setBigUint64(24, epoch, true);
  data.setUint32(32, bank, true); data.setUint32(36, flags, true); bytes.set(bankIdentity(identity, epoch), 40);
  data.setBigUint64(120, hash(bytes.subarray(0, 120)), true); return bytes;
}
function decode(bytes, slot, flags) {
  if (bytes === undefined) return undefined;
  if (!(bytes instanceof Uint8Array)) fail('checkpoint root bytes');
  if (bytes.length !== 128 || hash(bytes.subarray(0, 120)) !== view(bytes).getBigUint64(120, true)) return undefined;
  if (!same(bytes.subarray(0, 8), MAGIC) || bytes.subarray(56, 120).some(Boolean)) fail('unsupported checkpoint root version or padding');
  const data = view(bytes), identity = bytes.slice(8, 24), epoch = data.getBigUint64(24, true), bank = Number(epoch % 2n);
  if (data.getUint32(36, true) !== flags || data.getUint32(32, true) !== bank || bank !== slot
    || !same(bytes.subarray(40, 56), bankIdentity(identity, epoch))) fail('checkpoint root flags, bank or epoch identity');
  return { identity, epoch, flags };
}
export function selectDecision(raw, flags) {
  const a = decode(raw[0], 0, flags), b = decode(raw[1], 1, flags);
  if (a === undefined) return b;
  if (b === undefined) return a;
  const difference = a.epoch > b.epoch ? a.epoch - b.epoch : b.epoch - a.epoch;
  if (!same(a.identity, b.identity) || difference !== 1n) fail('conflicting checkpoint root decisions');
  return a.epoch > b.epoch ? a : b;
}
