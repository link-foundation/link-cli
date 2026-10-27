import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import { join } from 'node:path';
import { syncBuiltinESMExports } from 'node:module';
import { FileLsmJournalStore } from '../src/engine-file-lsm-store.js';
import { FileLsmFiles } from '../src/engine-file-lsm-files.js';
import { options, causes, put, run, seed, observe, snapshot } from './support/file-lsm.mjs';

test('discovery cleanup failure closes every other lease once and retains the outer fence', context => {
  for (const after of [false, true]) run(directory => {
    seed(directory); const before = snapshot(directory); let hit = false;
    const trace = observe(context, directory, (event, phase) => {
      if (!hit && event.operation === 'close' && event.name?.endsWith('.meta1') && phase === after) {
        hit = true; throw Error('injected discovery cleanup');
      }
    });
    try {
      assert.throws(() => new FileLsmJournalStore(directory, options), error => causes(error).includes('injected discovery cleanup'));
      assert.equal(hit, true); assert.equal(new Set(trace.closed).size, trace.opened.length);
      assert.equal(trace.closed.length, trace.opened.length); assert.equal(trace.active.size, after ? 0 : 1);
      assert.deepEqual(snapshot(directory), before);
      assert.throws(() => new FileLsmJournalStore(directory, options), /EEXIST/);
    } finally { trace.restore(); trace.recover(); }
    // The harness has fenced all its handles; only now may it remove evidence.
    fs.unlinkSync(join(directory, '.lsm-writer'));
    const reopened = new FileLsmJournalStore(directory, options); reopened.close();
  });
});

test('child cleanup uncertainty is cached while every remaining raw owner is released', context => {
  for (const after of [false, true]) run(directory => {
    let enabled = false, hit = false;
    const trace = observe(context, directory, (event, phase) => {
      if (enabled && !hit && event.operation === 'close' && event.name?.endsWith('.meta1') && phase === after) {
        hit = true; throw Error('injected child cleanup');
      }
    });
    try {
      const store = new FileLsmJournalStore(directory, { ...options, mirroredRoot: true, mirroredBanks: true, mirroredTables: true });
      put(store, 'a', ['acknowledged']); store.flush(); enabled = true;
      const start = trace.closed.length;
      assert.throws(() => store.close(), error => causes(error).includes('injected child cleanup'));
      const attempts = trace.closed.slice(start); assert.equal(new Set(attempts).size, attempts.length);
      assert.equal(hit, true); assert.equal(trace.active.size, after ? 0 : 1);
      assert.throws(() => store.close(), /fenced/); assert.equal(trace.closed.length, start + attempts.length);
      assert.throws(() => store.read(['a']), /closed/); assert.throws(() => new FileLsmJournalStore(directory, options), /EEXIST/);
    } finally { trace.restore(); trace.recover(); }
  });
});

test('lost acquisition replies retain the directory fence before and after an actual child open', context => {
  for (const after of [false, true]) run(directory => {
    let hit = false;
    const trace = observe(context, directory, (event, phase) => {
      if (!hit && event.operation === 'open' && event.name === join(directory, 'wal.bank0') && typeof event.flags === 'number'
        && (event.flags & fs.constants.O_CREAT) && phase === after) { hit = true; throw Error('child open reply lost'); }
    });
    try {
      assert.throws(() => new FileLsmJournalStore(directory, options), error => causes(error).includes('child open reply lost'));
      assert.equal(hit, true); assert.equal(trace.active.size, after ? 1 : 0);
      assert.throws(() => new FileLsmJournalStore(directory, options), /EEXIST/);
    } finally { trace.restore(); trace.recover(); }
  });
});

