// Probes the node:ffi API (Node.js >= 26): dlopen definition shape, errno access.
// Result on Node.js v26.13.0: only `{ arguments: [...], return: '...' }` declares a
// signature; `parameters`/`result` and `args`/`returns` are silently ignored (the
// function then takes no arguments). `__errno_location()` returns a bigint pointer
// that `getInt32` reads; a second open file description gets EWOULDBLOCK (11).
// Run: node experiments/issue-111/storage-lock-node-ffi-api.mjs
import { openSync, closeSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const ffi = process.getBuiltinModule('node:ffi');
console.log('node:ffi exports', ffi && Object.keys(ffi).length);
const shapes = [
  {
    flock: { parameters: ['int32', 'int32'], result: 'int32' },
    __errno_location: { parameters: [], result: 'pointer' },
  },
  {
    flock: { args: ['int32', 'int32'], returns: 'int32' },
    __errno_location: { args: [], returns: 'pointer' },
  },
  {
    flock: { arguments: ['int32', 'int32'], return: 'int32' },
    __errno_location: { arguments: [], return: 'pointer' },
  },
];
for (const shape of shapes) {
  try {
    const { lib, functions } = ffi.dlopen('libc.so.6', shape);
    console.log('shape OK', JSON.stringify(shape.flock), Object.keys(functions));
    const dir = mkdtempSync(join(tmpdir(), 'lock-'));
    const a = openSync(join(dir, 'x.lock'), 'a+');
    const b = openSync(join(dir, 'x.lock'), 'a+');
    console.log('flock(a, EX|NB)', functions.flock(a, 2 | 4));
    const r = functions.flock(b, 2 | 4);
    const p = functions.__errno_location();
    console.log('flock(b, EX|NB)', r, 'errno ptr', typeof p, p, 'errno', ffi.getInt32(p));
    console.log('flock(a, UN)', functions.flock(a, 8), 'flock(b, EX|NB)', functions.flock(b, 6));
    closeSync(a);
    closeSync(b);
    lib.close();
  } catch (error) {
    console.log('shape failed', JSON.stringify(shape.flock), error.message);
  }
}
