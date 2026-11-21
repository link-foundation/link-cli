// The version-control decorator of src/version-control.js and its marker
// codecs: the port of rust/src/version_control/tests.rs,
// rust/src/version_control/path_contract_tests.rs, the codec tests of
// movement.rs and publication.rs, and the scenarios of
// rust/tests/version_control_decorator_tests.rs,
// version_metadata_names_tests.rs and version_retained_history_tests.rs.
//
// Every vector of docs/protocol/version-metadata-vectors.json,
// docs/protocol/version-movement-vectors.json and
// docs/protocol/version-publication-vectors.json is replayed here; the
// recovery and failure contracts are in version-control-recovery.test.mjs.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { capture, traceLines } from './helpers/storage.mjs';
import {
  memoryLog,
  namedMemory,
  openTx,
  openVc,
  protocolVectors,
} from './helpers/transactions.mjs';
import { decodeUtf16Hex, encodeName, serializeRecord } from '../src/transactions-types.js';
import {
  branchInfo,
  decodeBranchMarker,
  decodeMovement,
  decodePublication,
  DEFAULT_BRANCH_NAME,
  encodeBranchMarker,
  encodeMovement,
  encodePublication,
  makeVersionControlDatabaseFilename,
  movementChecksum,
  VersionControlDecorator,
} from '../src/version-control.js';

const metadataVectors = protocolVectors('version-metadata-vectors.json');
const movementVectors = protocolVectors('version-movement-vectors.json');
const publicationVectors = protocolVectors('version-publication-vectors.json');

/** A version-control decorator whose stores can be reopened: `reopen()`. */
function repository(options = {}) {
  const stores = { store: namedMemory(), log: memoryLog(), branches: namedMemory(), ...options };
  return { ...stores, vc: openVc(stores), reopen: () => openVc(stores) };
}

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

describe('branch markers (tests.rs)', () => {
  it('decodes every shared branch vector and re-encodes the version-two ones', () => {
    assert.equal(metadataVectors.branches.length, 10);
    for (const row of metadataVectors.branches) {
      const expected = branchInfo(row.name, row.parent ?? null, BigInt(row.fork), BigInt(row.head));
      assert.deepEqual(decodeBranchMarker(row.marker), expected, row.marker);
      if (row.version === 2) assert.equal(encodeBranchMarker(expected), row.marker);
    }
  });

  it('rejects every malformed branch vector', () => {
    assert.equal(metadataVectors.invalidBranches.length, 19);
    for (const marker of metadataVectors.invalidBranches) {
      assert.equal(decodeBranchMarker(marker), null, marker);
    }
  });

  it('decodes lower-case names and rejects invalid UTF-16', () => {
    assert.equal(decodeUtf16Hex('006a006b'), 'jk');
    for (const invalid of ['D800', 'DFFF', 'D8000041', 'éé', '０'.repeat(4)]) {
      assert.equal(decodeUtf16Hex(invalid), undefined, invalid);
    }
  });

  it('round-trips an encoded marker', () => {
    const info = branchInfo('feature', 'main', 5n, 9n);
    const text = encodeBranchMarker(info);
    assert.equal(text, '__vc:branch:2|0066006500610074007500720065|006D00610069006E|5|9');
    assert.deepEqual(decodeBranchMarker(text), info);
    assert.equal(
      encodeBranchMarker(branchInfo('root', null, 0n, 0n)),
      '__vc:branch:2|0072006F006F0074|-|0|0',
    );
  });

  it('rejects markers with another prefix and legacy markers with bad numbers', () => {
    assert.equal(decodeBranchMarker('not a marker'), null);
    assert.equal(decodeBranchMarker('__vc:branch:x:parent=:fork=z:head=1'), null);
    assert.equal(decodeBranchMarker('__vc:branch:x:parent=:fork=1:head=z'), null);
    assert.equal(decodeBranchMarker('__vc:branch:x:parent=:fork=1'), null);
    assert.equal(decodeBranchMarker('__vc:branch:x:fork=1:head=1'), null);
    // A "2|" legacy name whose version-two fields do not decode falls back.
    assert.deepEqual(
      decodeBranchMarker('__vc:branch:2|0041|-|0|0:parent=:fork=1:head=2'),
      branchInfo('2|0041|-|0|0', null, 1n, 2n),
    );
    assert.equal(decodeBranchMarker('__vc:branch:2|0041|D800|0|0'), null);
    assert.equal(decodeBranchMarker('__vc:branch:2|D800|-|0|0'), null);
    assert.equal(decodeBranchMarker('__vc:branch:2|0041|-|x|0'), null);
    assert.equal(decodeBranchMarker('__vc:branch:2|0041|-|0|x'), null);
  });

  it('names the sidecar beside the database', () => {
    assert.equal(
      makeVersionControlDatabaseFilename('/var/data/db.links'),
      '/var/data/db.versioncontrol.links',
    );
    assert.equal(
      VersionControlDecorator.makeVersionControlDatabaseFilename('/var/data/db.links'),
      '/var/data/db.versioncontrol.links',
    );
    assert.equal(makeVersionControlDatabaseFilename('db.links'), 'db.versioncontrol.links');
  });
});

