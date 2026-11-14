// Which millisecond each runtime stores when `futimesSync` is given one.
// Node.js and Deno turn a Date into fractional seconds, and `x.370` is
// `x.36999…` in binary, so about half the Dates are stored just below the
// requested millisecond. Node.js keeps nanoseconds (`…369999999`); Deno 2.9
// keeps whole milliseconds, so it stores `…369`, the millisecond before.
// DoubletsStorage.flush asks for the middle of the millisecond, which every
// runtime stores inside it (root cause 16 in the case study).
// Run: node|bun|deno run -A experiments/issue-111/futimes-millisecond-truncation.mjs
import { closeSync, futimesSync, mkdtempSync, openSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const path = join(mkdtempSync(join(tmpdir(), 'futimes-')), 'file');
writeFileSync(path, 'x');
const storedMillisecond = (time) => {
  const fd = openSync(path, 'r+');
  futimesSync(fd, time, time);
  closeSync(fd);
  return statSync(path, { bigint: true }).mtimeNs / 1_000_000n;
};
let early = 0;
let earlyMiddle = 0;
const base = Date.now();
for (let ms = base; ms < base + 1000; ms++) {
  if (storedMillisecond(new Date(ms)) < BigInt(ms)) early++;
  if (storedMillisecond((ms + 0.5) / 1000) < BigInt(ms)) earlyMiddle++;
}
console.log(`of 1000 milliseconds: a Date stored early ${early} times,`);
console.log(`the middle of the millisecond stored early ${earlyMiddle} times`);
