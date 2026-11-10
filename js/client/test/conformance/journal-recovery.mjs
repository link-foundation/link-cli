// Damaged journals through the actual WebAssembly engine: every flipped byte
// is found, a file journal is repaired to its valid prefix or from a replica,
// and copies that disagree are refused. Twin of Rust's engine_recovery_tests.
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { MemoryCommitLog, WasmEngine } from '../../src/index.js';
import {
  FileCommitLog,
  readJournalFileLossy,
  repairJournalFile,
} from '../../src/engine-file-log.js';

// Revision k holds links 1..=k, each pointing at itself; `selfLinks: false`
// leaves them pointing at nothing, a history that diverges from the first.
async function journal(binding, commits, options = {}, { selfLinks = true } = {}) {
  const engine = new WasmEngine(binding, options);
  try {
    for (let commit = 0; commit < commits; commit++) {
      const tx = engine.begin();
      const link = await tx.create(0n, 0n);
      if (selfLinks) await tx.update(link.index, link.index, link.index);
      await tx.commit();
      await tx.close();
    }
    return engine.journal();
  } finally {
    engine.close();
  }
}

function assertState(binding, journalOrLog, revision) {
  const log = Array.isArray(journalOrLog) ? new MemoryCommitLog(journalOrLog) : journalOrLog;
  const engine = new WasmEngine(binding, {}, log);
  try {
    const snapshot = engine.snapshot();
    assert.equal(snapshot.revision, revision);
    assert.deepEqual(
      snapshot.rows.map((row) => [row.index, row.source, row.target]),
      Array.from({ length: Number(revision) }, (_, i) => {
        const id = BigInt(i + 1);
        return [id, id, id];
      }),
    );
  } finally {
    engine.close();
  }
}

export async function journalRecoveryConformance(binding) {
  for (const revisionEncoding of ['snapshot', 'changes']) {
    const entries = await journal(binding, 3, { revisionEncoding });
    const intact = WasmEngine.checkJournal(binding, entries);
    assert.deepEqual(intact, { validRecords: 3, totalRecords: 3, revision: 3n, damage: null });
    for (let record = 0; record < entries.length; record++) {
      // Every bit that keeps the ASCII record ASCII, including the one that
      // only changes the case of a checksum letter.
      for (let flip = 0; flip < entries[record].length * 7; flip++) {
        const position = Math.floor(flip / 7);
        const mask = 1 << (flip % 7);
        const damaged = [...entries];
        const text = damaged[record];
        damaged[record] =
          text.slice(0, position) +
          String.fromCharCode(text.charCodeAt(position) ^ mask) +
          text.slice(position + 1);
        const check = WasmEngine.checkJournal(binding, damaged);
        assert.equal(
          check.damage?.record,
          record,
          `${revisionEncoding} flip ${mask} at ${record}:${position}`,
        );
        assert.equal(check.validRecords, record);
        assert.equal(check.revision, BigInt(record));
        const recovery = WasmEngine.recoverJournal(binding, [damaged]);
        assertState(binding, recovery.entries, BigInt(record));
      }
    }
  }
  const strict = WasmEngine.checkJournal(binding, await journal(binding, 1), {
    constraints: ['references'],
  });
  assert.equal(strict.validRecords, 1, 'self references satisfy the constraint');
  assert.throws(() => WasmEngine.recoverJournal(binding, []), /at least one copy/);
  const self = await journal(binding, 2);
  const unlinked = await journal(binding, 2, {}, { selfLinks: false });
  assert.throws(() => WasmEngine.recoverJournal(binding, [self, unlinked]), /diverge/);
  const longer = WasmEngine.recoverJournal(binding, [self.slice(0, 1), self]);
  assert.equal(longer.source, 1);
  assert.equal(longer.revision, 2n);
  assert.equal(longer.checks.length, 2);

  const directory = mkdtempSync(join(tmpdir(), 'journal-recovery-'));
  const recover = (journals) => WasmEngine.recoverJournal(binding, journals);
  try {
    const path = join(directory, 'links.journal');
    const replica = join(directory, 'replica.journal');
    const entries = await journal(binding, 4, { revisionEncoding: 'changes' });
    const text = entries.map((entry) => entry + '\n').join('');
    writeFileSync(path, text);
    writeFileSync(replica, text);
    const pristine = readFileSync(path);
    // Every byte of the primary: repair alone keeps the valid prefix.
    for (let position = 0; position < pristine.length; position++) {
      const bytes = Uint8Array.from(pristine);
      bytes[position] = bytes[position] === 0xff ? 0 : 0xff;
      writeFileSync(path, bytes);
      const repair = repairJournalFile(path, recover);
      const line = BigInt(pristine.subarray(0, position).filter((byte) => byte === 10).length);
      assert.equal(repair.recovery.revision, line, `damage at ${position}`);
      assert.equal(repair.changed, true);
      assert.deepEqual(readFileSync(repair.preserved), Buffer.from(bytes));
      rmSync(repair.preserved);
      using log = new FileCommitLog(path);
      assertState(binding, log, line);
    }
    // A replica restores the lost suffix; the evidence is kept and never overwritten.
    writeFileSync(path, 'garbage\n');
    const restored = repairJournalFile(path, recover, { copies: [replica] });
    assert.equal(restored.recovery.source, 1);
    assert.equal(restored.recovery.revision, 4n);
    assert.equal(restored.preserved, path + '.damaged-1');
    writeFileSync(path, 'garbage again\n');
    assert.equal(
      repairJournalFile(path, recover, { copies: [replica] }).preserved,
      path + '.damaged-2',
    );
    assert.equal(readFileSync(path + '.damaged-1', 'utf8'), 'garbage\n');
    const again = repairJournalFile(path, recover, { copies: [replica] });
    assert.equal(again.changed, false);
    assert.equal(again.preserved, null);
    {
      using log = new FileCommitLog(path);
      assertState(binding, log, 4n);
    }
    // Invalid UTF-8 inside a complete record is damage, not a read failure.
    const bytes = readFileSync(path);
    bytes[entries[0].length + 3] = 0xc3;
    writeFileSync(path, bytes);
    assert.equal(readJournalFileLossy(path).length, 4);
    assert.equal(repairJournalFile(path, recover).recovery.revision, 1n);
    // A copy that disagrees leaves the journal as it was.
    const other = join(directory, 'other.journal');
    writeFileSync(other, unlinked.map((entry) => entry + '\n').join(''));
    const before = readFileSync(path);
    assert.throws(() => repairJournalFile(path, recover, { copies: [other] }), /diverge/);
    assert.deepEqual(readFileSync(path), before);
    // A running writer blocks repair.
    {
      using writer = new FileCommitLog(path);
      assert.throws(() => repairJournalFile(path, recover), /busy/);
      assert.equal(writer.read(), JSON.stringify(entries.slice(0, 1)), 'the writer is unaffected');
    }
    assert.equal(existsSync(path + '.writer'), false);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
}
