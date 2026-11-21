// The Windows advisory lock of storage-lock.js for Bun and Node.js: what the
// Rust std `File::open`, `lock`, `lock_shared`, `try_lock`, `try_lock_shared`
// and `unlock` do on Windows (library/std/src/sys/fs/windows.rs), called on
// kernel32.dll through `bun:ffi` or `node:ffi`.
//
// The sidecar is opened with `CreateFileW` itself -- `GENERIC_READ |
// GENERIC_WRITE`, every `FILE_SHARE_*`, `OPEN_ALWAYS` -- on the path Rust's
// `maybe_verbatim` makes, and closed with `CloseHandle`: a Node.js file
// descriptor is a C runtime one, and its `HANDLE` is out of reach. Locks are
// `LockFileEx` over the whole range (offset 0, length `u32::MAX` twice) with
// a zeroed `OVERLAPPED`, `LOCKFILE_EXCLUSIVE_LOCK` for exclusive holders and
// `LOCKFILE_FAIL_IMMEDIATELY` for tries, which report `ERROR_LOCK_VIOLATION`
// as busy. Unlocking is `UnlockFile` twice, as Rust does. Failures carry
// `GetLastError` and print as `std::io::Error`: `FormatMessageW` text, then
// ` (os error N)`.
//
// `kernel32Backend` takes the bound functions, so its logic runs anywhere
// over a stand-in; `loadKernel32` binds the real ones.

const GENERIC_READ = 0x80000000;
const GENERIC_WRITE = 0x40000000;
const FILE_SHARE_ALL = 0x1 | 0x2 | 0x4; // FILE_SHARE_READ | _WRITE | _DELETE
const OPEN_ALWAYS = 4;
const LOCKFILE_FAIL_IMMEDIATELY = 0x1;
const LOCKFILE_EXCLUSIVE_LOCK = 0x2;
const U32_MAX = 0xffffffff;
const ERROR_LOCK_VIOLATION = 33;
const ERROR_NOT_LOCKED = 158;
const ERROR_IO_PENDING = 997;
const FORMAT_MESSAGE_IGNORE_INSERTS = 0x200;
const FORMAT_MESSAGE_FROM_SYSTEM = 0x1000;
// `GetFullPathNameW` refuses longer paths without the `\\?\` prefix.
const LEGACY_MAX_PATH = 248;

/** A `std::io::Error` of the Windows calls: `message` is its `Display` text. */
export class WindowsIoError extends Error {
  constructor(message, rawOsError) {
    super(message);
    this.name = 'WindowsIoError';
    this.rawOsError = rawOsError;
  }
}

/** The `OVERLAPPED` size and its `hEvent` offset for a pointer size. */
export const overlappedLayout = (pointerSize) =>
  pointerSize === 8 ? { size: 32, hEvent: 24 } : { size: 20, hEvent: 16 };

const wide = (text) => Buffer.from(`${text}\0`, 'utf16le');

/**
 * `maybe_verbatim`: the path as UTF-16 with a NUL, made absolute and `\\?\`
 * prefixed unless it is already verbatim or a short drive or UNC path.
 */
export function maybeVerbatim(kernel32, path, lastOsError) {
  if (path.includes('\0')) {
    throw new WindowsIoError('strings passed to WinAPI cannot contain NULs');
  }
  if (path === '' || path.startsWith('\\\\?\\') || path.startsWith('\\??\\')) return wide(path);
  const short = path.length + 1 < LEGACY_MAX_PATH;
  if (short && (/^[^\\/]:(?:[\\/]|$)/.test(path) || /^[\\/]{2}/.test(path))) return wide(path);
  const absolute = fullPathName(kernel32, path, lastOsError);
  if (/^[\s\S]:\\/.test(absolute)) return wide(`\\\\?\\${absolute}`);
  if (absolute.startsWith('\\\\.\\')) return wide(`\\\\?\\${absolute.slice(4)}`);
  if (absolute.startsWith('\\\\?\\') || absolute.startsWith('\\??\\')) return wide(absolute);
  if (absolute.startsWith('\\\\')) return wide(`\\\\?\\UNC\\${absolute.slice(2)}`);
  return wide(absolute);
}

/** `GetFullPathNameW` through `fill_utf16_buf`, starting from 512 units. */
function fullPathName(kernel32, path, lastOsError) {
  const name = wide(path);
  let size = 512;
  for (;;) {
    const buffer = Buffer.alloc(size * 2);
    const length = kernel32.GetFullPathNameW(name, size, buffer, null);
    if (length === 0) throw lastOsError();
    if (length < size) return buffer.toString('utf16le', 0, length * 2);
    size = length > size ? length : size * 2;
  }
}

/** `sys::io::error_string`: the system text for `code`, as `FormatMessageW` writes it. */
export function errorString(kernel32, code) {
  const buffer = Buffer.alloc(2048 * 2);
  const flags = FORMAT_MESSAGE_FROM_SYSTEM | FORMAT_MESSAGE_IGNORE_INSERTS;
  const length = kernel32.FormatMessageW(flags, null, code, 0, buffer, 2048, null);
  if (length === 0) {
    const failure = kernel32.GetLastError();
    return `OS Error ${code} (FormatMessageW() returned error ${failure})`;
  }
  const text = buffer.toString('utf16le', 0, length * 2);
  if (!text.isWellFormed()) return `OS Error ${code} (FormatMessageW() returned invalid UTF-16)`;
  return text.replace(/[\t\n\f\r ]+$/, '');
}

