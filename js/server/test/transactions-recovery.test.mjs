// Journal validation, crash recovery, retention, the commit checkpoint and
// transaction ids of the transactions decorator:
// rust/src/transactions/journal_validation.rs, recovery.rs and commit.rs,
// with the crash and fault cases of rust/tests/transactions_recovery_tests.rs.
import assert from 'node:assert/strict';
import {
  chmodSync,
  existsSync,
  mkdirSync,
  readdirSync,
  readFileSync,
  writeFileSync,
} from 'node:fs';
import { join } from 'node:path';
import { describe, it } from 'node:test';
import { LinkStorage } from '../src/link-storage.js';
import { joinPath, newTransactionId } from '../src/transactions-recovery.js';
import {
  APPLIED_MARKER_PREFIX,
  COMMIT_MARKER_PREFIX,
  hex32,
  parseJournal,
  ROLLBACK_MARKER_PREFIX,
  serializeRecord,
  TRANSITION_NAME_PREFIX,
  TransactionsDecorator,
} from '../src/transactions.js';
import { capture, literally, NO_POSIX_PERMISSIONS, scratchDirectory } from './helpers/storage.mjs';
import { entriesWith, faulty, memoryLog, namedMemory, openTx } from './helpers/transactions.mjs';

const T = TRANSITION_NAME_PREFIX;
const C = COMMIT_MARKER_PREFIX;
const R = ROLLBACK_MARKER_PREFIX;
const A = APPLIED_MARKER_PREFIX;
const O = (n) => hex32(BigInt(n));
const rows = (tx) => tx.allLinks().map(({ index, source, target }) => [index, source, target]);
const sized = (maxTransitions) => ({ kind: 'sized', maxTransitions });
const chunked = (chunkSize, archiveDirectory) => ({ kind: 'chunked', chunkSize, archiveDirectory });
const U64_MASK = (1n << 64n) - 1n;

/** Runs `body` with `Date.now` fixed at `ms`. */
function atTime(ms, body) {
  const real = Date.now;
  Date.now = () => ms;
  try {
    return body();
  } finally {
    Date.now = real;
  }
}

describe('transaction ids', () => {
  // Runs first: only the first id of a process can be zero.
  it('skips a zero id and previously decided ids', () => {
    const errors = capture();
    const tx = openTx({ trace: true, stderr: errors });
    atTime(0, () => {
      const first = tx.beginTransaction().id;
      tx.rollback();
      if (first === 1n) assert.match(errors.text, /Skipped previously decided transaction IDs\./);
      const next = newTransactionId() & U64_MASK;
      tx.committed.add(next + 1n);
      tx.rolledBack.add(next + 2n);
      errors.text = '';
      assert.equal(tx.beginTransaction().id, next + 3n);
      tx.rollback();
    });
    assert.match(errors.text, /^\[Transactions\] Skipped previously decided transaction IDs\.$/m);
  });
});

