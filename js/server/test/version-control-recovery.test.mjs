// Recovery and failure contracts of the version-control decorator
// (src/version-control.js, src/version-control-movement.js and
// src/version-control-publication.js): the ports of
// rust/tests/version_movement_recovery_tests.rs,
// version_movement_failure_tests.rs, version_publication_failure_tests.rs,
// version_commit_retry_tests.rs, version_automatic_failure_tests.rs and
// version_initial_abort_tests.rs.
//
// The portable recovery of every marker of
// docs/protocol/version-movement-vectors.json and
// docs/protocol/version-publication-vectors.json is replayed here; their
// codecs are checked in version-control.test.mjs.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { capture, traceLines } from './helpers/storage.mjs';
import {
  faulty,
  memoryLog,
  namedMemory,
  openTx,
  protocolVectors,
} from './helpers/transactions.mjs';
import { encodeName, hex32, I64_MAX, serializeRecord } from '../src/transactions-types.js';
import {
  encodeMovement,
  encodePublication,
  VersionControlDecorator,
} from '../src/version-control.js';

const movementVectors = protocolVectors('version-movement-vectors.json');
const publicationVectors = protocolVectors('version-publication-vectors.json');

const MAIN = encodeName('main');
const INCOMPLETE =
  'Version publication is incomplete; retry commit, rollback or save before writing.';

/** The names of a metadata store, in address order. */
const markers = (branches) =>
  branches
    .allLinks()
    .map(({ index }) => branches.getName(index))
    .filter((name) => name !== null);

/** A metadata store holding `names`. */
function metadataWith(names) {
  const branches = namedMemory();
  for (const name of names) branches.setName(branches.tryCreate(0, 0), name);
  return branches;
}

/**
 * `target` behind a proxy whose `overrides[method](original, ...args)`
 * replace its methods; `original` is the bound method of `target`.
 */
function wrap(target, overrides) {
  return new Proxy(target, {
    get(object, key) {
      const value = Reflect.get(object, key, object);
      if (typeof value !== 'function') return value;
      const original = value.bind(object);
      return key in overrides ? (...args) => overrides[key](original, ...args) : original;
    },
  });
}

const open = (tx, branches, trace = false, stderr = capture()) =>
  new VersionControlDecorator(tx, branches, trace, { stderr });