describe('tag markers (version_metadata_names_tests.rs)', () => {
  it('recovers entire legacy tag names and ignores every malformed tag vector', () => {
    assert.equal(metadataVectors.tags.length, 6);
    assert.equal(metadataVectors.invalidTags.length, 6);
    const branches = metadataWith([
      ...metadataVectors.tags.map((row) => row.marker),
      ...metadataVectors.invalidTags,
    ]);
    const vc = openVc({ branches });
    assert.equal(vc.listTags().size, metadataVectors.tags.length);
    for (const row of metadataVectors.tags) {
      assert.equal(vc.tryGetTag(row.name), BigInt(row.sequence), row.marker);
    }
    assert.equal(vc.tryGetTag('missing'), null);
  });
});

describe('movement markers (movement.rs)', () => {
  it('preserves the exact paths and payloads of every shared movement vector', () => {
    assert.equal(movementVectors.movements.length, 5);
    for (const row of movementVectors.movements) {
      const intent = decodeMovement(row.marker);
      assert.equal(intent.fromBranch, row.fromBranch);
      assert.equal(intent.from, BigInt(row.fromSequence));
      assert.equal(intent.toBranch, row.toBranch);
      assert.equal(intent.to, BigInt(row.toSequence));
      assert.deepEqual(intent.undo.map(serializeRecord), row.undo);
      assert.deepEqual(intent.redo.map(serializeRecord), row.redo);
      assert.equal(encodeMovement(intent), row.marker);
    }
  });

  it('rejects every invalid and non-Unicode movement vector', () => {
    assert.equal(movementVectors.invalidMovements.length, 22);
    assert.equal(movementVectors.nonUnicodeMovements.length, 2);
    for (const marker of [
      ...movementVectors.invalidMovements,
      ...movementVectors.nonUnicodeMovements,
    ]) {
      assert.equal(decodeMovement(marker), null, marker);
    }
  });

  it('checksums with FNV-1a 64 over UTF-8', () => {
    assert.equal(movementChecksum(''), 0xcbf29ce484222325n);
    assert.equal(movementChecksum('a'), 0xaf63dc4c8601ec8cn);
    assert.equal(movementChecksum('é'), 0x0ac21707b7181e01n);
  });

  /** A movement marker over raw record fields, checksummed. */
  const rawMovement = (
    undo,
    redo = '',
    fields = ['006D00610069006E', '1', '006D00610069006E', '0'],
  ) => {
    const body = ['1', ...fields, undo, redo].join('|');
    return `__vc:move:${body}|${movementChecksum(body).toString(16).padStart(16, '0')}`;
  };
  const id = '00000000000000000000000000000001';

  it('accepts only canonical records of a decided shape', () => {
    const accepted = [
      `v1|${id}|1|0|0|0,0,0|1,0,0`,
      `v1|${id}|1|0|1|1,0,0|1,2,3`,
      `v1|${id}|1|0|2|1,2,3|0,0,0`,
      `v2|${id}|1|0|1|0,0,0|0,0,0|n|1,-,0041`,
    ];
    for (const record of accepted) {
      const marker = rawMovement(encodeName(record));
      assert.deepEqual(decodeMovement(marker).undo.map(serializeRecord), [record], record);
    }
    const rejected = [
      'garbage',
      `v1|${id}|01|0|0|0,0,0|1,0,0`,
      `v1|${'0'.repeat(32)}|1|0|0|0,0,0|1,0,0`,
      `v1|${id}|0|0|0|0,0,0|1,0,0`,
      `v1|${id}|1|-62135596800001|0|0,0,0|1,0,0`,
      `v1|${id}|1|253402300800000|0|0,0,0|1,0,0`,
      `v1|${id}|1|0|0|1,0,0|1,0,0`,
      `v1|${id}|1|0|0|0,0,0|0,0,0`,
      `v1|${id}|1|0|1|0,0,0|1,2,3`,
      `v1|${id}|1|0|1|1,0,0|2,0,0`,
      `v1|${id}|1|0|2|0,0,0|0,0,0`,
      `v1|${id}|1|0|2|1,0,0|1,0,0`,
    ];
    for (const record of rejected) {
      assert.equal(decodeMovement(rawMovement(encodeName(record))), null, record);
    }
    assert.equal(decodeMovement(rawMovement('D800')), null);
    assert.equal(decodeMovement(rawMovement('', 'D800')), null);
  });

  it('rejects bad fields, negative points, blank branches and non-canonical text', () => {
    const empty = rawMovement('');
    assert.deepEqual(decodeMovement(empty), {
      fromBranch: 'main',
      from: 1n,
      toBranch: 'main',
      to: 0n,
      undo: [],
      redo: [],
    });
    const branch = '006D00610069006E';
    for (const fields of [
      ['D800', '1', branch, '0'],
      [branch, '1', 'DFFF', '0'],
      [branch, 'x', branch, '0'],
      [branch, '1', branch, 'x'],
      [branch, '-1', branch, '0'],
      [branch, '1', branch, '-1'],
      ['0020', '1', branch, '0'],
      [branch, '1', '0009', '0'],
      [branch, '01', branch, '0'],
      ['006d00610069006e', '1', branch, '0'],
    ]) {
      assert.equal(decodeMovement(rawMovement('', '', fields)), null, fields.join('|'));
    }
    assert.equal(decodeMovement('__vc:move:no-checksum'), null);
    assert.equal(decodeMovement(`${empty.slice(0, -16)}${'z'.repeat(16)}`), null);
    const shortHash = empty.slice(0, -1);
    assert.equal(decodeMovement(shortHash), null);
    const twoBody = `2|${branch}|1|${branch}|0||`;
    const two = `__vc:move:${twoBody}|${movementChecksum(twoBody).toString(16).padStart(16, '0')}`;
    assert.equal(decodeMovement(two), null);
    const sixBody = `1|${branch}|1|${branch}|0|`;
    const six = `__vc:move:${sixBody}|${movementChecksum(sixBody).toString(16).padStart(16, '0')}`;
    assert.equal(decodeMovement(six), null);
    assert.equal(decodeMovement('wrong-prefix'), null);
  });
});