describe('parseJournal', () => {
  const invalid = [
    [[`${T}v3|undo|${O(1)}|0||`, `${T}v3|undo|${O(1)}|1||`], 'conflicting write-ahead undo images'],
    [
      [`${T}v1|${O(0)}|1|0|0|0,0,0|1,1,1`],
      'transaction records require a nonzero owner and positive sequence',
    ],
    [
      [`${T}v1|${O(1)}|0|0|0|0,0,0|1,1,1`],
      'transaction records require a nonzero owner and positive sequence',
    ],
    [
      [`${T}v1|${O(1)}|1|0|0|0,0,0|1,1,1`, `${T}v1|${O(1)}|1|0|0|0,0,0|2,2,2`],
      'conflicting transaction records for sequence 1',
    ],
    [[`${R}${O(1)}`, `${C}${O(1)}`], `transaction ${O(1)} has both commit and rollback decisions`],
    [[`${C}${O(1)}`, `${R}${O(1)}`], `transaction ${O(1)} has both commit and rollback decisions`],
    [[`${C}x`], 'invalid commit marker'],
    [[`${C}${O(0)}`], 'invalid commit marker'],
    [[`${R}x`], 'invalid rollback marker'],
    [[`${A}x`], 'invalid applied marker'],
    [[`${A}0`], 'nonpositive applied sequence'],
    [
      [`${T}v1|${O(1)}|1|0|0|0,0,0|1,1,1`, `${T}v3|undo|${O(1)}|1||`],
      "undo image follows its owner's writes",
    ],
    [
      [`${T}v3|undo|${O(1)}|0||`, `${T}v3|undo|${O(2)}|0||`],
      'ambiguous pending undo images at the same base',
    ],
  ];

  it('rejects malformed and contradictory journals', () => {
    for (const [entries, message] of invalid) {
      assert.throws(() => parseJournal(entries), { message: `Invalid link format: ${message}` });
    }
  });

  it('collapses equal entries, ignores other names and reserves sequences', () => {
    const record = `${T}v1|${O(1)}|2|0|0|0,0,0|1,1,1`;
    const journal = parseJournal([
      'unrelated',
      `${T}v1|${O(4)}|4|0|0|0,0,0|4,4,4`,
      `${T}v3|undo|${O(1)}|1||-`,
      `${T}v3|undo|${O(1)}|1||-`,
      record,
      record,
      `${T}v3|undo|${O(2)}|1||`,
      `${C}${O(1)}`,
      `${A}5`,
      `${A}3`,
      `${T}v3|undo|${O(3)}|7||`,
    ]);
    assert.deepEqual(
      journal.log.map((value) => value.sequence),
      [2n, 4n],
    );
    assert.deepEqual(journal.committed, new Set([1n]));
    assert.deepEqual(journal.applied, new Set([5n, 3n]));
    assert.equal(journal.appliedSequence, 5n);
    assert.equal(journal.sequence, 7n);
    assert.equal(journal.images.length, 3);
  });
});

describe('recovery', () => {
  it('redoes committed records that were never applied', () => {
    const log = memoryLog();
    const tx = openTx({ log });
    tx.create(1, 1);
    tx.create(2, 2);
    const entries = log.entries.filter((entry) => !entry.startsWith(A));
    const store = LinkStorage.inMemory();
    const recovered = new TransactionsDecorator(store, memoryLog(entries));
    assert.deepEqual(rows(store), [
      [1, 1, 1],
      [2, 2, 2],
    ]);
    assert.deepEqual(entries.slice(-2), [`${A}1`, `${A}2`]);
    assert.equal(recovered.appliedSequence(), 2n);
  });

  it('rolls back records that were never decided', () => {
    const log = memoryLog();
    const tx = openTx({ log });
    tx.beginTransaction();
    tx.create(1, 1);
    tx.create(2, 2);
    const owner = tx.current.id;
    const recovered = new TransactionsDecorator(tx.innerStore, memoryLog([...log.entries]));
    assert.deepEqual(rows(recovered), []);
    assert.equal(recovered.isTransactionRolledBack(owner), true);
    assert.deepEqual(entriesWith(recovered.logStore(), R), [`${R}${hex32(owner)}`]);
  });

  it('undoes pending images and records newest first', () => {
    const store = LinkStorage.inMemory();
    for (let i = 1; i <= 3; i++) store.create(i, i);
    const log = memoryLog([
      `${T}v1|${O(1)}|1|0|0|0,0,0|1,1,1`,
      `${T}v3|undo|${O(2)}|1|1,1,1|-`,
      `${T}v1|${O(2)}|2|0|0|0,0,0|2,2,2`,
      `${T}v1|${O(3)}|3|0|0|0,0,0|3,3,3`,
      `${T}v3|undo|${O(4)}|3||-`,
      `${C}${O(4)}`,
      `${T}v3|undo|${O(5)}|0||-`,
    ]);
    new TransactionsDecorator(store, log);
    assert.deepEqual(rows(store), []);
    assert.deepEqual(entriesWith(log, R), [
      `${R}${O(3)}`,
      `${R}${O(2)}`,
      `${R}${O(1)}`,
      `${R}${O(5)}`,
    ]);
  });

  it('redoes committed records after a pending image base', () => {
    const store = LinkStorage.inMemory();
    store.create(1, 1);
    const log = memoryLog([
      `${T}v3|undo|${O(2)}|0||-`,
      `${T}v1|${O(1)}|1|0|0|0,0,0|1,1,1`,
      `${C}${O(1)}`,
      `${A}1`,
    ]);
    new TransactionsDecorator(store, log);
    assert.deepEqual(rows(store), [[1, 1, 1]]);
    assert.deepEqual(log.entries.slice(-1), [`${R}${O(2)}`]);
  });

  it('requires a named adapter for images with names', () => {
    assert.throws(() => openTx({ log: memoryLog([`${T}v3|undo|${O(1)}|0||`]) }), {
      message: 'Storage error: undo image requires a named storage adapter',
    });
  });

  it('keeps admission fenced until a retried recovery succeeds', () => {
    const errors = capture();
    const log = memoryLog();
    const tx = openTx({ log, trace: true, stderr: errors });
    for (const finish of ['commit', 'rollback', 'flush', 'recover']) {
      log.entries.push(`${C}x`);
      assert.throws(() => tx.recover(), { message: 'Invalid link format: invalid commit marker' });
      assert.equal(tx.hasOpenTransaction(), true);
      log.entries.pop();
      tx[finish]();
      assert.equal(tx.hasOpenTransaction(), false);
    }
    assert.match(
      errors.text,
      /^\[Transactions\] Recovery failed; admission retained: Invalid link format: invalid commit marker\.$/m,
    );
    tx.beginVersionReplay();
    assert.throws(() => tx.recover(), {
      message: 'Transaction error: Cannot recover while a transaction is open.',
    });
    assert.equal(tx.hasOpenTransaction(), true);
    tx.finishVersionReplay();
  });
});

