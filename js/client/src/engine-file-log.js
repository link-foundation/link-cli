// Node/Bun/Deno filesystem adapter. The universal entry point never imports it.
import {
  openSync,
  closeSync,
  readSync,
  writeSync,
  fstatSync,
  ftruncateSync,
  fsyncSync,
  mkdirSync,
  rmdirSync,
  readFileSync,
  renameSync,
} from 'node:fs';
import { dirname, resolve } from 'node:path';
import process from 'node:process';

const decode = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true });
const lenient = new TextDecoder('utf-8', { fatal: false, ignoreBOM: true });
const encode = new TextEncoder();

function syncDirectory(directory) {
  // POSIX directory sync establishes a name before it is relied on.
  if (process.platform !== 'win32') {
    const dir = openSync(directory, 'r');
    try {
      fsyncSync(dir);
    } finally {
      closeSync(dir);
    }
  }
}

function readExactly(fd, bytes, length, position) {
  let offset = 0;
  while (offset < length) {
    const count = readSync(fd, bytes, offset, length - offset, position + offset);
    if (count === 0) throw new Error('journal was truncated by another writer');
    offset += count;
  }
}

/** A single writer, fenced by atomic directory creation. A crashed owner's
 * lock is NEVER stolen automatically: recovery must first fence the old owner.
 * Incomplete final lines, and trailing lines with a NUL byte that power loss
 * left, are repaired on open and before reuse after an I/O error; complete records remain available for uncertain-outcome discovery.
 * The caller owns this adapter and closes it after all engine sessions. */
