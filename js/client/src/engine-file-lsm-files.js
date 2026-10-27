// Internal exclusive file namespace shared by native LSM discovery and opening.
import fs from 'node:fs';
import { resolve, join } from 'node:path';
import process from 'node:process';
import { FilePageDevice } from './engine-file-paged-log.js';
import { MirroredPageDevice } from './engine-mirrored-pages.js';
import { MirroredCheckpointRoot } from './engine-checkpoint-root.js';
import { parts, ReadOnlyLsmDevice, deferRepair, throwCleanup } from './engine-file-lsm-readonly.js';

const maximum = 0xffffffffffffffffn;
export class FileLsmFiles {
  #directory; #options; #owner; #closed = false; #uncertain = false;
  #raw = []; #readers = []; #owned = new Set(); #leases = new Map();
  constructor(directory, options) {
    this.#directory = resolve(directory); this.#options = options;
    const entry = fs.lstatSync(this.#directory);
    if (!entry.isDirectory() || entry.isSymbolicLink()) throw Error('native LSM directory must be regular');
    this.#owner = fs.openSync(this.#path('.lsm-writer'), 'wx', 0o600);
  }
  #path(name) { return join(this.#directory, name); }
  #walBases() {
    const names = ['wal']; if (this.#options.mirroredRoot) names.push('wal.mirror');
    for (const index of [0, 1]) {
      names.push(`wal.bank${index}`);
      if (this.#options.mirroredBanks) names.push(`wal.bank${index}.mirror`);
    }
    return names;
  }
  list(bound) {
    const permitted = new Set(['.lsm-writer', ...this.#walBases().flatMap(parts)]), ids = new Set();
    const max = permitted.size + this.#options.maxNamespaceEntries * (this.#options.mirroredTables ? 6 : 3) + this.#owned.size;
    let directory;
    try { directory = fs.opendirSync(this.#directory); }
    catch (error) { this.#uncertain = true; throw error; }
    const errors = []; let count = 0;
    try {
      for (let entry; (entry = directory.readSync()) !== null;) {
        if (++count > max) throw Error('native LSM enumeration bound');
        const name = entry.name;
        if (!entry.isFile() || fs.lstatSync(this.#path(name)).isSymbolicLink()) throw Error('foreign or symbolic native LSM entry');
        if (this.#owned.has(name) || permitted.has(name)) continue;
        const match = /^table-([1-9][0-9]{0,19})(\.mirror)?(?:\.meta[01])?$/.exec(name);
        if (!match || BigInt(match[1]) > maximum || (match[2] && !this.#options.mirroredTables)) throw Error('foreign native LSM entry or owner');
        ids.add(match[1]);
        if (ids.size > this.#options.maxNamespaceEntries) throw Error('native LSM namespace bound');
      }
    } catch (error) { errors.push(error); }
    try { directory.closeSync(); } catch (error) { this.#uncertain = true; errors.push(error); }
    throwCleanup(errors, 'native LSM discovery or directory cleanup failed');
    const result = [...ids].sort((a, b) => BigInt(a) < BigInt(b) ? -1 : 1);
    if (result.length > bound) throw Error('native LSM namespace bound');
    return result;
  }
  #read(base) {
    try {
      const device = new ReadOnlyLsmDevice(this.#path(base)); this.#readers.push(device); return device;
    } catch (error) { this.#uncertain = true; throw error; }
  }
  #acquire(base) {
    try {
      const device = new FilePageDevice(this.#path(base));
      this.#raw.push({ device, base, closed: false, failure: undefined });
      this.#owned.add(base + '.writer'); return device;
    } catch (error) { this.#uncertain = true; throw error; }
  }
  wal(readonly) {
    const read = readonly ? base => this.#read(base) : base => this.#acquire(base);
    const primary = read('wal');
    const root = this.#options.mirroredRoot ? new MirroredCheckpointRoot(primary, read('wal.mirror'), this.#options.mirroredBanks ? 1 : 0) : primary;
    const banks = [0, 1].map(index => {
      const base = `wal.bank${index}`, first = read(base);
      return this.#options.mirroredBanks ? new MirroredPageDevice(first, read(base + '.mirror')) : first;
    });
    return { root, banks };
  }
  #table(id, read) {
    const base = 'table-' + id, first = read(base);
    return this.#options.mirroredTables ? new MirroredPageDevice(first, read(base + '.mirror')) : first;
  }
  provider(readonly) {
    return readonly ? {
      supportsDurableFlush: false, list: bound => this.list(bound),
      open: id => deferRepair(this.#table(id, base => this.#read(base))),
      create() { throw Error('read-only LSM table create'); }, close() {}, delete() {},
    } : {
      // File flush alone does not prove persistence of Windows directory entries.
      supportsDurableFlush: process.platform !== 'win32', list: bound => this.list(bound),
      open: id => this.#openTable(id), create: id => this.#createTable(id),
      close: device => this.#closeTable(device), delete: id => this.#deleteTable(id),
    };
  }
  closeReaders() {
    const errors = [];
    for (const device of this.#readers.slice().reverse()) try { device.close(); } catch (error) { errors.push(error); }
    this.#readers = []; if (errors.length) this.#uncertain = true; throwCleanup(errors);
  }
  #openTable(id) {
    const before = this.#raw.length, device = this.#table(id, base => this.#acquire(base));
    this.#leases.set(device, this.#raw.slice(before)); return device;
  }
  #createTable(id) {
    const base = 'table-' + id, bases = this.#options.mirroredTables ? [base, base + '.mirror'] : [base];
    for (const name of bases.flatMap(parts)) if (fs.existsSync(this.#path(name))) throw Error('occupied native LSM table identity');
    return this.#openTable(id);
  }
  #closeRecord(record) {
    if (record.closed) { if (record.failure) throw record.failure; return; }
    record.closed = true;
    try { record.device.close(); this.#owned.delete(record.base + '.writer'); }
    catch (error) { record.failure = error; this.#uncertain = true; throw error; }
  }
  #closeTable(device) {
    const errors = [];
    for (const record of this.#leases.get(device).slice().reverse()) try { this.#closeRecord(record); } catch (error) { errors.push(error); }
    throwCleanup(errors); this.#leases.delete(device);
    // Keep live leases bounded across arbitrarily many flush/compact cycles.
    this.#raw = this.#raw.filter(record => !record.closed);
  }
  #deleteTable(id) {
    const base = 'table-' + id, bases = this.#options.mirroredTables ? [base, base + '.mirror'] : [base];
    for (const name of bases.flatMap(parts)) try { fs.unlinkSync(this.#path(name)); } catch (error) { if (error.code !== 'ENOENT') throw error; }
    if (process.platform !== 'win32') {
      let fd;
      try { fd = fs.openSync(this.#directory, 'r'); } catch (error) { this.#uncertain = true; throw error; }
      const errors = []; try { fs.fsyncSync(fd); } catch (error) { errors.push(error); }
      try { fs.closeSync(fd); } catch (error) { this.#uncertain = true; errors.push(error); }
      throwCleanup(errors, 'native LSM directory synchronization or cleanup failed');
    }
  }
  close(errors = []) {
    if (this.#closed) { if (this.#uncertain) throw Error('native LSM remains fenced'); return; }
    this.#closed = true;
    try { this.closeReaders(); } catch (error) { errors.push(error); }
    for (const record of this.#raw.slice().reverse()) try { this.#closeRecord(record); } catch (error) { errors.push(error); }
    try { fs.closeSync(this.#owner); } catch (error) { this.#uncertain = true; errors.push(error); }
    if (errors.length === 0 && !this.#uncertain) try { fs.unlinkSync(this.#path('.lsm-writer')); } catch (error) { this.#uncertain = true; errors.push(error); }
    if (errors.length) this.#uncertain = true;
    if (this.#uncertain && errors.length === 0) errors.push(Error('native LSM ownership remains uncertain'));
    throwCleanup(errors);
  }
}