describe('retention', () => {
  it('drops rolled back and applied heads, tracing each', () => {
    const errors = capture();
    const tx = openTx({ retention: sized(1n), trace: true, stderr: errors });
    tx.beginTransaction();
    tx.create(1, 1);
    tx.create(2, 2);
    tx.rollback();
    tx.create(3, 3);
    assert.equal(tx.records.length, 1);
    assert.match(errors.text, /Dropped completed transition seq=1 per sized retention\./);
    assert.match(errors.text, /Dropped completed transition seq=2 per sized retention\./);
  });

  it('drops rolled back chunks without an archive', () => {
    const directory = join(scratchDirectory(), 'archive');
    const tx = openTx({ retention: chunked(2n, directory) });
    tx.beginTransaction();
    tx.create(1, 1);
    tx.create(2, 2);
    tx.rollback();
    assert.equal(tx.records.length, 0);
    assert.equal(existsSync(directory), false);
  });

  it('archives committed chunks and skips paths already owned', () => {
    const directory = scratchDirectory();
    const errors = capture();
    const tx = openTx({ retention: chunked(1n, directory), trace: true, stderr: errors });
    const taken = atTime(1000, () => {
      const counter = newTransactionId() & U64_MASK;
      const path = join(
        directory,
        `transitions-chunk-1000-${hex32((1000n << 64n) | (counter + 2n))}.log`,
      );
      writeFileSync(path, '');
      tx.create(1, 1);
      return path;
    });
    assert.match(errors.text, new RegExp(`Archive entry already owned: ${literally(taken)}\\.`));
    const archive = join(
      directory,
      readdirSync(directory).find((name) => join(directory, name) !== taken),
    );
    assert.match(errors.text, new RegExp(`Archived 1 transitions to ${literally(archive)}\\.`));
    assert.equal(readFileSync(archive, 'utf8'), `${serializeRecord(archived(tx))}\n`);
    assert.equal(tx.records.length, 0);
  });

  it('joins archive names as Path::join does on each platform', () => {
    const cases = [
      [['d', 'n', 'linux'], 'd/n'],
      [['d/', 'n', 'linux'], 'd/n'],
      [['', 'n', 'linux'], 'n'],
      [['d\\', 'n', 'linux'], 'd\\/n'],
      [['C:', 'n', 'linux'], 'C:/n'],
      [['d', 'n', 'win32'], 'd\\n'],
      [['d/', 'n', 'win32'], 'd/n'],
      [['d\\', 'n', 'win32'], 'd\\n'],
      [['C:', 'n', 'win32'], 'C:n'],
      [['C:\\', 'n', 'win32'], 'C:\\n'],
    ];
    for (const [args, joined] of cases) assert.equal(joinPath(...args), joined, args.join(' '));
    assert.equal(joinPath('d', 'n'), process.platform === 'win32' ? 'd\\n' : 'd/n');
  });

  it('joins archive names onto a directory with a trailing slash', () => {
    const directory = scratchDirectory();
    const tx = openTx({ retention: chunked(1n, `${directory}/`) });
    tx.create(1, 1);
    assert.equal(readdirSync(directory).length, 1);
  });

  it('reports an archive file that cannot be created', { skip: NO_POSIX_PERMISSIONS }, () => {
    const directory = scratchDirectory();
    mkdirSync(directory, { recursive: true });
    chmodSync(directory, 0o500);
    try {
      const tx = openTx();
      tx.create(1, 1);
      tx.setRetentionPolicy(chunked(1n, directory));
      assert.throws(() => tx.enforceRetention(), {
        message: new RegExp(
          `^Storage error: failed to create archive file ${directory}/transitions-chunk-\\d+-[0-9a-f]{32}\\.log: Permission denied \\(os error 13\\)$`,
        ),
      });
      assert.equal(tx.records.length, 1);
    } finally {
      chmodSync(directory, 0o700);
    }
  });
});

