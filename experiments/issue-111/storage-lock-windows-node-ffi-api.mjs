// Probes the node:ffi pointer and buffer conventions the Windows lock backend
// (js/server/src/storage-lock-windows.js) relies on, using C library functions
// on Linux that take or return the same kinds of values kernel32 does:
// - a Buffer passed for a 'pointer' argument is the address of its bytes, and
//   the callee writes through it (out-buffers: OVERLAPPED, GetFullPathNameW);
// - null passes a NULL pointer (lpSecurityAttributes, hTemplateFile);
// - 'uint32' takes 0xFFFFFFFF (u32::MAX) and returns it unchanged;
// - 'int64' returns a bigint, and (void *)-1 -- INVALID_HANDLE_VALUE -- reads
//   back as -1n, while 'pointer' returns it as an unsigned bigint;
// - an 'int64' argument takes a bigint handle back unchanged.
// Run: node experiments/issue-111/storage-lock-windows-node-ffi-api.mjs
const ffi = process.getBuiltinModule('node:ffi');
const open = (definitions) => ffi.dlopen('libc.so.6', definitions).functions;
const libc = open({
  strlen: { arguments: ['pointer'], return: 'uint64' },
  memset: { arguments: ['pointer', 'int32', 'uint64'], return: 'pointer' },
  time: { arguments: ['pointer'], return: 'int64' },
  htonl: { arguments: ['uint32'], return: 'uint32' },
  llabs: { arguments: ['int64'], return: 'int64' },
});
const asInt64 = open({
  mmap: { arguments: ['pointer', 'uint64', 'int32', 'int32', 'int32', 'int64'], return: 'int64' },
});
const asPointer = open({
  mmap: { arguments: ['pointer', 'uint64', 'int32', 'int32', 'int32', 'int64'], return: 'pointer' },
});
const results = {};
const check = (name, actual, expected) => {
  results[name] = Object.is(actual, expected) || actual === expected ? 'ok' : `FAIL ${actual}`;
};
check('strlen(Buffer)', libc.strlen(Buffer.from('hello\0')), 5n);
check('strlen(utf16le Buffer)', libc.strlen(Buffer.from('a\0', 'utf16le')), 1n);
const out = Buffer.alloc(32, 7);
libc.memset(out, 0, 32n);
check(
  'memset writes through a Buffer',
  out.every((byte) => byte === 0),
  true,
);
const view = new Uint8Array(8).fill(1);
libc.memset(view, 0, 8n);
check(
  'memset writes through a Uint8Array',
  view.every((byte) => byte === 0),
  true,
);
check('time(null) is a bigint', typeof libc.time(null), 'bigint');
check('uint32 0xFFFFFFFF round-trips', libc.htonl(0xffffffff), 0xffffffff);
check('int64 bigint argument', libc.llabs(-5n), 5n);
check('int64 return of (void *)-1', asInt64.mmap(null, 0n, 0, 0, -1, 0n), -1n);
check('pointer return of (void *)-1', asPointer.mmap(null, 0n, 0, 0, -1, 0n), 0xffffffffffffffffn);
const handle = Buffer.alloc(8);
handle.writeBigInt64LE(0x1234n, 0);
check('Buffer.writeBigInt64LE stores a handle', handle.readBigInt64LE(0), 0x1234n);
console.log(results);