describe('movement recovery (version_movement_recovery_tests.rs)', () => {
  /** `contract(scenario)`: a crash between selecting `original` and moving its rows. */
  function contract(scenario, { trace = false, transactions = (tx) => tx } = {}) {
    const store = namedMemory();
    const log = memoryLog();
    let tx = openTx({ store, log });
    const id = tx.createAndUpdate(11, 12);
    const original = tx.lastLoggedSequence();
    tx.update(id, 21, 22);
    const attempted = tx.lastLoggedSequence();
    const journal = tx.journal();
    const record = journal.at(-1);
    let payload = record;
    if (scenario === 'wrong-payload') {
      payload = { ...record, after: { ...record.after, source: 91 } };
    }
    if (scenario === 'wrong-decision') {
      payload = { ...record, transactionId: record.transactionId + 1n };
    }
    let intent = {
      fromBranch: scenario === 'missing-from' ? 'missing' : 'main',
      from: scenario === 'unreachable-from' ? I64_MAX : attempted,
      toBranch: scenario === 'missing-to' ? 'missing' : 'original',
      to: scenario === 'unreachable-to' ? I64_MAX : original,
      undo: scenario === 'missing-undo' ? [] : [payload],
      redo: scenario === 'extra-redo' ? [record] : [],
    };
    if (scenario === 'to-zero') {
      intent = { ...intent, toBranch: 'main', to: 0n, undo: [...journal].reverse() };
    }
    let marker = encodeMovement(intent);
    if (scenario === 'checksum') {
      marker = marker.slice(0, -1) + (marker.endsWith('0') ? '1' : '0');
    }
    const parent = { 'cyclic-parent': 'original', 'missing-parent': 'missing' }[scenario] ?? '';
    const published = scenario === 'published';
    const selected = scenario === 'wrong-current' ? 'other' : published ? 'original' : 'main';
    const applied =
      { 'wrong-applied': 999n, 'impossible-publication': original, 'negative-applied': -1n }[
        scenario
      ] ?? (published ? original : attempted);
    const names = [
      `__vc:branch:main:parent=${parent}:fork=0:head=${attempted}`,
      `__vc:branch:original:parent=main:fork=${original}:head=${original}`,
      `__vc:branch:other:parent=main:fork=${original}:head=${original}`,
      `__vc:current=${selected}`,
      `__vc:applied=${applied}`,
      marker,
    ];
    if (scenario === 'no-current') names.splice(3, 1);
    if (scenario === 'no-applied') names.splice(4, 1);
    for (let sequence = 1n; sequence <= attempted; sequence++) {
      names.push(`__vc:trans:${sequence}:branch=main`);
    }
    const extra = {
      overlap: encodePublication({
        id: record.transactionId + 1n,
        before: attempted,
        after: attempted,
        branch: 'main',
      }),
      duplicate: encodeMovement({ ...intent, to: original, toBranch: 'main', redo: [] }),
      'duplicate-current': '__vc:current=original',
      'duplicate-applied': `__vc:applied=${original}`,
      'duplicate-branch': `__vc:branch:original:parent=main:fork=0:head=${original}`,
      'duplicate-attribution': `__vc:trans:${attempted}:branch=original`,
      'invalid-branch': '__vc:branch:broken',
      'invalid-applied': '__vc:applied=broken',
      'invalid-attribution': '__vc:trans:broken',
      'unparsable-attribution': '__vc:trans:x:branch=main',
      'zero-attribution': '__vc:trans:0:branch=main',
    }[scenario];
    if (extra !== undefined) names.push(extra);
    const branches = metadataWith(names);
    if (scenario === 'evicted-record') {
      const entry = `__transactions:transition:${serializeRecord(record)}`;
      log.entries = log.entries.filter((value) => value !== entry);
      tx = openTx({ store, log });
      assert.equal(
        tx.log().some((transition) => transition.sequence === attempted),
        false,
      );
    }
    const before = markers(branches);
    const stderr = capture();
    try {
      const vc = open(transactions(tx), branches, trace, stderr);
      return { vc, id, original, attempted, store, branches, stderr };
    } catch (error) {
      assert.deepEqual(markers(branches), before, `changed ${scenario}`);
      assert.equal(store.get(id).source, 21, `changed rows ${scenario}`);
      throw error;
    }
  }

  const rejected = {
    'missing-from': 'Version movement has invalid branch ancestry.',
    'missing-to': 'Version movement has invalid branch ancestry.',
    'cyclic-parent': 'Version movement has invalid branch ancestry.',
    'missing-parent': 'Version movement has invalid branch ancestry.',
    'unreachable-from': 'Version movement intent contradicts selected history.',
    'unreachable-to': 'Version movement intent contradicts selected history.',
    'wrong-current': 'Version movement intent contradicts selected history.',
    'wrong-applied': 'Version movement intent contradicts selected history.',
    'impossible-publication': 'Version movement intent contradicts selected history.',
    'missing-undo': 'Version movement intent contradicts its transition paths.',
    'extra-redo': 'Version movement intent contradicts its transition paths.',
    'wrong-payload': 'Version movement intent contradicts its committed records.',
    'wrong-decision': 'Version movement intent contradicts its committed records.',
    overlap: 'Version publication and movement intents overlap.',
    duplicate: 'Multiple version movement intents.',
    checksum: 'Invalid version movement intent.',
    'duplicate-current': 'Ambiguous version movement metadata.',
    'duplicate-applied': 'Ambiguous version movement metadata.',
    'duplicate-branch': 'Ambiguous version movement metadata.',
    'duplicate-attribution': 'Ambiguous version movement metadata.',
    'invalid-branch': 'Ambiguous version movement metadata.',
    'invalid-applied': 'Ambiguous version movement metadata.',
    'invalid-attribution': 'Ambiguous version movement metadata.',
    'unparsable-attribution': 'Ambiguous version movement metadata.',
    'zero-attribution': 'Ambiguous version movement metadata.',
    'negative-applied': 'Ambiguous version movement metadata.',
    'no-current': 'Ambiguous version movement metadata.',
    'no-applied': 'Ambiguous version movement metadata.',
  };
  for (const [scenario, message] of Object.entries(rejected)) {
    it(`rejects ${scenario} without changing metadata or rows`, () => {
      assert.throws(() => contract(scenario), { message });
    });
  }

  it('rejects a record whose transaction is also rolled back', () => {
    const transactions = (tx) =>
      wrap(tx, { isTransactionRolledBack: (original, id) => id !== 0n || original(id) });
    assert.throws(() => contract('valid', { transactions }), {
      message: 'Version movement intent contradicts its committed records.',
    });
  });

  for (const scenario of ['valid', 'evicted-record', 'published']) {
    it(`finishes a ${scenario} movement and admits writes`, () => {
      const { vc, id, original, attempted, branches, stderr } = contract(scenario, {
        trace: true,
      });
      assert.equal(vc.currentBranch, 'original');
      assert.equal(vc.currentSequence(), original);
      assert.equal(vc.get(id).source, 11);
      assert.equal(vc.transactions.lastLoggedSequence(), attempted);
      assert.equal(
        markers(branches).some((name) => name.startsWith('__vc:move:')),
        false,
      );
      assert.deepEqual(traceLines(stderr), [
        '[VersionControl] Recovering interrupted version movement.',
        '[VersionControl] Completed durable version movement.',
      ]);
      vc.update(id, 31, 32);
      assert.equal(vc.currentSequence(), attempted + 1n);
      vc.save();
    });
  }

  it('finishes a movement back to the empty history', () => {
    const { vc, id } = contract('to-zero');
    assert.equal(vc.currentBranch, 'main');
    assert.equal(vc.currentSequence(), 0n);
    assert.equal(vc.exists(id), false);
  });

  it('validates every shared movement vector during portable recovery', () => {
    for (const [valid, rows] of [
      [true, movementVectors.movements],
      [false, movementVectors.invalidMovements],
      [false, movementVectors.nonUnicodeMovements],
    ]) {
      for (const row of rows) {
        const marker = valid ? row.marker : row;
        // Unknown prefixes are deliberately ignored by sidecar recovery.
        if (!marker.startsWith('__vc:move:')) continue;
        assert.throws(() => open(openTx(), metadataWith([marker])), {
          message: valid
            ? 'Ambiguous version movement metadata.'
            : 'Invalid version movement intent.',
        });
      }
    }
  });
});