describe('publication markers (publication.rs)', () => {
  it('round-trips the identity, range and name of every shared intent', () => {
    assert.equal(publicationVectors.intents.length, 3);
    for (const row of publicationVectors.intents) {
      const expected = {
        id: BigInt(`0x${row.id}`),
        before: BigInt(row.before),
        after: BigInt(row.after),
        branch: row.branch,
      };
      assert.deepEqual(decodePublication(row.marker), expected, row.marker);
      assert.equal(encodePublication(expected), row.marker);
    }
  });

  it('rejects every invalid intent vector', () => {
    assert.equal(publicationVectors.invalidIntents.length, 36);
    for (const marker of publicationVectors.invalidIntents) {
      assert.equal(decodePublication(marker), null, marker);
    }
  });
});

describe('branch paths (path_contract_tests.rs)', () => {
  /** The shallow recursive oracle of the Rust test. */
  function reference(vc, name, visited) {
    const info = vc.branches.get(name);
    if (info === undefined || visited.has(name)) return [];
    visited.add(name);
    const inherited = (info.parent === null ? [] : reference(vc, info.parent, visited)).filter(
      (sequence) => sequence <= info.forkSeq,
    );
    const own = [...vc.transitionBranches]
      .filter(([sequence, owner]) => owner === name && sequence <= info.head)
      .map(([sequence]) => sequence)
      .sort((left, right) => (left < right ? -1 : left > right ? 1 : 0));
    return [...inherited, ...own];
  }

  it('matches the shallow reference for cutoffs, order and broken chains', () => {
    const vc = openVc();
    const names = ['root', 'middle', 'leaf', 'unrelated', 'missing'];
    const mask = (1n << 64n) - 1n;
    let state = 0x138n;
    const next = () => {
      state = (state * 6364136223846793005n + 1n) & mask;
      return state >> 32n;
    };
    let visible = 0;
    for (let index = 0; index < 512; index++) {
      vc.branches.clear();
      for (let [name, parent] of [
        ['root', null],
        ['middle', 'root'],
        ['leaf', 'middle'],
        ['unrelated', null],
      ]) {
        if (index % 4 === 1 && name === 'root') parent = 'leaf';
        if (index % 4 === 2 && name === 'middle') parent = 'missing';
        const forkSeq = next() % 14n;
        const head = next() % 14n;
        vc.branches.set(name, branchInfo(name, parent, forkSeq, head));
      }
      vc.transitionBranches.clear();
      for (let sequence = 1n; sequence <= 12n; sequence++) {
        vc.transitionBranches.set(sequence, names[Number(next() % 5n)]);
      }
      for (const target of names) {
        const actual = vc.buildBranchSeqs(target);
        assert.deepEqual(actual, reference(vc, target, new Set()), `case ${index}, ${target}`);
        visible += actual.length;
      }
    }
    assert.equal(visible, VISIBLE_SEQUENCES);
  });
});

