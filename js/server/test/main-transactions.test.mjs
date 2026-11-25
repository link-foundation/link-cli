// The transactions and version-control entry points of rust/src/main.rs
// (`parse_commit_mode`, `parse_retention`, `resolve_sequence`,
// `run_with_transactions`, `run_with_vc`) over in-memory stores: the
// `openNamedStorage` of the context hands out one named in-memory store per
// path, so state survives from one run to the next as the files do.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  parseCommitMode,
  parseRetention,
  resolveSequence,
  runWithTransactions,
  runWithVc,
} from '../src/main-transactions.js';
import {
  CommitMode,
  INFINITE_RETENTION,
  makeTransitionsLogFilename,
} from '../src/transactions-types.js';
import { memoryLog, openVc, namedMemory } from './helpers/transactions.mjs';

/** A writable stream that keeps what it is given. */
const sink = () => ({
  text: '',
  write(chunk) {
    this.text += chunk;
  },
});

/** The context main.js passes: `finish` runs `query(storage)` and records the stores. */
function world() {
  const stores = new Map();
  const context = {
    out: sink(),
    err: sink(),
    opened: [],
    finished: [],
    query: (storage) => storage.create(1, 1),
    openNamedStorage(path, _cli) {
      context.opened.push(path);
      if (!stores.has(path)) stores.set(path, namedMemory());
      return stores.get(path);
    },
    openJournal(database, explicit) {
      const path = explicit ?? makeTransitionsLogFilename(database);
      context.opened.push(path);
      if (!stores.has(path)) stores.set(path, memoryLog());
      return stores.get(path);
    },
    async finish(cli, storage) {
      await Promise.resolve();
      context.finished.push(cli);
      context.query(storage);
      storage.save();
    },
  };
  return context;
}

/** A parsed command line with every transactions and version-control field unset. */
const cli = (fields = {}) => ({
  db: 'db.links',
  transactionsFile: null,
  vcFile: null,
  trace: false,
  showLog: false,
  checkout: null,
  branch: null,
  branchFrom: null,
  tag: null,
  listBranches: false,
  listTags: false,
  ...fields,
});

const vcRun = (context, fields) =>
  runWithVc(cli(fields), CommitMode.Sync, INFINITE_RETENTION, context);
const txRun = (context, fields) =>
  runWithTransactions(cli(fields), CommitMode.Sync, INFINITE_RETENTION, context);

/** Masks the timestamp and transaction id columns of `--log` lines. */
const masked = (text) =>
  text.replace(/^(\d+)\t\d+\t(\w+)\t[0-9a-f]{32}\t/gm, '$1\t<t>\t$2\t<tx>\t');

describe('parseCommitMode', () => {
  it('reads sync and async in any ASCII case, sync by default', () => {
    assert.equal(parseCommitMode(undefined), CommitMode.Sync);
    assert.equal(parseCommitMode(null), CommitMode.Sync);
    assert.equal(parseCommitMode('  '), CommitMode.Sync);
    assert.equal(parseCommitMode(' SyNc '), CommitMode.Sync);
    assert.equal(parseCommitMode('ASYNC'), CommitMode.Async);
    assert.equal(parseCommitMode('async'), CommitMode.Async);
  });

  it('rejects anything else with the trimmed value', () => {
    assert.throws(() => parseCommitMode(' later '), {
      message: "Invalid --commit-mode value 'later'. Use 'sync' or 'async'.",
    });
    assert.throws(() => parseCommitMode('ſync'), {
      message: "Invalid --commit-mode value 'ſync'. Use 'sync' or 'async'.",
    });
  });
});

describe('parseRetention', () => {
  it('reads the retention policies, infinite by default', () => {
    assert.equal(parseRetention(undefined), INFINITE_RETENTION);
    assert.equal(parseRetention(' '), INFINITE_RETENTION);
    assert.equal(parseRetention('infinite'), INFINITE_RETENTION);
    assert.deepEqual(parseRetention(' sized:3 '), { kind: 'sized', maxTransitions: 3n });
    assert.deepEqual(parseRetention('chunked:2:arch'), {
      kind: 'chunked',
      chunkSize: 2n,
      archiveDirectory: 'arch',
    });
  });

  it('reports the parse error after the trimmed value, with no cause', () => {
    const rejects = (raw, message) =>
      assert.throws(
        () => parseRetention(raw),
        (error) => error.message === message && error.cause === undefined,
      );
    rejects(' forever ', "Invalid --retention value 'forever': unknown retention spec 'forever'");
    rejects(
      'sized:x',
      "Invalid --retention value 'sized:x': invalid sized retention spec 'sized:x'",
    );
    rejects(
      'chunked:0:a',
      "Invalid --retention value 'chunked:0:a': invalid chunked size in 'chunked:0:a'",
    );
  });
});

describe('resolveSequence', () => {
  it('reads a sequence number, else a tag, else null', () => {
    const vc = openVc();
    vc.create(1, 1);
    vc.tag('one', 1n);
    assert.equal(resolveSequence(vc, ' 7 '), 7n);
    assert.equal(resolveSequence(vc, '-1'), -1n);
    assert.equal(resolveSequence(vc, ' one '), 1n);
    assert.equal(resolveSequence(vc, 'none'), null);
    assert.equal(resolveSequence(vc, '  '), null);
  });
});