test('directory acquisition and close uncertainty retain ownership and preserve every cause', context => {
  for (const stage of ['open', 'read', 'close', 'read-and-close']) run(directory => {
    const open = fs.opendirSync; let live, closed = false;
    const mock = context.mock.method(fs, 'opendirSync', (...args) => {
      if (stage === 'open') throw Error('directory acquisition');
      live = open(...args);
      return { readSync() { if (stage.includes('read')) throw Error('directory read'); return live.readSync(); },
        closeSync() { live.closeSync(); closed = true; if (stage.includes('close')) throw Error('directory close'); } };
    });
    syncBuiltinESMExports();
    try {
      assert.throws(() => new FileLsmJournalStore(directory, options), error => {
        const messages = causes(error);
        if (stage.includes('read')) assert.ok(messages.includes('directory read'));
        if (stage.includes('close')) assert.ok(messages.includes('directory close'));
        return true;
      });
      assert.equal(closed, stage !== 'open');
      assert.equal(fs.existsSync(join(directory, '.lsm-writer')), stage !== 'read');
    } finally { mock.mock.restore(); syncBuiltinESMExports(); }
  });
});

test('native namespace bounds and occupied identities reject without overwriting files', () => run(directory => {
  const files = new FileLsmFiles(directory, { ...options, maxNamespaceEntries: 2, mirroredTables: true });
  const provider = files.provider(false), readonly = files.provider(true);
  try {
    assert.throws(() => readonly.create('1'), /read-only/); readonly.close(); readonly.delete();
    fs.writeFileSync(join(directory, 'table-1.mirror.meta0'), 'incomplete');
    assert.deepEqual(provider.list(2), ['1']); assert.throws(() => provider.list(0), /namespace bound/);
    assert.throws(() => provider.create('1'), /occupied/);
    for (const name of ['table-2', 'table-3']) fs.writeFileSync(join(directory, name), '');
    assert.throws(() => provider.list(2), error => causes(error).some(message => /namespace bound/.test(message)));
    fs.unlinkSync(join(directory, 'table-3'));
    for (let index = 0; index < 30; index++) fs.writeFileSync(join(directory, 'foreign-' + index), '');
    assert.throws(() => provider.list(2), error => causes(error).some(message => /foreign|enumeration bound/.test(message)));
  } finally { files.close(); }
}));

test('enumeration stops at the finite physical entry bound even if entries repeat', context => run(directory => {
  const files = new FileLsmFiles(directory, { ...options, maxNamespaceEntries: 2 });
  const open = fs.opendirSync; let reads = 0, closed = false;
  const mock = context.mock.method(fs, 'opendirSync', (...args) => {
    const directory = open(...args);
    return { readSync() { reads++; return { name: '.lsm-writer', isFile: () => true }; },
      closeSync() { closed = true; directory.closeSync(); } };
  });
  syncBuiltinESMExports();
  try {
    assert.throws(() => files.list(2), error => causes(error).some(message => /enumeration bound/.test(message)));
    assert.equal(reads, 17); assert.equal(closed, true);
  } finally { mock.mock.restore(); syncBuiltinESMExports(); files.close(); }
}));

test('default native facade owns delete, scan, disposal and closed-operation behavior', () => run(directory => {
  const store = new FileLsmJournalStore(directory); put(store, 'a', ['first']); put(store, 'b', []);
  assert.equal(store.compareDelete('a', store.read(['a'])[0].generation), true);
  assert.deepEqual(store.scanNamespace({ keysOnly: true }).map(row => row.key), ['b']);
  store.flush(); store[Symbol.dispose](); store.close();
  for (const method of ['read', 'compareExchange', 'compareDelete', 'scanNamespace', 'flush', 'compact'])
    assert.throws(() => store[method](), /closed/);
  assert.throws(() => store.supportsDurableFlush, /closed/);
  const reopened = new FileLsmJournalStore(directory); assert.deepEqual(reopened.scanNamespace().map(row => row.key), ['b']); reopened.close();
}));

test('a finite physical WAL bound rejects without acknowledging a group', () => run(directory => {
  const store = new FileLsmJournalStore(directory, { ...options, maxPages: 1 }), before = snapshot(directory);
  assert.throws(() => put(store, 'a', ['requires another page']), /page|limit|admission/);
  assert.deepEqual(snapshot(directory), before); assert.throws(() => store.read(['a']), /fenced/); store.close();
  const reopened = new FileLsmJournalStore(directory, { ...options, maxPages: 1 });
  assert.equal(reopened.read(['a'])[0].generation, '0'); reopened.close();
}));