export class FileCommitLog {
  #fd;
  #lock;
  #closed = false;
  #tailMayBeTorn = false;
  constructor(path) {
    path = resolve(path);
    const directory = dirname(path);
    mkdirSync(directory, { recursive: true });
    this.#lock = path + '.writer';
    mkdirSync(this.#lock); // Fails fast if another writer or a stale lock exists.
    try {
      this.#fd = openSync(path, 'a+');
      this.#repairTail();
      syncDirectory(directory);
    } catch (error) {
      if (this.#fd !== undefined) closeSync(this.#fd);
      rmdirSync(this.#lock);
      throw error;
    }
  }
  // Only uncertain writes trigger a rescan; the buffer stays bounded even
  // when the failed record is large. Clear admission only after durable repair.
  // Power loss can also keep the newline of a record that was never flushed
  // while a sector before it reads back as zeros (the disk reordered writes no
  // flush ordered). Records never contain NUL (append refuses it), so trailing
  // lines holding one are torn too, as etcd's WAL treats a last record with a
  // zeroed sector. A NUL before an intact line is damage that reading refuses.
  #repairTail() {
    const buffer = new Uint8Array(8192);
    let end = fstatSync(this.#fd).size;
    let complete = null; // End of the line being scanned, past its newline.
    let newerNewline = end;
    let zeroed = false;
    let keep = -1;
    while (keep < 0) {
      if (end === 0) {
        keep = zeroed ? 0 : (complete ?? 0);
        break;
      }
      const length = Math.min(end, buffer.length);
      const start = end - length;
      readExactly(this.#fd, buffer, length, start);
      for (let offset = length - 1; offset >= 0 && keep < 0; offset--) {
        const position = start + offset;
        const byte = buffer[offset];
        if (byte === 10) {
          // Blank lines are skipped, as reading the journal skips them.
          if (complete === null) complete = position + 1;
          else if (newerNewline !== position + 1) {
            if (!zeroed) keep = complete;
            else {
              complete = position + 1;
              zeroed = false;
            }
          }
          newerNewline = position;
        } else if (byte === 0 && complete !== null) zeroed = true;
      }
      end = start;
    }
    if (keep !== fstatSync(this.#fd).size) ftruncateSync(this.#fd, keep);
    fsyncSync(this.#fd);
  }
  #repairIfNeeded() {
    if (this.#tailMayBeTorn) {
      this.#repairTail();
      this.#tailMayBeTorn = false;
    }
  }
  get supportsDurableFlush() {
    return true;
  }
  #check() {
    if (this.#closed) throw new Error('journal is closed');
  }
  read() {
    this.#check();
    this.#repairIfNeeded();
    const length = fstatSync(this.#fd).size;
    const bytes = new Uint8Array(length);
    readExactly(this.#fd, bytes, length, 0);
    const text = decode.decode(bytes);
    return JSON.stringify(text === '' ? [] : text.slice(0, -1).split('\n'));
  }
  append(record) {
    this.#check();
    if (typeof record !== 'string' || /[\r\n\0]/.test(record))
      throw new TypeError('journal records must be strings without line breaks or NUL');
    this.#repairIfNeeded();
    const bytes = encode.encode(record + '\n');
    this.#tailMayBeTorn = true;
    let offset = 0;
    while (offset < bytes.length) {
      const count = writeSync(this.#fd, bytes, offset, bytes.length - offset);
      if (count === 0) throw new Error('journal write made no progress');
      offset += count;
    }
    this.#tailMayBeTorn = false;
  }
  flush() {
    this.#check();
    this.#repairIfNeeded();
    this.#tailMayBeTorn = true;
    fsyncSync(this.#fd);
    this.#tailMayBeTorn = false;
  }
  close() {
    if (!this.#closed) {
      this.#closed = true;
      try {
        closeSync(this.#fd);
      } finally {
        rmdirSync(this.#lock);
      }
    }
  }
  [Symbol.dispose]() {
    this.close();
  }
}

function readBytes(path) {
  try {
    return readFileSync(path);
  } catch (error) {
    if (error?.code === 'ENOENT') return new Uint8Array(0);
    throw error;
  }
}

function lines(bytes) {
  const records = [];
  let start = 0;
  for (let index = bytes.indexOf(10); index !== -1; index = bytes.indexOf(10, start)) {
    if (index > start) records.push(bytes.subarray(start, index));
    start = index + 1;
  }
  // Trailing lines with a NUL byte are torn by power loss, as FileCommitLog
  // drops them; one further back is damage.
  while (records.length > 0 && records.at(-1).includes(0)) records.pop();
  return records.map((record) => lenient.decode(record));
}

function writeAll(fd, bytes) {
  let offset = 0;
  while (offset < bytes.length) {
    const count = writeSync(fd, bytes, offset, bytes.length - offset);
    if (count === 0) throw new Error('journal write made no progress');
    offset += count;
  }
  fsyncSync(fd);
}

/** Complete lines of a file journal as `FileCommitLog` reads them, tolerating
 * damage it would reject: invalid UTF-8 becomes U+FFFD (so the checksum flags
 * that record), blank lines are skipped, a torn final line and trailing lines
 * with a NUL byte are ignored and a missing file is empty. */
export function readJournalFileLossy(path) {
  return lines(readBytes(path));
}

/** Rebuild the journal at `path` from the longest valid prefix among it and
 * `copies`. `recover(journals)` picks that prefix, normally
 * `(journals) => WasmEngine.recoverJournal(binding, journals, options)`, which
 * keeps this adapter free of the WebAssembly module. The original bytes are
 * first copied to `<path>.damaged-<n>`; the replacement is flushed, then renamed
 * over `path`. Takes the writer lock of `FileCommitLog`, so it fails while an
 * engine owns the journal; a crashed owner's lock must be removed after fencing
 * it, as for reopening. Copies are only read. */
export function repairJournalFile(path, recover, { copies = [] } = {}) {
  if (typeof recover !== 'function') throw new TypeError('recover must be a function');
  path = resolve(path);
  const lock = path + '.writer';
  try {
    mkdirSync(lock);
  } catch (error) {
    if (error?.code === 'EEXIST') throw new Error('journal writer is busy', { cause: error });
    throw error;
  }
  try {
    const original = readBytes(path);
    const recovery = recover([lines(original), ...copies.map(readJournalFileLossy)]);
    const repaired = encode.encode(recovery.entries.map((entry) => entry + '\n').join(''));
    if (repaired.length === original.length && repaired.every((byte, i) => byte === original[i]))
      return { recovery, changed: false, preserved: null };
    let preserved = null;
    if (original.length > 0) {
      for (let attempt = 1; preserved === null; attempt++) {
        const target = `${path}.damaged-${attempt}`;
        let fd;
        try {
          fd = openSync(target, 'wx');
        } catch (error) {
          if (error?.code === 'EEXIST') continue;
          throw error;
        }
        try {
          writeAll(fd, original);
        } finally {
          closeSync(fd);
        }
        preserved = target;
      }
    }
    const temporary = path + '.repair';
    const fd = openSync(temporary, 'w');
    try {
      writeAll(fd, repaired);
    } finally {
      closeSync(fd);
    }
    renameSync(temporary, path);
    syncDirectory(dirname(path));
    return { recovery, changed: true, preserved };
  } finally {
    rmdirSync(lock);
  }
}