describe('publication recovery (version_publication_failure_tests.rs)', () => {
  it('recovers or rejects every shared intent vector without guessing', () => {
    for (const [valid, rows] of [
      [true, publicationVectors.intents],
      [false, publicationVectors.invalidIntents],
    ]) {
      for (const row of rows) {
        const marker = valid ? row.marker : row;
        if (!marker.startsWith('__vc:pending:')) continue;
        const branch = valid ? row.branch : 'main';
        const names = [`__vc:branch:2|${MAIN}|-|0|0`];
        if (branch !== 'main') names.push(`__vc:branch:2|${encodeName(branch)}|${MAIN}|0|0`);
        names.push(`__vc:current=${branch}`, '__vc:applied=0', marker);
        const branches = metadataWith(names);
        const before = markers(branches);
        const stderr = capture();
        if (valid) {
          const vc = open(openTx(), branches, true, stderr);
          assert.equal(vc.currentBranch, branch);
          assert.equal(vc.currentSequence(), 0n);
          assert.deepEqual(vc.all(), []);
          assert.deepEqual(traceLines(stderr), [
            `[VersionControl] Recovered aborted version publication ${row.id}.`,
          ]);
          vc.branch('following');
          vc.save();
          vc.recover();
          assert.equal(
            markers(branches).some((name) => name.startsWith('__vc:pending:')),
            false,
          );
        } else {
          assert.throws(() => open(openTx(), branches), {
            message: 'Invalid version publication intent.',
          });
          assert.deepEqual(markers(branches), before);
        }
      }
    }
  });

  /** Two committed transactions, the second on `topic`, and a scenario's sidecar. */
  function contract(scenario, transactions = (tx) => tx) {
    const store = namedMemory();
    const log = memoryLog();
    const tx = openTx({ store, log });
    tx.beginTransaction();
    const id = tx.createAndUpdate(11, 12);
    tx.commit();
    const original = tx.lastLoggedSequence();
    tx.beginTransaction();
    tx.update(id, 21, 22);
    tx.commit();
    const attempted = tx.lastLoggedSequence();
    const transaction = tx.log().at(-1).transactionId;
    const topic = 'topic|:parent=雪\0';
    const unattributed = scenario === 'unattributed';
    const head = scenario === 'head-ahead' ? attempted + 1n : unattributed ? original : attempted;
    const applied =
      scenario === 'applied-ahead' ? attempted + 1n : unattributed ? original : attempted;
    const names = [
      `__vc:branch:2|${MAIN}|-|0|${original}`,
      `__vc:branch:2|${encodeName(topic)}|${MAIN}|${original}|${head}`,
      `__vc:current=${scenario === 'wrong-current' ? 'main' : topic}`,
      `__vc:applied=${applied}`,
    ];
    for (let sequence = 1n; sequence <= attempted; sequence++) {
      if (sequence > original && unattributed) continue;
      const owner = sequence <= original || scenario === 'foreign-attribution' ? 'main' : topic;
      names.push(`__vc:trans:${sequence}:branch=${owner}`);
    }
    const intent = {
      id: transaction,
      before: scenario === 'foreign-record' ? 0n : original,
      after: scenario === 'missing-record' ? attempted + 1n : attempted,
      branch: scenario === 'unknown-branch' ? 'unknown' : topic,
    };
    names.push(encodePublication(intent));
    if (scenario === 'multiple-intents') {
      names.push(encodePublication({ ...intent, id: transaction + 1n }));
    }
    const branches = metadataWith(names);
    const before = markers(branches);
    const stderr = capture();
    try {
      const vc = open(transactions(tx), branches, true, stderr);
      return { vc, id, attempted, transaction, topic, branches, stderr, log, store };
    } catch (error) {
      assert.deepEqual(markers(branches), before, `changed ${scenario}`);
      assert.equal(store.get(id).source, 21);
      throw error;
    }
  }

  const rejected = {
    'unknown-branch': 'Version publication intent contradicts branch metadata.',
    'wrong-current': 'Version publication intent contradicts branch metadata.',
    'head-ahead': 'Version publication intent contradicts branch metadata.',
    'applied-ahead': 'Version publication intent contradicts branch metadata.',
    'foreign-record': 'Version publication intent contradicts its transition range.',
    'missing-record': 'Version publication intent contradicts its transition range.',
    'foreign-attribution': 'Version publication intent contradicts its transition range.',
    'multiple-intents': 'Multiple version publication intents.',
  };
  for (const [scenario, message] of Object.entries(rejected)) {
    it(`rejects a contradictory ${scenario} intent, preserving metadata and rows`, () => {
      assert.throws(() => contract(scenario), { message });
    });
  }

  it('rejects conflicting decisions in the journal and in the decorator', () => {
    const { log, store, transaction } = contract('valid');
    log.entries.push(`__transactions:rollback:${hex32(transaction)}`);
    assert.throws(() => openTx({ store, log }), {
      message: `Invalid link format: transaction ${hex32(transaction)} has both commit and rollback decisions`,
    });
    // Transaction ids are fresh per run, so every decision reads as a rollback too.
    const transactions = (tx) => wrap(tx, { isTransactionRolledBack: () => true });
    assert.throws(() => contract('valid', transactions), {
      message: 'Version publication has conflicting transaction decisions.',
    });
  });

  for (const scenario of ['valid', 'unattributed']) {
    it(`publishes a committed ${scenario} intent to its branch`, () => {
      const { vc, attempted, transaction, topic, branches, stderr } = contract(scenario);
      assert.equal(vc.currentBranch, topic);
      assert.equal(vc.currentSequence(), attempted);
      assert.equal(vc.branches.get(topic).head, attempted);
      assert.deepEqual(vc.buildBranchSeqs(topic), [1n, 2n, 3n]);
      assert.equal(
        markers(branches).some((name) => name.startsWith('__vc:pending:')),
        false,
      );
      assert.deepEqual(traceLines(stderr), [
        `[VersionControl] Recovered committed version publication ${hex32(transaction)}.`,
      ]);
    });
  }

  it('rejects metadata that names an uncommitted transition', () => {
    const tx = openTx();
    tx.createAndUpdate(11, 12);
    tx.beginTransaction();
    tx.create(0, 0);
    tx.rollback();
    assert.throws(() => open(tx, metadataWith(['__vc:trans:3:branch=main'])), {
      message: 'Version metadata references uncommitted transition 3.',
    });
  });
});