describe('runWithTransactions', () => {
  it('opens the data store and the default log, and runs the pipeline', async () => {
    const context = world();
    await txRun(context);
    assert.deepEqual(context.opened, ['db.links', 'db.transitions.log']);
    assert.equal(context.finished.length, 1);
    await txRun(context, { showLog: true });
    assert.equal(context.finished.length, 1);
    assert.equal(masked(context.out.text), '1\t<t>\tCreate\t<tx>\t(0,0,0) -> (1,1,1)\n');
  });

  it('honors --transactions-file', async () => {
    const context = world();
    await txRun(context, { transactionsFile: 'custom.log' });
    assert.deepEqual(context.opened, ['db.links', 'custom.log']);
  });

  it('awaits the pipeline and rejects with its error', async () => {
    const context = world();
    context.query = () => {
      throw new Error('pipeline failed');
    };
    await assert.rejects(txRun(context), { message: 'pipeline failed' });
  });

  it('closes the journal after every run, as Rust drops it (issue #195)', async () => {
    const context = world();
    let closed = 0;
    const openJournal = context.openJournal;
    context.openJournal = (database, explicit) =>
      Object.assign(openJournal(database, explicit), { close: () => closed++ });
    await txRun(context);
    await txRun(context, { showLog: true });
    await vcRun(context, { listTags: true });
    context.query = () => {
      throw new Error('pipeline failed');
    };
    await assert.rejects(txRun(context), { message: 'pipeline failed' });
    await assert.rejects(vcRun(context), { message: 'pipeline failed' });
    assert.equal(closed, 5);
  });
});

describe('runWithVc', () => {
  it('opens the version-control store next to the database or at --vc-file', async () => {
    const context = world();
    await vcRun(context);
    assert.deepEqual(context.opened, ['db.links', 'db.transitions.log', 'db.versioncontrol.links']);
    await vcRun(context, { vcFile: 'branches.links', listTags: true });
    assert.equal(context.opened.at(-1), 'branches.links');
    assert.equal(context.finished.length, 1);
  });

  it('lists branches, tags and the log, each without running the pipeline', async () => {
    const context = world();
    await vcRun(context);
    await vcRun(context);
    await vcRun(context, { tag: 'v1' });
    await vcRun(context, { tag: 'first=1' });
    context.out.text = '';
    await vcRun(context, { listTags: true });
    assert.equal(context.out.text, 'first\t1\nv1\t2\n');
    context.out.text = '';
    await vcRun(context, { listBranches: true });
    assert.equal(context.out.text, '* main\tparent=-\tfork=0\thead=4\n');
    context.out.text = '';
    await vcRun(context, { showLog: true });
    assert.equal(
      masked(context.out.text),
      [1, 2, 3, 4].map((n) => `${n}\t<t>\tCreate\t<tx>\t(0,0,0) -> (${n},1,1)\n`).join(''),
    );
    assert.equal(context.finished.length, 4);
  });

  it('checks out by sequence or tag and rejects unknown points', async () => {
    const context = world();
    await vcRun(context);
    await vcRun(context, { tag: 'one' });
    await vcRun(context);
    context.query = () => {};
    await vcRun(context, { checkout: 'one', trace: true });
    assert.equal(context.out.text, "Checked out seq 1 on branch 'main'.\n");
    await vcRun(context, { checkout: '0' });
    assert.equal(context.out.text, "Checked out seq 1 on branch 'main'.\n");
    await assert.rejects(vcRun(context, { checkout: 'nowhere' }), {
      message: "Unknown checkout point 'nowhere'.",
    });
    await assert.rejects(vcRun(context, { checkout: ' ' }), {
      message: "Unknown checkout point ' '.",
    });
  });

  it('creates a missing branch, then switches to it', async () => {
    const context = world();
    await vcRun(context);
    await vcRun(context, { branch: 'side', branchFrom: 0n, trace: true });
    assert.equal(context.out.text, "Created branch 'side'.\nSwitched to branch 'side'.\n");
    context.out.text = '';
    await vcRun(context, { branch: 'main', trace: true });
    await vcRun(context, { branch: 'side' });
    assert.equal(context.out.text, "Switched to branch 'main'.\n");
    context.out.text = '';
    await vcRun(context, { listBranches: true });
    assert.equal(
      context.out.text,
      '  main\tparent=-\tfork=0\thead=3\n* side\tparent=main\tfork=0\thead=4\n',
    );
  });

  it('tags at the head, at a point, at an alias with = in it, and rejects unknown points', async () => {
    const context = world();
    await vcRun(context);
    await vcRun(context, { tag: 'head', trace: true });
    await vcRun(context, { tag: 'release=0=1', trace: true });
    await vcRun(context, { tag: 'alias=release=0', trace: true });
    assert.equal(
      context.out.text,
      "Tagged 'head' at seq 1.\nTagged 'release=0' at seq 1.\nTagged 'alias' at seq 1.\n",
    );
    await assert.rejects(vcRun(context, { tag: 'lost=nowhere=x' }), {
      message: "Unknown tag point 'nowhere=x'.",
    });
    await assert.rejects(vcRun(context, { tag: 'blank=' }), {
      message: "Unknown tag point ''.",
    });
  });
});
