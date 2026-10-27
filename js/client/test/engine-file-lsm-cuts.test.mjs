// Every before/after cut in one finite observed OS-operation trace; no power-loss claim.
import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import { basename, join } from 'node:path';
import { syncBuiltinESMExports } from 'node:module';
import { FileLsmJournalStore } from '../src/engine-file-lsm-store.js';
import { options, causes, put, run, seed } from './support/file-lsm.mjs';

function harness(context) {
  const originals = Object.fromEntries(['openSync', 'closeSync', 'writeSync', 'ftruncateSync', 'fsyncSync', 'unlinkSync'].map(name => [name, fs[name]]));
  const control = { active: new Map(), trace: [], armed: false, fault: undefined, hit: false };
  const effect = (operation, path, action) => {
    if (!control.armed) return action();
    control.trace.push({ operation, name: basename(path) });
    const hit = control.trace.length === control.fault?.at;
    if (hit) control.hit = true;
    if (hit && !control.fault.after) throw Error('native mutation cut');
    const value = action(); if (hit) throw Error('native mutation cut'); return value;
  };
  const mocks = [context.mock.method(fs, 'openSync', (path, flags, ...args) => {
    const open = () => { const fd = originals.openSync(path, flags, ...args); control.active.set(fd, String(path)); return fd; };
    return (typeof flags === 'number' && (flags & fs.constants.O_CREAT)) || (typeof flags === 'string' && flags.includes('w'))
      ? effect('create', String(path), open) : open();
  }), context.mock.method(fs, 'closeSync', fd => effect('close', control.active.get(fd), () => {
    originals.closeSync(fd); control.active.delete(fd);
  })), ...['writeSync', 'ftruncateSync', 'fsyncSync'].map(operation => context.mock.method(fs, operation,
    (fd, ...args) => effect(operation, control.active.get(fd), () => originals[operation](fd, ...args)))),
  context.mock.method(fs, 'unlinkSync', path => effect('unlinkSync', String(path), () => originals.unlinkSync(path)))];
  syncBuiltinESMExports();
  control.reset = fault => { control.trace = []; control.hit = false; control.fault = fault; control.armed = true; };
  control.release = () => {
    control.armed = false; for (const fd of control.active.keys()) originals.closeSync(fd); control.active.clear();
  };
  control.restore = () => { for (const mock of mocks) mock.mock.restore(); syncBuiltinESMExports(); };
  return control;
}
function recoverMarkers(directory) {
  for (const name of fs.readdirSync(directory)) if (name === '.lsm-writer' || name.endsWith('.writer')) fs.unlinkSync(join(directory, name));
}
const createdIds = trace => trace.filter(event => event.operation === 'create' && /^table-[0-9]+\.writer$/.test(event.name))
  .map(event => BigInt(event.name.slice(6, -7)));

// Eight finite groups keep each test within the existing 30-second suite limit.
for (let group = 0; group < 8; group++) test(`native OS compaction cuts ${group + 1}/8 preserve acknowledged state and reservations`, context => {
  const control = harness(context);
  try {
    let operations;
    run(directory => {
      seed(directory); const store = new FileLsmJournalStore(directory, options);
      control.reset(); store.compact(); operations = control.trace.slice(); control.armed = false; store.close();
    });
    assert.ok(operations.length > 30 && operations.length <= 192, 'finite observed operation bound');
    for (const operation of ['create', 'writeSync', 'ftruncateSync', 'fsyncSync', 'close', 'unlinkSync'])
      assert.ok(operations.some(event => event.operation === operation), operation);
    for (let at = group + 1; at <= operations.length; at += 8) for (const after of [false, true]) run(directory => {
      seed(directory); const store = new FileLsmJournalStore(directory, options), expected = store.read(['a', 'b', 'c']);
      control.reset({ at, after });
      assert.throws(() => store.compact(), error => causes(error).includes('native mutation cut'), `${at}/${after}/${JSON.stringify(operations[at - 1])}`);
      assert.equal(control.hit, true); const count = control.trace.length, attempted = createdIds(control.trace);
      assert.throws(() => store.read(['a']), /fenced/); assert.equal(control.trace.length, count);
      control.armed = false;
      try { store.close(); } catch (error) { assert.match(String(error), /cleanup|fenced/); }
      control.release(); recoverMarkers(directory); // Explicit external recovery only after every test handle is fenced.
      const reopened = new FileLsmJournalStore(directory, options);
      assert.deepEqual(reopened.read(['a', 'b', 'c']), expected, `${at}/${after}`);
      control.reset(); put(reopened, 'd', ['following']); reopened.flush();
      for (const next of createdIds(control.trace)) for (const previous of attempted) assert.ok(next > previous, 'reserved names are never reused');
      control.armed = false; reopened.close(); assert.equal(control.active.size, 0);
    });
    console.log(`native compaction group ${group + 1}: ${operations.length} observed OS operations; before/after cuts modulo eight`);
  } finally { control.release(); control.restore(); }
});
