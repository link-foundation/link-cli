// Internal read-only bounded discovery device; never creates files or repairs.
import fs from 'node:fs';
export const parts = name => [name, name + '.meta0', name + '.meta1'];
function readExactly(fd, length, position) {
  const bytes = new Uint8Array(length);
  for (let offset = 0; offset < length;) {
    const count = fs.readSync(fd, bytes, offset, length - offset, position + offset);
    if (count === 0) throw Error('truncated native LSM image');
    offset += count;
  }
  return bytes;
}
export function throwCleanup(errors, message = 'native LSM cleanup failed; retain outer fence') {
  if (errors.length) throw new AggregateError(errors, message);
}
export class ReadOnlyLsmDevice {
  #files = []; #closed = false; #failure;
  constructor(path) {
    try {
      for (const name of parts(path)) {
        let fd;
        // Bitwise OR treats an unavailable O_NOFOLLOW as zero on Windows.
        try { fd = fs.openSync(name, fs.constants.O_RDONLY | fs.constants.O_NOFOLLOW); }
        catch (error) { if (error.code !== 'ENOENT') throw error; }
        this.#files.push(fd);
        if (fd !== undefined && !fs.fstatSync(fd).isFile()) throw Error('nonregular native LSM image');
      }
    } catch (cause) {
      try { this.close(); } catch (cleanup) { throw new AggregateError([cause, cleanup], 'native LSM read-only acquisition and cleanup failed'); }
      throw cause;
    }
  }
  #check() { if (this.#closed) throw Error('native LSM discovery device is closed'); }
  get supportsDurableFlush() { return true; }
  get dataLength() { this.#check(); return this.#files[0] === undefined ? 0 : fs.fstatSync(this.#files[0]).size; }
  readMetadata(slot) {
    this.#check(); const fd = this.#files[slot + 1];
    if (fd === undefined) return undefined;
    const size = fs.fstatSync(fd).size;
    return size === 0 ? undefined : size === 128 ? readExactly(fd, 128, 0) : new Uint8Array(1);
  }
  readPage(index) {
    this.#check();
    if (this.#files[0] === undefined) throw Error('missing selected native LSM data');
    return readExactly(this.#files[0], 4096, index * 4096);
  }
  newDatabaseId() { throw Error('read-only LSM identity allocation'); }
  writeMetadata() { throw Error('read-only LSM metadata write'); }
  syncMetadata() { throw Error('read-only LSM metadata sync'); }
  writePage() { throw Error('read-only LSM page write'); }
  repairPage() { throw Error('read-only LSM repair'); }
  syncData() { throw Error('read-only LSM data sync'); }
  truncateData() { throw Error('read-only LSM truncation'); }
  completeRecovery() {}
  close() {
    if (this.#closed) { if (this.#failure) throw this.#failure; return; }
    this.#closed = true; const errors = [];
    // Cache uncertainty: an OS close error must never trigger a raw-fd retry.
    for (const fd of this.#files.slice().reverse()) if (fd !== undefined) {
      try { fs.closeSync(fd); } catch (error) { errors.push(error); }
    }
    if (errors.length) { this.#failure = new AggregateError(errors, 'native LSM read-only cleanup failed'); throw this.#failure; }
  }
}
export const deferRepair = device => new Proxy(device, { get(target, name) {
  if (name === 'completeRecovery') return () => {};
  const value = Reflect.get(target, name, target);
  return typeof value === 'function' ? value.bind(target) : value;
} });
export function readOnlyWal(records) {
  const text = JSON.stringify(records);
  return { supportsReplacement: true, read: () => text,
    append() { throw Error('read-only WAL append'); }, flush() { throw Error('read-only WAL flush'); },
    replace() { throw Error('read-only WAL replacement'); } };
}
