// Node/Bun/Deno filesystem adapter. The universal entry point never imports it.
import { openSync, closeSync, readSync, writeSync, fstatSync, ftruncateSync, fsyncSync, mkdirSync, rmdirSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import process from 'node:process';

const decode = new TextDecoder('utf-8', { fatal: true });
const encode = new TextEncoder();

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
 * The caller owns this adapter and closes it after all engine sessions. */
export class FileCommitLog {
  #fd;
  #lock;
  #closed = false;
  constructor(path) {
    path = resolve(path);
    const directory = dirname(path);
    mkdirSync(directory, { recursive: true });
    this.#lock = path + '.writer';
    mkdirSync(this.#lock); // Fails fast if another writer or a stale lock exists.
    try {
      this.#fd = openSync(path, 'a+');
      const buffer = new Uint8Array(8192);
      let end = fstatSync(this.#fd).size;
      let complete = 0;
      while (end > 0) {
        const length = Math.min(end, buffer.length);
        readExactly(this.#fd, buffer, length, end - length);
        const offset = buffer.subarray(0, length).lastIndexOf(10);
        if (offset >= 0) { complete = end - length + offset + 1; break; }
        end -= length;
      }
      if (complete !== fstatSync(this.#fd).size) ftruncateSync(this.#fd, complete);
      fsyncSync(this.#fd);
      // POSIX directory sync establishes a new journal's name before commits.
      if (process.platform !== 'win32') {
        const dir = openSync(directory, 'r');
        try { fsyncSync(dir); } finally { closeSync(dir); }
      }
    } catch (error) {
      if (this.#fd !== undefined) closeSync(this.#fd);
      rmdirSync(this.#lock);
      throw error;
    }
  }
  get supportsDurableFlush() { return true; }
  #check() { if (this.#closed) throw new Error('journal is closed'); }
  read() {
    this.#check();
    const length = fstatSync(this.#fd).size;
    const bytes = new Uint8Array(length);
    readExactly(this.#fd, bytes, length, 0);
    const text = decode.decode(bytes);
    return JSON.stringify(text === '' ? [] : text.slice(0, -1).split('\n'));
  }
  append(record) {
    this.#check();
    if (typeof record !== 'string' || /[\r\n]/.test(record)) throw new TypeError('journal records must be strings without line breaks');
    const bytes = encode.encode(record + '\n');
    let offset = 0;
    while (offset < bytes.length) {
      const count = writeSync(this.#fd, bytes, offset, bytes.length - offset);
      if (count === 0) throw new Error('journal write made no progress');
      offset += count;
    }
  }
  flush() { this.#check(); fsyncSync(this.#fd); }
  close() {
    if (!this.#closed) {
      this.#closed = true;
      try { closeSync(this.#fd); } finally { rmdirSync(this.#lock); }
    }
  }
  [Symbol.dispose]() { this.close(); }
}