describe('VersionControlDecorator (version_control_decorator_tests.rs)', () => {
  it('creates the default branch on first open', () => {
    const { vc, branches } = repository();
    assert.equal(DEFAULT_BRANCH_NAME, 'main');
    assert.equal(vc.currentBranch, 'main');
    assert.equal(vc.currentSequence(), 0n);
    assert.deepEqual(vc.listBranches(), [branchInfo('main', null, 0n, 0n)]);
    assert.deepEqual(markers(branches), [
      '__vc:branch:2|006D00610069006E|-|0|0',
      '__vc:current=main',
      '__vc:applied=0',
    ]);
  });

  it('attributes new transitions to the current branch', () => {
    const { vc, branches } = repository();
    const id = vc.create(0, 0);
    vc.update(id, 11, 12);
    assert.equal(vc.currentSequence(), 2n);
    assert.deepEqual(vc.listBranches(), [branchInfo('main', null, 0n, 2n)]);
    assert.deepEqual(markers(branches), [
      '__vc:branch:2|006D00610069006E|-|0|2',
      '__vc:current=main',
      '__vc:applied=2',
      '__vc:trans:1:branch=main',
      '__vc:trans:2:branch=main',
    ]);
  });

  it('rewinds to zero and replays forward', () => {
    const { vc } = repository();
    const first = vc.createAndUpdate(11, 12);
    const second = vc.createAndUpdate(21, 22);
    vc.checkout(0n);
    assert.equal(vc.currentSequence(), 0n);
    assert.equal(vc.exists(first), false);
    assert.deepEqual(vc.all(), []);
    vc.checkout(4n);
    assert.deepEqual(vc.get(second), { index: second, source: 21, target: 22 });
    vc.checkout(2n);
    assert.equal(vc.exists(second), false);
    assert.deepEqual(vc.search(11, 12), first);
    // A checkout to the applied point moves nothing.
    vc.checkout(2n);
    assert.equal(vc.currentSequence(), 2n);
  });

  it('forks, switches, tags and branches from explicit points', () => {
    const { vc } = repository();
    const base = vc.createAndUpdate(11, 12);
    vc.branch('feature');
    assert.deepEqual(vc.listBranches()[0], branchInfo('feature', 'main', 2n, 2n));
    vc.switchBranch('feature');
    const feature = vc.createAndUpdate(21, 22);
    assert.equal(vc.currentSequence(), 4n);
    vc.switchBranch('main');
    assert.equal(vc.currentSequence(), 2n);
    assert.equal(vc.exists(feature), false);
    const mainOnly = vc.createAndUpdate(31, 32);
    assert.equal(vc.currentSequence(), 6n);
    vc.switchBranch('feature');
    // Both branches reuse the second address.
    assert.equal(mainOnly, feature);
    assert.deepEqual(vc.all(), [
      { index: base, source: 11, target: 12 },
      { index: feature, source: 21, target: 22 },
    ]);
    vc.tag('release');
    vc.tag('start', 0n);
    assert.deepEqual(
      [...vc.listTags()],
      [
        ['release', 4n],
        ['start', 0n],
      ],
    );
    vc.tag('release', 2n);
    assert.equal(vc.tryGetTag('release'), 2n);
    vc.branch('early', 2n);
    vc.branch('empty', 0n);
    assert.deepEqual(
      vc.listBranches().map(({ name, parent, forkSeq }) => [name, parent, forkSeq]),
      [
        ['early', 'feature', 2n],
        ['empty', 'feature', 0n],
        ['feature', 'main', 2n],
        ['main', null, 0n],
      ],
    );
    // The listing is a copy.
    vc.listBranches()[0].head = 99n;
    assert.equal(vc.branches.get('early').head, 2n);
  });

  it('rebuilds its state from the branches store', () => {
    const repo = repository();
    repo.vc.createAndUpdate(11, 12);
    repo.vc.branch('topic');
    repo.vc.switchBranch('topic');
    const created = repo.vc.createAndUpdate(21, 22);
    repo.vc.tag('v1');
    repo.vc.save();
    const reopened = repo.reopen();
    assert.equal(reopened.currentBranch, 'topic');
    assert.equal(reopened.currentSequence(), 4n);
    assert.equal(reopened.tryGetTag('v1'), 4n);
    assert.deepEqual(reopened.listBranches(), repo.vc.listBranches());
    reopened.recover();
    assert.equal(reopened.currentBranch, 'topic');
    assert.equal(reopened.exists(created), true);
  });

  it('rejects unreachable points, duplicate and empty names and unknown branches', () => {
    const { vc } = repository();
    vc.createAndUpdate(11, 12);
    vc.branch('side');
    vc.switchBranch('side');
    vc.createAndUpdate(21, 22);
    vc.switchBranch('main');
    const rejects = (action, message) => assert.throws(action, { message });
    rejects(() => vc.checkout(99n), "Sequence 99 is not reachable on branch 'main'.");
    rejects(() => vc.checkout(3n), "Sequence 3 is not reachable on branch 'main'.");
    rejects(() => vc.checkout(-1n), 'Sequence must be non-negative.');
    rejects(() => vc.branch('side'), "Branch 'side' already exists.");
    rejects(() => vc.branch(' \t'), 'Branch name must not be empty.');
    rejects(() => vc.branch('x', -1n), 'Fork point cannot be negative.');
    rejects(() => vc.branch('x', 4n), "Fork point 4 is not reachable on branch 'main'.");
    rejects(() => vc.switchBranch('nope'), "Unknown branch 'nope'.");
    rejects(() => vc.tag(' '), 'Tag name must not be empty.');
    rejects(() => vc.tag('x', -1n), 'Tag sequence must be non-negative.');
    rejects(() => vc.commit(), 'No version-control transaction is open.');
    assert.equal(vc.currentSequence(), 2n);
  });

  it('commits and rolls back explicit transactions atomically', () => {
    const repo = repository();
    const { vc } = repo;
    vc.beginTransaction();
    assert.throws(() => vc.beginTransaction(), {
      message: 'Nested version-control transactions are not supported.',
    });
    for (const [operation, action] of [
      ['branch', () => vc.branch('x')],
      ['switch_branch', () => vc.switchBranch('main')],
      ['checkout', () => vc.checkout(0n)],
      ['tag', () => vc.tag('x')],
      ['recover', () => vc.recover()],
    ]) {
      assert.throws(action, {
        message: `${operation} is not allowed while a version-control transaction is open.`,
      });
    }
    const rolledBack = vc.createAndUpdate(11, 12);
    vc.rollback();
    assert.equal(vc.exists(rolledBack), false);
    assert.equal(vc.currentSequence(), 0n);
    assert.deepEqual(
      markers(repo.branches).filter((name) => name.startsWith('__vc:trans:')),
      [],
    );
    const { id } = vc.beginTransaction();
    assert.equal(vc.requireTransaction().id, id);
    const first = vc.createAndUpdate(11, 12);
    const second = vc.createAndUpdate(21, 22);
    vc.commit();
    assert.equal(vc.currentSequence(), 6n);
    assert.deepEqual(
      markers(repo.branches).filter((name) => name.startsWith('__vc:trans:')),
      [
        '__vc:trans:3:branch=main',
        '__vc:trans:4:branch=main',
        '__vc:trans:5:branch=main',
        '__vc:trans:6:branch=main',
      ],
    );
    vc.save();
    const reopened = repo.reopen();
    assert.equal(reopened.exists(first) && reopened.exists(second), true);
    assert.equal(reopened.currentSequence(), 6n);
    // The rolled back records are never versions.
    assert.throws(() => reopened.checkout(2n), {
      message: "Sequence 2 is not reachable on branch 'main'.",
    });
    // An empty transaction commits no attribution.
    reopened.beginTransaction();
    reopened.commit();
    assert.equal(reopened.currentSequence(), 6n);
    // Rolling back with no transaction open reports it.
    assert.throws(() => reopened.rollback(), {
      message: 'No version-control transaction is open.',
    });
  });

  it('publishes nothing when the store already rolled the transaction back', () => {
    const { vc } = repository();
    vc.beginTransaction();
    const id = vc.createAndUpdate(11, 12);
    vc.transactions.rollback();
    vc.commit();
    assert.equal(vc.activeTransaction, null);
    assert.equal(vc.exists(id), false);
    assert.equal(vc.currentSequence(), 0n);
    assert.deepEqual(vc.listBranches(), [branchInfo('main', null, 0n, 0n)]);
  });

  it('routes every write and read through the transactions decorator', () => {
    const { vc } = repository();
    const id = vc.create(1, 2);
    assert.deepEqual(
      vc.updateObserved(id, 3, 4, () => {}),
      { index: id, source: 1, target: 2 },
    );
    vc.setName(id, 'named');
    assert.equal(vc.getName(id), 'named');
    assert.equal(vc.getByName('named'), id);
    vc.removeName(id);
    assert.equal(vc.getName(id), null);
    assert.equal(vc.getOrCreate(3, 4), id);
    const other = vc.tryGetOrCreate(5, 6);
    assert.equal(vc.tryCreate(0, 0), other + 1);
    assert.equal(vc.ensureCreated(0), 0);
    assert.equal(vc.ensureCreated(id), id);
    assert.equal(vc.ensureCreated(10), 10);
    assert.deepEqual(vc.getLink(id), { index: id, source: 3, target: 4 });
    assert.equal(vc.allLinks().length, 4);
    const changes = [];
    vc.deleteObserved(10, (before, after) => changes.push([before.index, after.index]));
    assert.deepEqual(changes, [[10, 0]]);
    assert.equal(vc.exists(10), false);
    const sequence = vc.currentSequence();
    assert.throws(() => vc.update(999, 1, 2), { message: 'Storage error: Link not found: 999' });
    assert.equal(vc.currentSequence(), sequence);
    assert.equal(vc.activeTransaction, null);
    assert.equal(vc.transactions.hasOpenTransaction(), false);
  });

  it('traces branching, switching, checkout and tagging', () => {
    const stderr = capture();
    const { vc } = repository({ trace: true, stderr });
    vc.createAndUpdate(11, 12);
    vc.branch('topic');
    vc.switchBranch('topic');
    vc.checkout(0n);
    vc.tag('zero');
    const lines = traceLines(stderr).filter((line) => line.startsWith('[VersionControl]'));
    assert.match(
      lines[0],
      /^\[VersionControl\] Prepared version publication [0-9a-f]{32}: 0\.\.2 on 'main'\.$/,
    );
    assert.deepEqual(lines.slice(1), [
      "[VersionControl] Created branch 'topic' from 'main' at seq 2.",
      '[VersionControl] Prepared durable version movement.',
      '[VersionControl] Completed durable version movement.',
      "[VersionControl] Switched to branch 'topic' at seq 2.",
      '[VersionControl] Prepared durable version movement.',
      '[VersionControl] Completed durable version movement.',
      "[VersionControl] Checked out seq 0 on branch 'topic'.",
      "[VersionControl] Created tag 'zero' at seq 0.",
    ]);
  });
});

