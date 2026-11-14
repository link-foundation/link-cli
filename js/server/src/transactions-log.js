// The append-only file transitions log of rust/src/transactions/log.rs
// (`FileTransitionLog` and `truncate_torn_tail`): one entry per line,
// `fdatasync`ed on every append by default, an incomplete or zeroed tail
// discarded on open and before a writer is reused after an I/O error.
//
// `FileTransitionLog::open_exclusive` is not ported: it needs the sidecar
// lock of storage-lock.js.
import {
  closeSync,
  existsSync,
  fdatasyncSync,
  fstatSync,
  fsyncSync,
  ftruncateSync,
  mkdirSync,
  openSync,
  readFileSync,
  readSync,
  writeSync,
} from 'node:fs';
import { dirname } from 'node:path';

import { InvalidFormatError, LinkError } from './errors.js';
import { io, parentDirectory, syncPath } from './transactions-storage.js';

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
  const fd = openSync(path, flags);
  try {
    truncateTornTail(fd);
    if (sync) fsyncSync(fd);
  } finally {
    closeSync(fd);
  }
}

/** The text of `bytes`, or the I/O error `read_to_string` reports. */
function decodeUtf8(bytes) {
  try {
    return new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
  } catch {
    throw new LinkError('I/O error: stream did not contain valid UTF-8');
  }
}

/** A transitions log in one text file, one entry per line. */
export class FileTransitionLog {
  constructor(path, fd) {
    this.filePath = path;
    this.fd = fd;
    this.syncOnAppendEnabled = true;
    this.tailMayBeTorn = false;
  }

  /** Opens (creating if needed) the log at `path`, discarding a torn tail. */
  static open(path) {
    return io(() => {
      const parent = dirname(path);
      if (!existsSync(parent)) mkdirSync(parent, { recursive: true });
      repairFile(path, existsSync(path) ? 'r+' : 'w+', false);
      const fd = openSync(path, 'a+');
      try {
        syncPath(parentDirectory(path));
      } catch (error) {
        closeSync(fd);
        throw error;
      }
      return new FileTransitionLog(path, fd);
    });
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

  /** Releases the file descriptor (the Rust log closes it on drop). */
  close() {
    closeSync(this.fd);
  }

  supportsDurableFlush() {
    return true;
  }

  // An error may follow successful short writes. Keep the flag until both
  // truncation and its durable flush succeed, so repeated I/O faults retry.
  repairIfNeeded() {
    if (this.tailMayBeTorn) {
      io(() => repairFile(this.filePath, 'r+', true));
      this.tailMayBeTorn = false;
    }
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