describe('opening over existing history (version_initial_abort_tests.rs)', () => {
  it('versions only committed records and rejects a live owner', () => {
    const tx = openTx();
    tx.createAndUpdate(11, 12);
    tx.beginTransaction();
    tx.create(0, 0);
    tx.rollback();
    tx.beginTransaction();
    assert.throws(() => open(tx, namedMemory()), {
      message: 'recover is not allowed while a version-control transaction is open.',
    });
    tx.rollback();
    const branches = namedMemory();
    const vc = open(tx, branches);
    assert.deepEqual(vc.listBranches(), [{ name: 'main', parent: null, forkSeq: 0n, head: 2n }]);
    assert.equal(vc.currentSequence(), 2n);
    assert.deepEqual(vc.buildBranchSeqs('main'), [1n, 2n]);
    assert.deepEqual(markers(branches), [
      '__vc:trans:1:branch=main',
      '__vc:trans:2:branch=main',
      `__vc:branch:2|${MAIN}|-|0|2`,
      '__vc:current=main',
      '__vc:applied=2',
    ]);
  });
});

describe('publication failures (version_commit_retry_tests.rs)', () => {
  /** A repository whose metadata store fails `save` as `plan` says. */
  function failing(plan) {
    const store = namedMemory();
    const branches = faulty(namedMemory());
    const vc = open(openTx({ store }), branches);
    vc.save();
    branches.fail('save', plan);
    return { vc, branches };
  }

  /** The fences of an incomplete publication. */
  function assertFenced(vc) {
    assert.throws(() => vc.create(1, 1), { message: INCOMPLETE });
    assert.throws(() => vc.reserveQueryAllocations(null, null, null), { message: INCOMPLETE });
    assert.throws(() => vc.beginTransaction(), {
      message: 'Nested version-control transactions are not supported.',
    });
    assert.throws(() => vc.branch('later'), {
      message: 'branch is not allowed while a version-control transaction is open.',
    });
  }

  for (const finish of ['commit', 'rollback', 'save']) {
    it(`fences a publication whose intent was not saved until ${finish}`, () => {
      const { vc, branches } = failing({});
      assert.throws(() => vc.create(1, 2), { message: 'injected save' });
      assertFenced(vc);
      branches.heal();
      vc[finish]();
      assert.deepEqual(
        vc.all().map(({ source, target }) => [source, target]),
        finish === 'rollback' ? [] : [[1, 2]],
      );
      assert.equal(vc.currentSequence(), finish === 'rollback' ? 0n : 1n);
      assert.equal(
        markers(branches).some((name) => name.startsWith('__vc:pending:')),
        false,
      );
      assert.equal(vc.create(3, 4), finish === 'rollback' ? 1 : 2);
    });
  }

  for (const finish of ['commit', 'rollback', 'save']) {
    it(`completes a decided publication on ${finish}`, () => {
      const { vc, branches } = failing({ after: 1 });
      assert.throws(() => vc.create(1, 2), { message: 'injected save' });
      assertFenced(vc);
      vc[finish]();
      assert.deepEqual(vc.buildBranchSeqs('main'), [1n]);
      assert.deepEqual(markers(branches).slice(-3), [
        '__vc:current=main',
        '__vc:applied=1',
        '__vc:trans:1:branch=main',
      ]);
      assert.equal(
        markers(branches).some((name) => name.startsWith('__vc:pending:')),
        false,
      );
    });
  }

  it('retries clearing an intent whose removal was not saved', () => {
    const { vc, branches } = failing({ after: 2 });
    assert.throws(() => vc.create(1, 2), { message: 'injected save' });
    assertFenced(vc);
    vc.commit();
    assert.equal(
      markers(branches).some((name) => name.startsWith('__vc:pending:')),
      false,
    );
    assert.equal(vc.create(3, 4), 2);
    assert.deepEqual(vc.buildBranchSeqs('main'), [1n, 2n]);
  });

  it('undoes the write when the intent link cannot be created', () => {
    const { vc, branches } = failing({ times: 0 });
    branches.fail('tryCreate');
    vc.beginTransaction();
    vc.create(1, 2);
    assert.throws(() => vc.commit(), { message: 'injected tryCreate' });
    vc.rollback();
    assert.deepEqual(vc.all(), []);
    assert.equal(vc.currentSequence(), 0n);
    vc.save();
  });

  it('lets the store veto a commit, publishing nothing', () => {
    const tx = openTx();
    const vc = open(tx, namedMemory());
    vc.beginTransaction();
    vc.create(1, 2);
    tx.rollback();
    vc.commit();
    assert.deepEqual(vc.all(), []);
    assert.deepEqual(vc.buildBranchSeqs('main'), []);
  });

  it('delegates query allocations to the transactions decorator', () => {
    const tx = wrap(openTx(), { reserveQueryAllocations: (_original, ...args) => args });
    assert.deepEqual(open(tx, namedMemory()).reserveQueryAllocations(1, 2, 3), [1, 2, 3]);
  });
});

