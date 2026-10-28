// Native file device; SQL-free and deliberately separate from root imports.
import fs from 'node:fs';
import { resolve, dirname } from 'node:path';
import { randomBytes } from 'node:crypto';
import process from 'node:process';
import { PagedCommitLog } from './engine-paged-log.js';

function readExactly(fd, length, position) {
  const bytes = new Uint8Array(length);
  let offset = 0;
  while (offset < length) {
    const count = fs.readSync(fd, bytes, offset, length - offset, position + offset);
    if (count === 0) throw new Error('native page file was truncated');
    offset += count;
  }
  return bytes;
}
function writeExactly(fd, bytes, position) {
  let offset = 0;
  while (offset < bytes.length) {
    const count = fs.writeSync(fd, bytes, offset, bytes.length - offset, position + offset);
    if (count === 0) throw new Error('native page write made no progress');
    offset += count;
  }
}
export class FilePageDevice {
  #files = [];
  #lock;
  #owner;
  #closed = false;
  constructor(path) {
    path = resolve(path); this.#lock = path + '.writer';
    // The parent directory must already exist. Never steal a crashed owner.
    this.#owner = fs.openSync(this.#lock, 'wx', 0o600);
    try {
      for (const file of [path, path + '.meta0', path + '.meta1'])
        this.#files.push(fs.openSync(file, fs.constants.O_RDWR | fs.constants.O_CREAT, 0o600));
      if (process.platform !== 'win32') {
        const directory = fs.openSync(dirname(path), 'r');
        const errors = [];
        try { fs.fsyncSync(directory); } catch (error) { errors.push(error); }
        try { fs.closeSync(directory); } catch (error) { errors.push(error); }
        if (errors.length === 1) throw errors[0];
        if (errors.length > 1) throw new AggregateError(errors, 'native page directory synchronization and cleanup failed');
      }
    } catch (cause) {
      try { this.close(); }
      catch (cleanup) { throw new AggregateError([cause, cleanup], 'native page initialization and cleanup failed'); }
      throw cause;
    }
  }
  #check() { if (this.#closed) throw new Error('native page device is closed'); }
  get supportsDurableFlush() { return true; }
  get dataLength() { this.#check(); return fs.fstatSync(this.#files[0]).size; }
  newDatabaseId() { this.#check(); return new Uint8Array(randomBytes(16)); }
  readMetadata(slot) {
    this.#check(); const file = this.#files[slot + 1], length = fs.fstatSync(file).size;
    if (length === 0) return undefined;
    // Do not allocate according to a malformed or unbounded metadata file.
    return length === 128 ? readExactly(file, 128, 0) : new Uint8Array(1);
  }
  writeMetadata(slot, bytes) {
    this.#check(); const file = this.#files[slot + 1];
    writeExactly(file, bytes, 0); fs.ftruncateSync(file, bytes.length);
  }
  syncMetadata(slot) { this.#check(); fs.fsyncSync(this.#files[slot + 1]); }
  readPage(index) { this.#check(); return readExactly(this.#files[0], 4096, index * 4096); }
  writePage(index, bytes) {
    this.#check();
    if (fs.fstatSync(this.#files[0]).size !== index * 4096) throw new Error('native page file changed during ownership');
    writeExactly(this.#files[0], bytes, index * 4096);
  }
  // Used only by a selected repair decorator after complete logical validation.
  repairPage(index, bytes) {
    this.#check();
    if (!Number.isSafeInteger(index) || index < 0 || index >= Math.floor(Number.MAX_SAFE_INTEGER / 4096)
      || !(bytes instanceof Uint8Array) || bytes.length !== 4096) throw new RangeError('invalid repair page');
    if (fs.fstatSync(this.#files[0]).size < index * 4096) throw new Error('repair cannot create a page hole');
    writeExactly(this.#files[0], bytes, index * 4096);
  }
  syncData() { this.#check(); fs.fsyncSync(this.#files[0]); }
  truncateData(length) { this.#check(); fs.ftruncateSync(this.#files[0], length); }
  close() {
    if (this.#closed) return;
    this.#closed = true;
    const failures = [];
    for (const file of this.#files.reverse()) {
      try { fs.closeSync(file); } catch (error) { failures.push(error); }
    }
    try { fs.closeSync(this.#owner); } catch (error) { failures.push(error); }
    // A close error has an uncertain handle outcome. Retain ownership until
    // external recovery fences this process; do not admit another writer.
    if (failures.length === 0) {
      try { fs.unlinkSync(this.#lock); } catch (error) { failures.push(error); }
    }
    if (failures.length) throw new AggregateError(failures, 'native page device cleanup failed');
  }
  [Symbol.dispose]() { this.close(); }
}
export class FilePagedCommitLog extends PagedCommitLog {
  #device;
  #closed = false;
  constructor(path, options) {
    const device = new FilePageDevice(path);
    try { super(device, options); }
    catch (cause) {
      try { device.close(); }
      catch (cleanup) { throw new AggregateError([cause, cleanup], 'native page log initialization and cleanup failed'); }
      throw cause;
    }
    this.#device = device;
  }
  #check() { if (this.#closed) throw new Error('native page log is closed'); }
  read() { this.#check(); return super.read(); }
  append(record) { this.#check(); super.append(record); }
  flush() { this.#check(); super.flush(); }
  close() { if (!this.#closed) { this.#closed = true; this.#device.close(); } }
  [Symbol.dispose]() { this.close(); }
}