test('native root must be an existing regular directory rather than a file or symlink', () => run(directory => {
  const file = join(directory, 'file'), link = join(directory, 'link'); fs.writeFileSync(file, '');
  assert.throws(() => new FileLsmJournalStore(file, options), /directory must be regular/);
  fs.mkdirSync(join(directory, 'real')); fs.symlinkSync(join(directory, 'real'), link, 'dir');
  assert.throws(() => new FileLsmJournalStore(link, options), /directory must be regular/);
}));

test('failed read-only acquisition and cleanup preserve both causes and fence the namespace', context => run(directory => {
  seed(directory); const open = fs.openSync, close = fs.closeSync; let hit = false;
  const mocks = [context.mock.method(fs, 'openSync', (name, ...args) => {
    if (name === join(directory, 'wal.bank0')) { hit = true; throw Error('read-only acquisition'); }
    return open(name, ...args);
  }), context.mock.method(fs, 'closeSync', fd => { close(fd); if (hit) throw Error('acquisition cleanup'); })];
  syncBuiltinESMExports();
  try {
    assert.throws(() => new FileLsmJournalStore(directory, options), error => {
      const messages = causes(error); assert.ok(messages.includes('read-only acquisition'));
      assert.ok(messages.includes('acquisition cleanup')); return true;
    });
    assert.ok(fs.existsSync(join(directory, '.lsm-writer')));
  } finally { for (const mock of mocks) mock.mock.restore(); syncBuiltinESMExports(); }
}));

test('outer owner close is attempted once and retains its marker on an uncertain reply', context => {
  for (const after of [false, true]) run(directory => {
    let enabled = false, hits = 0;
    const trace = observe(context, directory, (event, phase) => {
      if (enabled && event.operation === 'close' && event.name === join(directory, '.lsm-writer') && phase === after) {
        hits++; throw Error('outer close uncertainty');
      }
    });
    try {
      const store = new FileLsmJournalStore(directory, options); enabled = true;
      assert.throws(() => store.close(), error => causes(error).includes('outer close uncertainty'));
      assert.equal(hits, 1); assert.ok(fs.existsSync(join(directory, '.lsm-writer')));
      assert.throws(() => store.close(), /fenced/); assert.equal(hits, 1);
    } finally { trace.restore(); trace.recover(); }
  });
});

test('outer marker removal errors occur only after every child has closed', context => {
  for (const after of [false, true]) run(directory => {
    const store = new FileLsmJournalStore(directory, options), unlink = fs.unlinkSync;
    const mock = context.mock.method(fs, 'unlinkSync', path => {
      if (path === join(directory, '.lsm-writer')) { if (after) unlink(path); throw Error('outer unlink reply'); }
      return unlink(path);
    });
    syncBuiltinESMExports();
    try {
      assert.throws(() => store.close(), error => causes(error).includes('outer unlink reply'));
      assert.equal(fs.existsSync(join(directory, '.lsm-writer')), !after);
      assert.throws(() => store.close(), /fenced/);
    } finally { mock.mock.restore(); syncBuiltinESMExports(); }
  });
});

test('directory sync acquisition uncertainty retains the outer fence after table retirement', context => run(directory => {
  if (process.platform === 'win32') return;
  const files = new FileLsmFiles(directory, { ...options, maxNamespaceEntries: 2 }), open = fs.openSync;
  const mock = context.mock.method(fs, 'openSync', (path, ...args) => {
    if (path === directory) throw Error('directory sync acquisition'); return open(path, ...args);
  });
  syncBuiltinESMExports();
  try { assert.throws(() => files.provider(false).delete('1'), /directory sync acquisition/); }
  finally { mock.mock.restore(); syncBuiltinESMExports(); }
  assert.throws(() => files.close(), /cleanup/); assert.ok(fs.existsSync(join(directory, '.lsm-writer')));
}));

test('Windows namespace capability excludes Unix directory synchronization', () => run(directory => {
  const descriptor = Object.getOwnPropertyDescriptor(process, 'platform');
  Object.defineProperty(process, 'platform', { value: 'win32' });
  try {
    const files = new FileLsmFiles(directory, { ...options, maxNamespaceEntries: 2 });
    const provider = files.provider(false); assert.equal(provider.supportsDurableFlush, false);
    provider.delete('1'); files.close();
  } finally { Object.defineProperty(process, 'platform', descriptor); }
}));
