import assert from 'node:assert/strict';
import fs from 'node:fs';
import { tmpdir } from 'node:os';
import { join, sep } from 'node:path';
import { syncBuiltinESMExports } from 'node:module';
import { createHash } from 'node:crypto';
import { selected } from './lsm-store.mjs';
import { FileLsmJournalStore } from '../../src/engine-file-lsm-store.js';

export const options = { ...selected, maxPages: 32 };
export const causes = error => error instanceof AggregateError ? error.errors.flatMap(causes) : [error.message];
export const put = (store, key, entries) => assert.equal(store.compareExchange([key], store.read([key]), [entries]), true);
export const files = directory => fs.readdirSync(directory).filter(name => !name.endsWith('.writer') && name !== '.lsm-writer').sort();
export const snapshot = directory => Object.fromEntries(files(directory).map(name => [name, createHash('sha256').update(fs.readFileSync(join(directory, name))).digest('hex')]));
export function run(body) {
  const directory = fs.mkdtempSync(join(tmpdir(), 'clink-file-lsm-'));
  try { return body(directory); } finally { fs.rmSync(directory, { recursive: true, force: true }); }
}
export function seed(directory, modes = {}) {
  const store = new FileLsmJournalStore(directory, { ...options, ...modes });
  put(store, 'a', ['old\0雪😀']); put(store, 'b', []); store.flush();
  put(store, 'a', ['selected']); store.flush(); put(store, 'c', ['buffered']);
  const expected = store.scanNamespace(); store.close(); return expected;
}
// Track lifetimes rather than fd numbers: the OS may reuse a closed descriptor.
export function observe(context, directory, fault = () => {}) {
  const open = fs.openSync, close = fs.closeSync, active = new Map(), opened = [], closed = [];
  const mocks = [context.mock.method(fs, 'openSync', (name, ...args) => {
    const event = { operation: 'open', name: String(name), flags: args[0] }; fault(event, false);
    const fd = open(name, ...args), token = { fd, name: String(name), index: opened.length };
    if (token.name === directory || token.name.startsWith(directory + sep)) { active.set(fd, token); opened.push(token); }
    fault(event, true); return fd;
  }), context.mock.method(fs, 'closeSync', fd => {
    const token = active.get(fd), event = { operation: 'close', name: token?.name, token };
    closed.push(token); fault(event, false); close(fd); active.delete(fd); fault(event, true);
  })];
  syncBuiltinESMExports();
  return { opened, closed, active, restore() {
    for (const mock of mocks) mock.mock.restore(); syncBuiltinESMExports();
  }, recover() { for (const fd of active.keys()) close(fd); active.clear(); } };
}
