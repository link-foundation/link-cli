// The file side of journal repair with a stand-in for the WebAssembly
// recovery; test/conformance/journal-recovery.mjs runs it with the real engine.
import assert from 'node:assert/strict';
import test from 'node:test';
import fs, {
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { WasmEngine } from '../src/index.js';
import { readJournalFileLossy, repairJournalFile } from '../src/engine-file-log.js';

// Keeps the records up to the first one named "bad" of the longest copy.
const recover = (journals) => {
  const prefixes = journals.map((journal) => {
    const bad = journal.indexOf('bad');
    return bad === -1 ? journal : journal.slice(0, bad);
  });
  let source = 0;
  prefixes.forEach((prefix, index) => {
    if (prefix.length > prefixes[source].length) source = index;
  });
  return { entries: prefixes[source], source, revision: BigInt(prefixes[source].length) };
};

function scratch(context) {
  const directory = mkdtempSync(join(tmpdir(), 'journal-repair-'));
  context.after(() => rmSync(directory, { recursive: true, force: true }));
  return { directory, path: join(directory, 'journal') };
}

function mock(context, name, implementation) {
  const mocked = context.mock.method(fs, name, implementation);
  syncBuiltinESMExports();
  context.after(() => {
    mocked.mock.restore();
    syncBuiltinESMExports();
  });
  return mocked;
}

test('lossy reading keeps complete lines and marks invalid UTF-8', (context) => {
  const { directory, path } = scratch(context);
  assert.deepEqual(readJournalFileLossy(path), []);
  writeFileSync(path, Uint8Array.from([0x61, 10, 10, 0xff, 0x62, 10, 0x63]));
  assert.deepEqual(readJournalFileLossy(path), ['a', '�b']);
  assert.throws(() => readJournalFileLossy(directory), /EISDIR/);
  // Trailing lines zeroed by power loss are torn; an earlier one is damage.
  writeFileSync(path, 'a\0\nb\nc\0\n\n\0\0\n\0');
  assert.deepEqual(readJournalFileLossy(path), ['a\0', 'b']);
});

test('repair keeps the valid prefix, preserves every original and reports no-ops', (context) => {
  const { path } = scratch(context);
  assert.throws(() => repairJournalFile(path), /recover must be a function/);
  const missing = repairJournalFile(path, recover);
  assert.deepEqual(missing, {
    recovery: { entries: [], source: 0, revision: 0n },
    changed: false,
    preserved: null,
  });
  assert.equal(existsSync(path), false);

  writeFileSync(path, 'one\ntwo\nbad\nlost\ntorn');
  const first = repairJournalFile(path, recover);
  assert.equal(first.changed, true);
  assert.equal(first.preserved, path + '.damaged-1');
  assert.equal(readFileSync(path, 'utf8'), 'one\ntwo\n');
  assert.equal(readFileSync(first.preserved, 'utf8'), 'one\ntwo\nbad\nlost\ntorn');

  writeFileSync(path, 'one\nbad\n');
  assert.equal(repairJournalFile(path, recover).preserved, path + '.damaged-2');
  assert.equal(readFileSync(path + '.damaged-1', 'utf8'), 'one\ntwo\nbad\nlost\ntorn');

  const intact = repairJournalFile(path, recover);
  assert.deepEqual(intact, {
    recovery: { entries: ['one'], source: 0, revision: 1n },
    changed: false,
    preserved: null,
  });
  // Same length, different bytes, is still a change.
  writeFileSync(path, 'bad\n');
  assert.equal(repairJournalFile(path, recover).changed, true);
  assert.equal(readFileSync(path, 'utf8'), '');
  assert.equal(existsSync(path + '.repair'), false);
  assert.equal(existsSync(path + '.writer'), false);
});

test('repair restores a missing or damaged journal from its copies', (context) => {
  const { directory, path } = scratch(context);
  const replica = join(directory, 'replica');
  writeFileSync(replica, 'one\ntwo\nthree\n');
  const restored = repairJournalFile(path, recover, { copies: [join(directory, 'none'), replica] });
  assert.equal(restored.recovery.source, 2);
  assert.equal(restored.preserved, null, 'nothing to keep from a missing file');
  assert.equal(readFileSync(path, 'utf8'), 'one\ntwo\nthree\n');
  let seen;
  repairJournalFile(path, (journals) => ((seen = journals), recover(journals)), {
    copies: [replica],
  });
  assert.deepEqual(seen, [
    ['one', 'two', 'three'],
    ['one', 'two', 'three'],
  ]);
});

test('repair refuses a busy journal and leaves it untouched when recovery fails', (context) => {
  const { directory, path } = scratch(context);
  writeFileSync(path, 'one\nbad\n');
  mkdirSync(path + '.writer');
  assert.throws(() => repairJournalFile(path, recover), /journal writer is busy/);
  rmSync(path + '.writer', { recursive: true });
  assert.throws(() => repairJournalFile(join(directory, 'absent', 'journal'), recover), /ENOENT/);
  assert.throws(
    () =>
      repairJournalFile(path, () => {
        throw new Error('copies diverge');
      }),
    /diverge/,
  );
  assert.equal(readFileSync(path, 'utf8'), 'one\nbad\n');
  assert.equal(existsSync(path + '.writer'), false);
  assert.equal(existsSync(path + '.damaged-1'), false);
});

test('repair writes through short writes and stops on failures', (context) => {
  const { path } = scratch(context);
  const original = fs.writeSync;
  const writes = mock(context, 'writeSync', (fd, bytes, offset, length) =>
    original(fd, bytes, offset, Math.min(length, 2)),
  );
  writeFileSync(path, 'one\ntwo\nbad\n');
  repairJournalFile(path, recover);
  assert.ok(writes.mock.callCount() > 4);
  assert.equal(readFileSync(path, 'utf8'), 'one\ntwo\n');
  assert.equal(readFileSync(path + '.damaged-1', 'utf8'), 'one\ntwo\nbad\n');

  writes.mock.mockImplementation(() => 0);
  writeFileSync(path, 'one\nbad\n');
  assert.throws(() => repairJournalFile(path, recover), /no progress/);
  writes.mock.mockImplementation((fd, bytes, offset, length) => {
    // Preservation succeeds; the replacement fails.
    if (fs.fstatSync(fd).size === 0 && length === 4) return 0;
    return original(fd, bytes, offset, length);
  });
  writeFileSync(path, 'one\nbad\n');
  assert.throws(() => repairJournalFile(path, recover), /no progress/);
  assert.equal(readFileSync(path, 'utf8'), 'one\nbad\n', 'the journal is replaced only by rename');
  assert.equal(existsSync(path + '.writer'), false);
});

test('repair surfaces errors other than an existing preserved copy', (context) => {
  const { path } = scratch(context);
  const original = fs.openSync;
  mock(context, 'openSync', (target, flags, ...rest) => {
    if (flags === 'wx') throw Object.assign(new Error('EACCES: denied'), { code: 'EACCES' });
    return original(target, flags, ...rest);
  });
  writeFileSync(path, 'one\nbad\n');
  assert.throws(() => repairJournalFile(path, recover), /EACCES/);
  assert.equal(readFileSync(path, 'utf8'), 'one\nbad\n');
  assert.equal(existsSync(path + '.writer'), false);
});

test('WasmEngine.checkJournal and recoverJournal translate the binding', () => {
  for (const binding of [undefined, {}, { StagedEngine: {} }]) {
    assert.throws(() => WasmEngine.checkJournal(binding, []), /checkJournal support/);
    assert.throws(() => WasmEngine.recoverJournal(binding, [[]]), /recoverJournal support/);
  }
  const calls = [];
  const check = {
    validRecords: 1,
    totalRecords: 2,
    revision: '1',
    damage: { record: 1, reason: 'x' },
  };
  const binding = {
    StagedEngine: {
      checkJournal: (...args) => (calls.push(args), JSON.stringify(check)),
      recoverJournal: (...args) => (
        calls.push(args),
        JSON.stringify({
          entries: ['a'],
          source: 1,
          revision: '18446744073709551615',
          checks: [check],
        })
      ),
    },
  };
  assert.deepEqual(WasmEngine.checkJournal(binding, ['a', 'b'], { constraints: ['references'] }), {
    ...check,
    revision: 1n,
  });
  assert.deepEqual(WasmEngine.recoverJournal(binding, [['a'], ['a', 'b']]), {
    entries: ['a'],
    source: 1,
    revision: 18446744073709551615n,
    checks: [{ ...check, revision: 1n }],
  });
  assert.deepEqual(calls, [
    ['{"constraints":["references"]}', '["a","b"]'],
    ['{}', '[["a"],["a","b"]]'],
  ]);
  assert.throws(() => WasmEngine.checkJournal(binding, [], { lockWaitMs: 1 }), /pessimistic/);
});