/** The first record of `tx`'s committed history, recovered from its journal. */
function archived(tx) {
  return parseJournal(tx.logStore().entries).log[0];
}

describe('commit checkpoint', () => {
  it('retries a commit whose marker append failed', () => {
    const log = faulty(memoryLog());
    const tx = openTx({ log });
    const { id } = tx.beginTransaction();
    tx.create(1, 1);
    log.fail('appendLogEntry');
    assert.throws(() => tx.commit(), { message: 'injected appendLogEntry' });
    assert.equal(tx.mustFinishCommit(id), true);
    assert.throws(() => tx.create(2, 2), {
      message:
        'Transaction error: Retry transaction commit before further writes; its outcome may be unknown.',
    });
    tx.flush();
    assert.equal(tx.isTransactionCommitted(id), true);
    assert.equal(entriesWith(log, C).length, 1);
    assert.equal(tx.mustFinishCommit(id), true);
    assert.equal(tx.mustFinishCommit(id + 1n), false);
  });

  it('does not repeat a marker that reached the journal', () => {
    const log = faulty(memoryLog());
    const tx = openTx({ log });
    const { id } = tx.beginTransaction();
    tx.create(1, 1);
    log.fail('flushLog');
    assert.throws(() => tx.commit(), { message: 'injected flushLog' });
    tx.rollback();
    assert.equal(tx.isTransactionCommitted(id), true);
    assert.equal(entriesWith(log, C).length, 1);
    assert.deepEqual(rows(tx), [[1, 1, 1]]);
  });

  it('keeps the owner when the checkpoint fails', () => {
    const log = faulty(memoryLog());
    const tx = openTx({ log });
    tx.beginTransaction();
    tx.create(1, 1);
    log.fail('flushLog', { after: 1 });
    assert.throws(() => tx.commit(), { message: 'injected flushLog' });
    assert.notEqual(tx.current, null);
    tx.commit();
    assert.equal(tx.current, null);
    assert.equal(entriesWith(log, C).length, 1);
    assert.deepEqual(entriesWith(log, A), [`${A}1`]);
  });

  it('refuses to commit a rollback-only owner', () => {
    const log = faulty(memoryLog());
    const tx = openTx({ log });
    tx.beginTransaction();
    log.fail('appendLogEntry');
    assert.throws(() => tx.create(1, 1), { message: 'injected appendLogEntry' });
    assert.throws(() => tx.create(1, 1), {
      message: 'Transaction error: Transaction requires rollback before further writes.',
    });
    assert.throws(() => tx.commit(), {
      message:
        'Transaction error: Cannot commit a rollback-only transaction; rollback is required.',
    });
    tx.rollback();
    assert.deepEqual(rows(tx), []);
  });

  it('flushes a named store and its names', () => {
    const tx = openTx({ store: namedMemory() });
    tx.beginTransaction();
    tx.create(1, 1);
    tx.commit();
    tx.flush();
    assert.deepEqual(rows(tx), [[1, 1, 1]]);
  });
});