/**
 * Advisory locks through kernel32 functions bound as
 * `{ CreateFileW, CloseHandle, LockFileEx, UnlockFile, CreateEventW,
 * GetOverlappedResult, GetLastError, GetFullPathNameW, FormatMessageW }`,
 * taking and returning handles as the binding represents them, buffers for
 * pointers and `null` for NULL. `pointerSize` lays out `OVERLAPPED`.
 */
export function kernel32Backend(primitive, kernel32, pointerSize) {
  const layout = overlappedLayout(pointerSize);
  const osError = (code) =>
    new WindowsIoError(`${errorString(kernel32, code)} (os error ${code})`, code);
  const lastOsError = () => osError(kernel32.GetLastError());
  const lockFile = (handle, flags, overlapped) =>
    kernel32.LockFileEx(handle, flags, 0, U32_MAX, U32_MAX, overlapped) !== 0;
  const unlockFile = (handle) => kernel32.UnlockFile(handle, 0, 0, U32_MAX, U32_MAX) !== 0;

  // `acquire_lock`: waits on an event in case the handle completes the lock
  // asynchronously; a handle CreateFileW opened without FILE_FLAG_OVERLAPPED
  // simply blocks.
  const acquire = (handle, flags) => {
    const overlapped = Buffer.alloc(layout.size);
    const event = kernel32.CreateEventW(null, 0, 0, null);
    if (Number(event) === 0) throw lastOsError();
    if (pointerSize === 8) overlapped.writeBigInt64LE(BigInt(event), layout.hEvent);
    else overlapped.writeInt32LE(Number(event), layout.hEvent);
    let failure = null;
    if (!lockFile(handle, flags, overlapped)) {
      const code = kernel32.GetLastError();
      if (code !== ERROR_IO_PENDING) failure = osError(code);
      else if (kernel32.GetOverlappedResult(handle, overlapped, Buffer.alloc(4), 1) === 0) {
        failure = lastOsError();
      }
    }
    kernel32.CloseHandle(event);
    if (failure !== null) throw failure;
  };

  return {
    primitive,
    open: (path) => {
      const name = maybeVerbatim(kernel32, path, lastOsError);
      const access = (GENERIC_READ | GENERIC_WRITE) >>> 0;
      const handle = kernel32.CreateFileW(name, access, FILE_SHARE_ALL, null, OPEN_ALWAYS, 0, null);
      if (Number(handle) === -1) throw lastOsError();
      return handle;
    },
    lock: (handle, exclusive) => acquire(handle, exclusive ? LOCKFILE_EXCLUSIVE_LOCK : 0),
    tryLock: (handle, exclusive) => {
      const flags = LOCKFILE_FAIL_IMMEDIATELY | (exclusive ? LOCKFILE_EXCLUSIVE_LOCK : 0);
      if (lockFile(handle, flags, Buffer.alloc(layout.size))) return true;
      const code = kernel32.GetLastError();
      if (code === ERROR_LOCK_VIOLATION) return false;
      throw osError(code);
    },
    unlock: (handle) => {
      // A handle may hold an exclusive and a shared lock; each needs an unlock.
      if (!unlockFile(handle)) throw lastOsError();
      if (unlockFile(handle)) return;
      const code = kernel32.GetLastError();
      if (code !== ERROR_NOT_LOCKED) throw osError(code);
    },
    close: (handle) => {
      kernel32.CloseHandle(handle);
    },
  };
}

// The kernel32 signatures in each foreign function interface: handles are
// pointer-sized integers (so INVALID_HANDLE_VALUE reads back as -1), pointers
// take buffers or null.
const SIGNATURES = {
  'node:ffi': (ffi, wide) => ({
    library: (name, definitions) => ffi.dlopen(name, definitions).functions,
    types: { H: wide ? 'int64' : 'int32', P: 'pointer', U: 'uint32', I: 'int32' },
    declare: (parameters, result) => ({ arguments: parameters, return: result }),
  }),
  'bun:ffi': (ffi, wide) => {
    const { i64, i32, ptr, u32 } = ffi.FFIType;
    return {
      library: (name, definitions) => ffi.dlopen(name, definitions).symbols,
      types: { H: wide ? i64 : i32, P: ptr, U: u32, I: i32 },
      declare: (parameters, result) => ({ args: parameters, returns: result }),
    };
  },
};

/** Binds the kernel32 functions `kernel32Backend` calls through `ffi` (`id`). */
export function loadKernel32(ffi, id, pointerSize) {
  const { library, types, declare } = SIGNATURES[id](ffi, pointerSize === 8);
  const { H, P, U, I } = types;
  return library('kernel32.dll', {
    CreateFileW: declare([P, U, U, P, U, U, P], H),
    CloseHandle: declare([H], I),
    LockFileEx: declare([H, U, U, U, U, P], I),
    UnlockFile: declare([H, U, U, U, U], I),
    CreateEventW: declare([P, I, I, P], H),
    GetOverlappedResult: declare([H, P, P, I], I),
    GetLastError: declare([], U),
    GetFullPathNameW: declare([P, U, P, P], U),
    FormatMessageW: declare([U, P, U, U, P, U, P], U),
  });
}
