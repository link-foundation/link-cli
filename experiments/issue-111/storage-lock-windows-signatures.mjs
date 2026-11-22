// Checks that the kernel32 declarations loadKernel32 (js/server/src/storage-lock-windows.js)
// hands to node:ffi or bun:ffi are accepted by the runtime's FFI on Linux, where
// kernel32.dll does not exist: every declaration is bound in turn to the C
// library's `getpid` (bound only, never called), for both pointer sizes.
// Acceptance means something: both FFIs reject an unknown type name at
// dlopen (node:ffi "Unsupported FFI type: bogus", bun:ffi "Unknown type bogus").
// Result on Node.js 26.13.0 and Bun 1.4.4: every declaration is accepted.
// Run: node experiments/issue-111/storage-lock-windows-signatures.mjs
//      bun experiments/issue-111/storage-lock-windows-signatures.mjs
import { loadKernel32 } from '../../js/server/src/storage-lock-windows.js';

const bun = process.versions.bun !== undefined;
const id = bun ? 'bun:ffi' : 'node:ffi';
const ffi = bun ? await import('bun:ffi') : process.getBuiltinModule('node:ffi');
for (const pointerSize of [8, 4]) {
  const accepted = [];
  const checking = {
    ...ffi,
    FFIType: ffi.FFIType,
    dlopen(name, definitions) {
      if (name !== 'kernel32.dll') throw new Error(`unexpected library ${name}`);
      for (const [symbol, definition] of Object.entries(definitions)) {
        const library = ffi.dlopen('libc.so.6', { getpid: definition });
        accepted.push(symbol);
        library.close?.();
        library.lib?.close?.();
      }
      return { functions: {}, symbols: {} };
    },
  };
  loadKernel32(checking, id, pointerSize);
  console.log(`${id} pointer size ${pointerSize}: accepted ${accepted.join(', ')}`);
}