/** `verify_names` of version_metadata_names_tests.rs. */
function verifyNames(branch, tag, legacyMarkers) {
  const repo = repository();
  const { vc } = repo;
  const baseline = vc.createAndUpdate(11, 12);
  const baselineHead = vc.currentSequence();
  vc.branch(branch);
  vc.switchBranch(branch);
  const created = vc.createAndUpdate(21, 22);
  const head = vc.currentSequence();
  vc.tag(tag);
  vc.branch('child', head);
  vc.tag('plain');
  const listed = vc.listBranches();
  vc.save();
  if (legacyMarkers) {
    for (const info of listed) {
      const link = repo.branches.getByName(encodeBranchMarker(info));
      assert.notEqual(link, null);
      repo.branches.setName(
        link,
        `__vc:branch:${info.name}:parent=${info.parent ?? ''}:fork=${info.forkSeq}:head=${info.head}`,
      );
    }
  }
  const recovered = repo.reopen();
  assert.equal(recovered.currentBranch, branch);
  assert.equal(recovered.currentSequence(), head);
  const byName = new Map(recovered.listBranches().map((info) => [info.name, info]));
  assert.equal(byName.get(branch).parent, 'main');
  assert.equal(byName.get(branch).head, head);
  assert.equal(byName.get('child').parent, branch);
  assert.equal(byName.get('child').forkSeq, head);
  assert.equal(recovered.tryGetTag(tag), head);
  assert.equal(recovered.tryGetTag('plain'), head);
  recovered.switchBranch('main');
  assert.equal(recovered.currentSequence(), baselineHead);
  assert.equal(recovered.exists(baseline), true);
  assert.equal(recovered.exists(created), false);
  recovered.switchBranch('child');
  assert.equal(recovered.currentSequence(), head);
  assert.equal(recovered.exists(created), true);
  recovered.checkout(baselineHead);
  assert.equal(recovered.exists(created), false);
  recovered.checkout(head);
  assert.equal(recovered.exists(created), true);
}