describe('rollback failures (version_automatic_failure_tests.rs)', () => {
  /** A repository whose transactions rollback fails `failures` times, before or after undoing. */
  function failing(undoFirst, failures = 1) {
    let remaining = failures;
    const tx = wrap(openTx(), {
      rollback(original) {
        if (remaining === 0) return original();
        remaining--;
        if (undoFirst) original();
        throw new Error('injected rollback');
      },
    });
    return open(tx, namedMemory());
  }

  it('keeps the transaction open when its undo fails, for a retry', () => {
    const vc = failing(false);
    vc.beginTransaction();
    vc.create(1, 2);
    assert.throws(() => vc.rollback(), { message: 'injected rollback' });
    assert.throws(() => vc.beginTransaction(), {
      message: 'Nested version-control transactions are not supported.',
    });
    vc.rollback();
    assert.deepEqual(vc.all(), []);
    vc.beginTransaction();
    vc.rollback();
  });

  it('releases a transaction that was aborted before the error', () => {
    const vc = failing(true);
    vc.beginTransaction();
    vc.create(1, 2);
    assert.throws(() => vc.rollback(), { message: 'injected rollback' });
    assert.deepEqual(vc.all(), []);
    assert.equal(vc.create(3, 4), 1);
  });

  it('reports the undo failure of an automatic write, then retries', () => {
    const vc = failing(false);
    assert.throws(() => vc.update(999, 1, 2), { message: 'injected rollback' });
    vc.rollback();
    assert.throws(() => vc.update(999, 1, 2), {
      message: 'Storage error: Link not found: 999',
    });
    assert.equal(vc.create(1, 2), 1);
  });
});

