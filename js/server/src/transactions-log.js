// The append-only file transitions log of rust/src/transactions/log.rs
// (`FileTransitionLog` and `truncate_torn_tail`): one entry per line,
// `fdatasync`ed on every append by default, an incomplete or zeroed tail
// discarded on open and before a writer is reused after an I/O error.
// Retention compacts the log by writing the retained entries to
// `<log>.compact`, syncing it and renaming it over the log (issue #195).
// `openExclusive` holds the `<log>.lock` sidecar lock of storage-lock.js until
// `close`, as the Rust log holds it until drop.
import {
  closeSync,
  existsSync,
  fdatasyncSync,
  fstatSync,
  fsyncSync,
  ftruncateSync,
  mkdirSync,
  readFileSync,
  readSync,
  renameSync,
  unlinkSync,
  writeFileSync,
  writeSync,
} from 'node:fs';
import { dirname } from 'node:path';

import { InvalidFormatError, LinkError, LockError } from './errors.js';
import { openFileSync } from './open-file.js';
import { FileLock, LockMode, lockFilePath } from './storage-lock.js';
import { io, syncParentDirectory } from './transactions-storage.js';

const SCAN_BUFFER = 8192;
const NEWLINE = 0x0a;

/**
 * Shrinks the file at `fd` to its last complete (newline-terminated) line
 * that holds no NUL byte: a crash can leave a fragment of an entry, and
 * power loss a line whose earlier sector reads back as zeros.
 */
export function truncateTornTail(fd) {
  const length = fstatSync(fd).size;
  if (length === 0) return;
  let end = length;
  // End of the line being scanned, just past its newline; `null` while still
  // in the fragment after the last newline.
  let complete = null;
  let newerNewline = length;
  let zeroed = false;
  const buffer = Buffer.alloc(SCAN_BUFFER);
  let keep;
  scan: for (;;) {
    if (end === 0) {
      keep = zeroed ? 0 : (complete ?? 0);
      break;
    }
    const chunk = Math.min(end, SCAN_BUFFER);
    const start = end - chunk;
    readSync(fd, buffer, 0, chunk, start);
    for (let offset = chunk - 1; offset >= 0; offset--) {
      const byte = buffer[offset];
      const position = start + offset;
      if (byte === NEWLINE) {
        if (complete === null) complete = position + 1;
        else if (newerNewline !== position + 1) {
          if (!zeroed) {
            keep = complete;
            break scan;
          }
          complete = position + 1;
          zeroed = false;
        }
        newerNewline = position;
      } else if (byte === 0 && complete !== null) zeroed = true;
    }
    end = start;
  }
  if (keep !== length) {
    ftruncateSync(fd, keep);
    fsyncSync(fd);
  }
}