describe('metadata names survive reopening (version_metadata_names_tests.rs)', () => {
  it('keeps delimiter branch names', () =>
    verifyNames('topic:parent=decoy:fork=7:head=9/\u{1f600}', 'plain', false));
  it('keeps quoted parent names', () =>
    verifyNames('{"name":"quoted"}:parent=:', '"quoted"=value', false));
  it('keeps delimiter tag names', () =>
    verifyNames('safe-branch', 'release=version=\u{1f600}', false));
  it('keeps NUL and newline names', () =>
    verifyNames('zero\0line\n:parent=\u{1f600}', 'zero\0=\n', false));
  it('keeps legacy ancestry and checkout', () => verifyNames('legacy', 'old=tag', true));
  it('keeps legacy names starting with the version-two prefix', () =>
    verifyNames('2|0041|-|0|0', 'prefix=tag', true));

  /** `reopened_after`: a one-commit repository reopened after `edit`. */
  function reopenedAfter(edit) {
    const repo = repository();
    repo.vc.createAndUpdate(11, 12);
    const head = repo.vc.currentSequence();
    repo.vc.save();
    edit(repo.branches);
    const reopened = repo.reopen();
    assert.equal(reopened.currentBranch, 'main');
    assert.equal(
      reopened.listBranches().some((info) => info.name === 'main'),
      true,
    );
    assert.equal(reopened.currentSequence(), head);
    return reopened;
  }
  const removeMarker = (prefix) => (branches) => {
    const link = branches
      .allLinks()
      .find(({ index }) => branches.getName(index)?.startsWith(prefix)).index;
    branches.removeName(link);
  };

  it('writes a lost current-branch marker again', () => {
    const reopened = reopenedAfter(removeMarker('__vc:current='));
    assert.equal(markers(reopened.branchesStore).includes('__vc:current=main'), true);
  });

  it('keeps the transitions a lost default-branch marker owned', () => {
    const reopened = reopenedAfter(removeMarker('__vc:branch:'));
    assert.deepEqual(reopened.buildBranchSeqs('main'), [1n, 2n]);
  });

  it('ignores unreadable transition markers while nothing moves', () => {
    reopenedAfter((branches) => {
      for (const [index, name] of [
        [40, '__vc:trans:not-a-number:branch=main'],
        [41, '__vc:trans:no-branch-here'],
      ]) {
        branches.ensureCreated(index);
        branches.setName(index, name);
      }
    });
  });

  it('ignores names of other prefixes and unnamed links', () => {
    const reopened = reopenedAfter((branches) => {
      branches.tryCreate(0, 0);
      branches.setName(branches.tryCreate(0, 0), 'unrelated');
    });
    assert.deepEqual(reopened.buildBranchSeqs('main'), [1n, 2n]);
  });
});

