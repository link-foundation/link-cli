// Probes the bun:ffi pointer and buffer conventions the Windows lock backend
// (js/server/src/storage-lock-windows.js) relies on, using C library functions
// on Linux that take or return the same kinds of values kernel32 does:
// - a Buffer or Uint8Array passed for a `ptr` argument is the address of its
//   bytes, and the callee writes through it (OVERLAPPED, GetFullPathNameW);
// - null passes a NULL pointer (lpSecurityAttributes, hTemplateFile);
// - `u32` takes 0xFFFFFFFF (u32::MAX) and returns it unchanged;
// - `i64` returns a bigint, and (void *)-1 -- INVALID_HANDLE_VALUE -- reads
//   back as -1n; an `i64` argument takes a bigint handle back unchanged.
// Run: bun experiments/issue-111/storage-lock-windows-bun-ffi-api.mjs
const { dlopen, FFIType } = await import('bun:ffi');
const { ptr, u64, i32, u32, i64 } = FFIType;
const { symbols: libc } = dlopen('libc.so.6', {
  strlen: { args: [ptr], returns: u64 },
  memset: { args: [ptr, i32, u64], returns: ptr },
  time: { args: [ptr], returns: i64 },
  htonl: { args: [u32], returns: u32 },
  llabs: { args: [i64], returns: i64 },
  mmap: { args: [ptr, u64, i32, i32, i32, i64], returns: i64 },
});
const results = {};
const check = (name, actual, expected) => {
  results[name] = actual === expected ? 'ok' : `FAIL ${actual} (${typeof actual})`;
};
check('strlen(Buffer)', Number(libc.strlen(Buffer.from('hello\0'))), 5);
check('strlen(utf16le Buffer)', Number(libc.strlen(Buffer.from('a\0', 'utf16le'))), 1);
const out = Buffer.alloc(32, 7);
libc.memset(out, 0, 32);
check(
  'memset writes through a Buffer',
  out.every((byte) => byte === 0),
  true,
);
const view = new Uint8Array(8).fill(1);
libc.memset(view, 0, 8);
check(
  'memset writes through a Uint8Array',
  view.every((byte) => byte === 0),
  true,
);
check('time(null) is a bigint', typeof libc.time(null), 'bigint');
check('u32 0xFFFFFFFF round-trips', libc.htonl(0xffffffff), 0xffffffff);
check('i64 bigint argument', libc.llabs(-5n), 5n);
check('i64 return of (void *)-1', libc.mmap(null, 0, 0, 0, -1, 0), -1n);
console.log(results);