/** Opens `path` with `flags`, repairs its tail, and closes it. */
function repairFile(path, flags, sync) {
  const fd = openFileSync(path, flags);
  try {
    truncateTornTail(fd);
    if (sync) fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
}

/** `compaction_path`: the temporary file a compaction renames over the log. */
export const compactionPath = (path) => `${path}.compact`;

/** The text of `bytes`, or the I/O error `read_to_string` reports. */
function decodeUtf8(bytes) {
  try {
    return new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  } catch {
    throw new LinkError('I/O error: stream did not contain valid UTF-8');
  }
}

/**
 * Removes the file at `path` unless it is missing, as `std::fs::remove_file`
 * with `NotFound` ignored. `unlinkSync` rather than `rmSync({ force: true })`:
 * Bun reports a parent that is a file as `EFAULT` there instead of `ENOTDIR`.
 */
export function removeFile(path) {
  try {
    unlinkSync(path);
  } catch (error) {
    if (error?.code !== 'ENOENT') throw error;
  }
}

/** A transitions log in one text file, one entry per line. */
export class FileTransitionLog {
  constructor(path, fd) {
    this.filePath = path;
    this.fd = fd;
    this.syncOnAppendEnabled = true;
    this.tailMayBeTorn = false;
    this.lock = null;
  }

  /** Opens (creating if needed) the log at `path`, discarding a torn tail. */
  static open(path) {
    return io(() => {
      const parent = dirname(path);
      if (!existsSync(parent)) mkdirSync(parent, { recursive: true });
      // A compaction interrupted before its rename leaves only a stale copy;
      // the log itself still holds the complete previous journal.
      removeFile(compactionPath(path));
      repairFile(path, existsSync(path) ? 'r+' : 'w+', false);
      const fd = openFileSync(path, 'a+');
      try {
        syncParentDirectory(path);
      } catch (error) {
        closeSync(fd);
        throw error;
      }
      return new FileTransitionLog(path, fd);
    });
  }

  /**
   * Opens the log holding the conventional sidecar lock until `close`.
   * Contention fails fast; all writers must follow the same advisory
   * locking convention.
   */
  static openExclusive(path) {
    const lock = FileLock.tryAcquire(lockFilePath(path), LockMode.Exclusive);
    if (lock === null) throw new LockError('journal writer is busy');
    let log;
    try {
      log = FileTransitionLog.open(path);
    } catch (error) {
      lock.release();
      throw error;
    }
    log.lock = lock;
    return log;
  }

  /** Path of the backing file. */
  path() {
    return this.filePath;
  }

  /** Whether every append is `fdatasync`ed. Enabled by default. */
  syncOnAppend() {
    return this.syncOnAppendEnabled;
  }

  setSyncOnAppend(value) {
    this.syncOnAppendEnabled = value;
  }

  /** Releases the file descriptor and the lock (the Rust log does on drop). */
  close() {
    try {
      if (this.fd !== null) closeSync(this.fd);
    } finally {
      this.lock?.release();
      this.lock = null;
    }
  }

  supportsDurableFlush() {
    return true;
  }

  // An error may follow successful short writes. Keep the flag until both
  // truncation and its durable flush succeed, so repeated I/O faults retry.
  repairIfNeeded() {
    if (this.tailMayBeTorn) {
      io(() => {
        repairFile(this.filePath, 'r+', true);
        // A failed compaction may have released the writer.
        if (this.fd === null) this.fd = openFileSync(this.filePath, 'a+');
      });
      this.tailMayBeTorn = false;
    }
  }

  supportsCompaction() {
    return true;
  }

  /**
   * Atomically and durably replaces every entry with `entries`: after a
   * crash the log holds either the old or the new journal.
   */
  compactLogEntries(entries) {
    if (entries.some((entry) => entry === '' || /[\n\r\0]/.test(entry))) {
      throw new InvalidFormatError(
        'transition log entries must be non-empty single lines without NUL',
      );
    }
    this.repairIfNeeded();
    const temporary = compactionPath(this.filePath);
    io(() => {
      try {
        const fd = openFileSync(temporary, 'w');
        try {
          writeFileSync(fd, entries.map((entry) => `${entry}\n`).join(''));
          fsyncSync(fd);
        } finally {
          closeSync(fd);
        }
        // Release the writer first: Windows refuses to replace an open file.
        // Until it is reopened, repair reopens it first.
        this.tailMayBeTorn = true;
        const writer = this.fd;
        this.fd = null;
        closeSync(writer);
        renameSync(temporary, this.filePath);
      } catch (error) {
        removeFile(temporary);
        throw error;
      }
      syncParentDirectory(this.filePath);
    });
    this.repairIfNeeded();
  }

  /** `ReplaceableTransitionLogStore`: compacts to `entries` unless already equal. */
  replaceLogEntries(entries) {
    const current = this.readLogEntries();
    if (current.length === entries.length && current.every((entry, i) => entry === entries[i])) {
      return false;
    }
    this.compactLogEntries(entries);
    return true;
  }

  appendLogEntry(entry) {
    if (/[\n\r\0]/.test(entry)) {
      throw new InvalidFormatError('transition log entries must not contain line breaks or NUL');
    }
    this.repairIfNeeded();
    this.tailMayBeTorn = true;
    io(() => {
      writeSync(this.fd, `${entry}\n`);
      if (this.syncOnAppendEnabled) fdatasyncSync(this.fd);
    });
    this.tailMayBeTorn = false;
  }

  readLogEntries() {
    this.repairIfNeeded();
    const contents = decodeUtf8(io(() => readFileSync(this.filePath)));
    // Only newline-terminated lines were fully written.
    const lines = contents.split('\n').slice(0, -1);
    return lines
      .map((line) => (line.endsWith('\r') ? line.slice(0, -1) : line))
      .filter((line) => line !== '');
  }

  flushLog() {
    this.repairIfNeeded();
    this.tailMayBeTorn = true;
    io(() => fdatasyncSync(this.fd));
    this.tailMayBeTorn = false;
  }
}