describe('retained history (version_retained_history_tests.rs)', () => {
  /** A repository whose journal lost `sequence` before a movement. */
  function evicted(sequence) {
    const store = namedMemory();
    const log = memoryLog();
    const branches = namedMemory();
    const vc = new VersionControlDecorator(openTx({ store, log }), branches);
    vc.createAndUpdate(11, 12);
    vc.branch('side');
    vc.switchBranch('side');
    vc.createAndUpdate(21, 22);
    vc.switchBranch('main');
    vc.createAndUpdate(31, 32);
    vc.save();
    log.entries = log.entries.filter(
      (entry) =>
        !entry.startsWith(`__transactions:transition:`) || entry.split('|')[2] !== `${sequence}`,
    );
    return new VersionControlDecorator(openTx({ store, log }), branches);
  }
  const message = (sequence) =>
    `Transition ${sequence} is no longer retained; checkout requires complete changed history.`;

  it('rejects a checkout with missing undo before mutation', () => {
    const vc = evicted(6);
    const rows = vc.all();
    assert.throws(() => vc.checkout(2n), { message: message(6) });
    assert.deepEqual(vc.all(), rows);
    assert.equal(vc.currentSequence(), 6n);
    assert.equal(vc.activeMovement, null);
  });

  it('rejects a branch switch with missing redo before undoing the current branch', () => {
    const vc = evicted(4);
    const rows = vc.all();
    assert.throws(() => vc.switchBranch('side'), { message: message(4) });
    assert.deepEqual(vc.all(), rows);
    assert.equal(vc.currentBranch, 'main');
  });
});

const VISIBLE_SEQUENCES = 3895;
