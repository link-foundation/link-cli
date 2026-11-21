// Probes bun:ffi for flock(2) and errno. Run: bun experiments/issue-111/storage-lock-bun-ffi-api.mjs
import { openSync, closeSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const { dlopen, FFIType, read } = await import('bun:ffi');
const { symbols, close } = dlopen('libc.so.6', {
  flock: { args: [FFIType.i32, FFIType.i32], returns: FFIType.i32 },
  __errno_location: { args: [], returns: FFIType.ptr },
});
const dir = mkdtempSync(join(tmpdir(), 'lock-'));
const a = openSync(join(dir, 'x.lock'), 'a+');
const b = openSync(join(dir, 'x.lock'), 'a+');
console.log('fds', a, b);
console.log('flock(a, EX|NB)', symbols.flock(a, 6));
const r = symbols.flock(b, 6);
const p = symbols.__errno_location();
console.log('flock(b, EX|NB)', r, 'errno ptr', typeof p, 'errno', read.i32(p, 0));
console.log('flock(a, UN)', symbols.flock(a, 8), 'flock(b, EX|NB)', symbols.flock(b, 6));
closeSync(a);
closeSync(b);
close();