describe('movement failures (version_movement_failure_tests.rs)', () => {
  /** A repository with two transitions and a fault planned on `target`. */
  function failing(plan) {
    const branches = faulty(namedMemory());
    const tx = faulty(openTx());
    const vc = open(tx, branches);
    const id = vc.create(1, 2);
    vc.update(id, 3, 4);
    plan({ tx, branches });
    return { vc, id, tx, branches };
  }

  for (const [label, plan] of [
    ['the replay cannot begin', ({ tx }) => tx.fail('beginVersionReplay')],
    ['the intent cannot be saved', ({ branches }) => branches.fail('save')],
    ['a row cannot be replayed', ({ tx }) => tx.fail('replayVersionTransition')],
    ['the replay cannot be flushed', ({ tx }) => tx.fail('flushVersionReplay')],
    ['the selection cannot be saved', ({ branches }) => branches.fail('save', { after: 1 })],
    ['the intent cannot be cleared', ({ branches }) => branches.fail('save', { after: 2 })],
    ['the replay cannot finish', ({ tx }) => tx.fail('finishVersionReplay')],
  ]) {
    it(`finishes a checkout on save after ${label}`, () => {
      const { vc, id, branches } = failing(plan);
      assert.throws(() => vc.checkout(1n), { message: /^injected / });
      assert.throws(() => vc.create(5, 6), { message: INCOMPLETE });
      assert.throws(() => vc.checkout(0n), {
        message: 'checkout is not allowed while a version-control transaction is open.',
      });
      vc.save();
      assert.equal(vc.currentSequence(), 1n);
      assert.deepEqual(vc.get(id), { index: id, source: 1, target: 2 });
      assert.equal(
        markers(branches).some((name) => name.startsWith('__vc:move:')),
        false,
      );
      vc.checkout(2n);
      assert.deepEqual(vc.get(id), { index: id, source: 3, target: 4 });
    });
  }
});
