// The permission, token and trigger stages of src/main-permissions.js, one
// export at a time with stub callbacks, then end to end through `main` as the
// cases of rust/tests/cli_permissions_tests.rs and rust/tests/cli_triggers_tests.rs.
// Every expected text is what the Rust clink prints for the same invocation.
import { after, before, describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { LinkError, NotFoundError, QueryError } from '../src/errors.js';
import { LinkStorage } from '../src/link-storage.js';
import {
  finish,
  openSideStore,
  persistentTransformationsEnabled,
  persistentTransformationsRequested,
  resolveSubject,
  runPermittedQuery,
  runTokens,
  runTriggerCommand,
  triggerCommandCount,
  triggersPath,
  writeTokens,
} from '../src/main-permissions.js';
import { PermissionsDecorator } from '../src/permissions.js';
import { PersistentTransformationDecorator } from '../src/persistent-transformations.js';
import { indexes, World } from './permissions-world.mjs';

const TRIGGER = '(((1: 1 1)) ((1: 1 2)))';

// The end-to-end cases need the whole entry point; while a module it imports
// is still missing they are skipped, and any other failure to load is thrown.
const loaded = await import('../src/main.js').catch((error) => {
  if (error.code !== 'ERR_MODULE_NOT_FOUND') throw error;
  return error;
});
const main = loaded instanceof Error ? null : loaded.main;
const endToEnd = main == null ? `src/main.js does not load: ${loaded.message}` : false;

/** A fake `process.stdout`: what was written, in `text`. */
function sink() {
  const out = {
    text: '',
    write(chunk) {
      out.text += chunk;
      return true;
    },
  };
  return out;
}

let directory;
before(() => {
  directory = mkdtempSync(join(tmpdir(), 'clink-main-permissions-'));
});
after(() => rmSync(directory, { recursive: true, force: true }));

/** A fresh directory of its own under the test directory. */
const fresh = () => mkdtempSync(join(directory, 'case-'));

/** An in-memory store whose saves are told in `saves` as `label`. */
function saving(label, saves) {
  const store = LinkStorage.inMemory();
  store.save = () => saves.push(label);
  return store;
}

describe('the trigger flags', () => {
  it('counts the trigger commands', () => {
    assert.equal(triggerCommandCount({}), 0);
    assert.equal(triggerCommandCount({ always: true }), 1);
    assert.equal(triggerCommandCount({ once: true, never: true }), 2);
    assert.equal(triggerCommandCount({ always: true, once: true, never: true }), 3);
  });

  it('tells whether any flag of the family was given', () => {
    assert.equal(persistentTransformationsRequested({}), false);
    assert.equal(persistentTransformationsRequested({ triggersFile: null }), false);
    for (const flag of ['always', 'once', 'never', 'triggers', 'embedTriggers']) {
      assert.equal(persistentTransformationsRequested({ [flag]: true }), true, flag);
    }
    assert.equal(persistentTransformationsRequested({ triggersFile: 't.links' }), true);
    assert.equal(persistentTransformationsRequested({ triggersFile: '' }), true, 'Some("")');
  });

  it('puts the triggers next to the database unless told where', () => {
    assert.equal(triggersPath({ db: 'db.links' }), 'db.triggers.links');
    assert.equal(triggersPath({ db: 'dir/data.links' }), 'dir/data.triggers.links');
    assert.equal(
      triggersPath({ db: 'db.links', triggersFile: 'elsewhere.links' }),
      'elsewhere.links',
    );
  });

  it('runs the triggers when asked to or when a trigger store exists', () => {
    const dir = fresh();
    const cli = { db: join(dir, 'data.links') };
    assert.equal(persistentTransformationsEnabled(cli), false);
    assert.equal(persistentTransformationsEnabled({ ...cli, triggers: true }), true);
    writeFileSync(join(dir, 'data.triggers.links'), '');
    assert.equal(persistentTransformationsEnabled(cli), true, 'the sidecar exists');
    const elsewhere = { ...cli, triggersFile: join(dir, 'missing.links') };
    assert.equal(persistentTransformationsEnabled(elsewhere), true, '--triggers-file is a flag');
  });
});

describe('finish', () => {
  const unopened = () => assert.fail('no trigger store is opened');

  it('runs over the store itself when triggers are off', () => {
    const storage = LinkStorage.inMemory();
    const out = sink();
    const calls = [];
    const cli = { db: join(fresh(), 'data.links') };
    const result = finish(cli, storage, {
      run: (runCli, runStorage, stage) => {
        calls.push([runCli, runStorage]);
        assert.equal(stage({ always: true }, runStorage, TRIGGER), false, 'no trigger stage');
        return 'ran';
      },
      openTriggerStore: unopened,
      out,
    });
    assert.equal(result, 'ran');
    assert.deepEqual(calls, [[cli, storage]]);
    assert.equal(out.text, '');
  });

  it('wraps the store with embedded triggers', () => {
    const storage = LinkStorage.inMemory();
    const out = sink();
    const cli = { db: 'data.links', embedTriggers: true, autoCreateMissingReferences: true };
    const result = finish(cli, storage, {
      run: (runCli, decorated, stage) => {
        assert.equal(runCli, cli);
        assert.ok(decorated instanceof PersistentTransformationDecorator);
        assert.equal(decorated.inner(), storage);
        assert.equal(decorated.triggerStore().isEmbedded(), true);
        assert.equal(decorated.autoCreateMissingReferences(), true);
        assert.equal(stage({}, decorated, TRIGGER), false);
        assert.equal(stage({ always: true }, decorated, TRIGGER), true);
        return decorated;
      },
      openTriggerStore: unopened,
      out,
    });
    // The fresh trigger schema and trigger take 1..17, as in a fresh Rust store.
    assert.equal(out.text, 'Always persistent transformation trigger stored: 17\n');
    assert.equal(result.triggers().length, 1);
    assert.equal(storage.getByName('Always') != null, true, 'stored in the data');
  });

  it('wraps the store with a sidecar of triggers', () => {
    for (const [extra, path] of [
      [{}, 'dir/data.triggers.links'],
      [{ triggersFile: 'elsewhere.links' }, 'elsewhere.links'],
    ]) {
      const storage = LinkStorage.inMemory();
      const sidecar = LinkStorage.inMemory();
      const out = sink();
      const opened = [];
      const cli = {
        db: 'dir/data.links',
        trace: false,
        triggers: true,
        autoCreateMissingReferences: false,
        ...extra,
      };
      finish(cli, storage, {
        run: (_cli, decorated, stage) => {
          assert.equal(decorated.triggerStore().isEmbedded(), false);
          assert.equal(decorated.autoCreateMissingReferences(), false);
          assert.equal(stage({ once: true }, decorated, TRIGGER), true);
          assert.equal(stage({ never: true }, decorated, TRIGGER), true);
          assert.equal(stage({ always: true }, decorated, TRIGGER), true);
        },
        openTriggerStore: (openPath, trace) => {
          opened.push([openPath, trace]);
          return sidecar;
        },
        out,
      });
      assert.deepEqual(opened, [[path, false]]);
      assert.equal(
        out.text,
        'Once persistent transformation trigger stored: 17\n' +
          'Persistent transformation triggers removed: 1\n' +
          'Always persistent transformation trigger stored: 17\n',
      );
      assert.equal(storage.allLinks().length, 0, 'the data holds no trigger');
      assert.equal(sidecar.getByName('Always') != null, true);
    }
  });

  it('prints to process.stdout by default', () => {
    const written = [];
    const write = process.stdout.write;
    process.stdout.write = (chunk) => written.push(chunk);
    try {
      finish({ db: 'data.links', embedTriggers: true }, LinkStorage.inMemory(), {
        run: (_cli, decorated, stage) => stage({ never: true }, decorated, TRIGGER),
        openTriggerStore: unopened,
      });
      runTriggerCommand({ never: true }, { removeTriggers: () => 0 }, TRIGGER);
      writeTokens({ revokeToken: 'x' }, LinkStorage.inMemory(), 0);
      runTokens({ revokeToken: 'x' }, saving('data', []), null, 0);
    } finally {
      process.stdout.write = write;
    }
    assert.deepEqual(written, [
      'Persistent transformation triggers removed: 0\n',
      'Persistent transformation triggers removed: 0\n',
      'Unknown token.\n',
      'Unknown token.\n',
    ]);
  });
});

describe('openSideStore', () => {
  it('opens nothing for self and a store of its own otherwise', () => {
    const cli = { db: 'data.links' };
    assert.equal(
      openSideStore(cli, 'self', () => assert.fail('self opens nothing')),
      null,
    );
    const opened = [];
    const store = LinkStorage.inMemory();
    const open = (path, openCli) => {
      opened.push([path, openCli]);
      return store;
    };
    assert.equal(openSideStore(cli, 'rules.links', open), store);
    assert.equal(openSideStore(cli, 'Self', open), store, 'only self is the data');
    assert.deepEqual(opened, [
      ['rules.links', cli],
      ['Self', cli],
    ]);
  });
});

describe('resolveSubject', () => {
  const storage = LinkStorage.inMemory();
  const alice = storage.create(0, 0);
  storage.setName(alice, 'alice');
  const big = storage.create(0, 0);
  storage.setName(big, '4294967296');
  const nameless = { getByName: () => 0 };

  it('is a guest without a subject', () => {
    for (const subject of [undefined, null, '', '   ', '\t\n']) {
      assert.equal(resolveSubject({ subject }, nameless), 0, JSON.stringify(subject));
    }
  });

  it('reads a link index', () => {
    assert.equal(resolveSubject({ subject: '13' }, nameless), 13);
    assert.equal(resolveSubject({ subject: ' 007 ' }, nameless), 7);
    assert.equal(resolveSubject({ subject: '0' }, nameless), 0);
    assert.equal(resolveSubject({ subject: '4294967295' }, nameless), 0xffffffff);
    assert.equal(resolveSubject({ subject: '99' }, storage), 99, 'an index need not exist');
  });

  it('reads a name, and a number too large to be an index as one', () => {
    assert.equal(resolveSubject({ subject: 'alice' }, storage), alice);
    assert.equal(resolveSubject({ subject: ' alice ' }, storage), alice);
    assert.equal(resolveSubject({ subject: '4294967296' }, storage), big);
  });

  it('refuses an unknown subject', () => {
    for (const [subject, text] of [
      ['nobody', 'nobody'],
      [' x ', 'x'],
      ['99999999999', '99999999999'],
      ['-1', '-1'],
      ['+5', '+5'],
    ]) {
      assert.throws(() => resolveSubject({ subject }, storage), {
        message: `Unknown subject '${text}'.`,
      });
    }
    assert.throws(() => resolveSubject({ subject: 'zero' }, nameless), {
      message: "Unknown subject 'zero'.",
    });
  });
});

describe('runTokens and writeTokens', () => {
  it('grants and revokes in a table of its own, saving it and the data', () => {
    const saves = [];
    const [data, tokens] = [saving('data', saves), saving('tokens', saves)];
    const out = sink();
    runTokens({ grantToken: 'alice-secret' }, data, tokens, 13, out);
    runTokens({ grantToken: 'alice-secret', revokeToken: 'alice-secret' }, data, tokens, 13, out);
    runTokens({ revokeToken: 'alice-secret' }, data, tokens, 0, out);
    assert.equal(
      out.text,
      'Token granted to subject 13: 3\n' +
        'Token granted to subject 13: 3\n' +
        'Token revoked.\n' +
        'Unknown token.\n',
    );
    assert.deepEqual(saves, ['tokens', 'data', 'tokens', 'data', 'tokens', 'data']);
    assert.equal(data.allLinks().length, 0, 'the table is not in the data');
    assert.deepEqual(
      tokens.allLinks().map((value) => [value.index, value.source, value.target]),
      [
        [1, 1, 1],
        [2, 1, 13],
      ],
    );
  });

  it('grants and revokes in the data, saving it once', () => {
    const saves = [];
    const data = saving('data', saves);
    const subject = data.create(0, 0);
    const out = sink();
    runTokens({ grantToken: 'secret' }, data, null, subject, out);
    runTokens({ grantToken: 'secret' }, data, null, subject, out);
    runTokens({ revokeToken: 'secret' }, data, null, 0, out);
    runTokens({ revokeToken: 'secret' }, data, null, 0, out);
    assert.equal(
      out.text,
      'Token granted to subject 1: 4\n' +
        'Token granted to subject 1: 4\n' +
        'Token revoked.\n' +
        'Unknown token.\n',
    );
    assert.deepEqual(saves, ['data', 'data', 'data', 'data']);
    assert.equal(data.getByName('Token'), 2);
  });

  it('needs a subject to grant and nothing to revoke', () => {
    const saves = [];
    const out = sink();
    assert.throws(
      () => runTokens({ grantToken: 'x' }, saving('data', saves), saving('tokens', saves), 0, out),
      { message: '--grant-token needs a --subject.' },
    );
    assert.deepEqual(saves, [], 'a refused grant saves nothing');
    runTokens({ revokeToken: 'x' }, saving('data', saves), null, 0, out);
    assert.equal(out.text, 'Unknown token.\n');
    // Without a token to grant or revoke there is nothing to print, still a save.
    writeTokens({}, LinkStorage.inMemory(), 1, out);
    runTokens({}, saving('data', saves), null, 0, out);
    assert.equal(out.text, 'Unknown token.\n');
    assert.deepEqual(saves, ['data', 'data']);
  });
});

describe('runPermittedQuery', () => {
  const pipeline = (action) => (_cli, view, stage) => {
    assert.ok(view instanceof PermissionsDecorator);
    assert.equal(stage({ always: true }, view, TRIGGER), false, 'no trigger stage');
    action(view);
  };

  it('commits what the subject may write', () => {
    const world = new World();
    const cli = { subject: 'boss' };
    let seen;
    runPermittedQuery(
      cli,
      world.links,
      world.policy,
      world.v.boss,
      pipeline((view) => {
        seen = view;
        assert.equal(view.subject(), world.v.boss);
        assert.equal(view.inner().recording(), true, 'the query is recorded');
        view.create(world.v.note, world.v.note);
      }),
    );
    assert.equal(seen.inner().recording(), false, 'committed');
    assert.equal(seen.inner().rollback().length, 0, 'nothing is left to undo');
    assert.equal(world.links.search(world.v.note, world.v.note) != null, true);
  });

  it('rethrows a link error, even under a context, after undoing the query', () => {
    const world = new World();
    const linkError = new NotFoundError(99);
    for (const thrown of [linkError, new Error('while querying', { cause: linkError })]) {
      const before = indexes(world.links);
      const caught = (() => {
        try {
          runPermittedQuery(
            {},
            world.links,
            world.policy,
            world.v.boss,
            pipeline((view) => {
              view.create(world.v.note, world.v.note);
              throw thrown;
            }),
          );
        } catch (error) {
          return error;
        }
        return assert.fail('the query fails');
      })();
      assert.equal(caught, thrown, 'as it was thrown');
      assert.deepEqual(indexes(world.links), before, 'rolled back');
    }
  });

  it('reports any other failure as a query error, its causes joined', () => {
    const world = new World();
    const before = indexes(world.links);
    for (const [subject, action, message] of [
      [0, (view) => view.create(0, 0), 'Query error: Forbidden: create'],
      [
        world.v.alice,
        (view) => view.update(world.v.note, world.t.Public, world.v.note),
        `Query error: Forbidden: update ${world.v.note}`,
      ],
      [
        world.v.boss,
        (view) => {
          view.create(world.v.note, world.v.note);
          throw new Error('outer', { cause: new Error('middle', { cause: new Error('inner') }) });
        },
        'Query error: outer: middle: inner',
      ],
    ]) {
      assert.throws(
        () => runPermittedQuery({}, world.links, world.policy, subject, pipeline(action)),
        (error) =>
          error instanceof QueryError && error instanceof LinkError && error.message === message,
      );
      assert.deepEqual(indexes(world.links), before, `rolled back: ${message}`);
    }
  });
});

describe('runTriggerCommand', () => {
  /** A stand-in for the decorator that tells what it was asked. */
  function triggers() {
    const calls = [];
    return {
      calls,
      storeTrigger: (kind, query) => {
        calls.push(['store', kind, query]);
        return 17;
      },
      removeTriggers: (query) => {
        calls.push(['remove', query]);
        return 2;
      },
    };
  }

  it('takes no query without a trigger command', () => {
    const storage = triggers();
    const out = sink();
    assert.equal(runTriggerCommand({}, storage, TRIGGER, out), false);
    assert.equal(runTriggerCommand({ triggers: true }, storage, null, out), false);
    assert.deepEqual(storage.calls, []);
    assert.equal(out.text, '');
  });

  it('needs a query', () => {
    for (const flag of ['always', 'once', 'never']) {
      for (const query of [null, undefined, '', '  ', '\n\t']) {
        assert.throws(() => runTriggerCommand({ [flag]: true }, triggers(), query, sink()), {
          message: '--always, --once, and --never require a query.',
        });
      }
    }
  });

  it('stores always and once triggers and removes them with never', () => {
    const storage = triggers();
    const out = sink();
    assert.equal(runTriggerCommand({ always: true }, storage, TRIGGER, out), true);
    assert.equal(runTriggerCommand({ once: true }, storage, ` ${TRIGGER} `, out), true);
    assert.equal(runTriggerCommand({ never: true }, storage, TRIGGER, out), true);
    assert.deepEqual(storage.calls, [
      ['store', 'Always', TRIGGER],
      ['store', 'Once', ` ${TRIGGER} `],
      ['remove', TRIGGER],
    ]);
    assert.equal(
      out.text,
      'Always persistent transformation trigger stored: 17\n' +
        'Once persistent transformation trigger stored: 17\n' +
        'Persistent transformation triggers removed: 2\n',
    );
  });
});

/** Runs `main` in-process on `db` in `dir`: its status, standard output and standard error. */
async function clink(dir, db, ...args) {
  const [stdout, stderr] = [sink(), sink()];
  const status = await main(['--db', join(dir, db), ...args], { stdout, stderr });
  return { status, stdout: stdout.text, stderr: stderr.text };
}

const ok = (stdout = '') => ({ status: 0, stdout, stderr: '' });
const failed = (message) => ({ status: 1, stdout: '', stderr: `Error: ${message}\n` });
const missing = (name, pattern) =>
  failed(
    `Query error: Invalid reference to non-existent link '${name}' in ${pattern} pattern. ` +
      `Link '${name}' does not exist and will not be created by this operation. ` +
      'Use --auto-create-missing-references to create missing references as point links.',
  );

const WORLD =
  '() ((Type: Type Type) (Extends: Extends Extends) (AllowRead: AllowRead AllowRead) ' +
  '(AllowWrite: AllowWrite AllowWrite) (AllowReference: AllowReference AllowReference) ' +
  '(Anything: Anything Anything) (User: Type User) (Admin: User Admin) (Doc: Type Doc) ' +
  '(Public: Type Public) (Note: Doc Note) (Secret: Doc Secret) (alice: User alice) ' +
  '(boss: Admin boss) (note: Note note) (secret: Secret secret) (news: Public news) ' +
  '(AllowWrite (Admin Anything)) (AllowWrite (User Note)) (AllowRead (User Public)))';
const WORLD_AFTER =
  '(Type: Type Type)\n(Extends: Extends Extends)\n(AllowRead: AllowRead AllowRead)\n' +
  '(AllowWrite: AllowWrite AllowWrite)\n(AllowReference: AllowReference AllowReference)\n' +
  '(Anything: Anything Anything)\n(User: Type User)\n(Admin: User Admin)\n(Doc: Type Doc)\n' +
  '(Public: Type Public)\n(Note: Doc Note)\n(Secret: Doc Secret)\n(alice: User alice)\n' +
  '(boss: Admin boss)\n(note: Note note)\n(secret: Secret secret)\n(news: Public news)\n' +
  '(18: Admin Anything)\n(19: AllowWrite 18)\n(20: User Note)\n(21: AllowWrite 20)\n' +
  '(22: User Public)\n(23: AllowRead 22)\n';
const ALICE_SEES = '(note: Note note)\n(news: Public news)\n';

describe('clink permissions, end to end', { skip: endToEnd }, () => {
  /** A directory whose data.links holds the world of the permission tests. */
  async function world() {
    const dir = fresh();
    assert.deepEqual(
      await clink(dir, 'data.links', '--auto-create-missing-references', WORLD),
      ok(),
    );
    assert.deepEqual(await clink(dir, 'data.links', '--after'), ok(WORLD_AFTER));
    return dir;
  }

  it('a subject sees what its rules allow and writes nothing else', async () => {
    const dir = await world();
    const as = (...args) => clink(dir, 'data.links', '--permissions', 'self', ...args);
    for (const subject of ['alice', '13', ' alice ']) {
      assert.deepEqual(await as('--subject', subject, '--after'), ok(ALICE_SEES), subject);
    }
    const update = ['--changes', '((note: Note note)) ((note: Public note))'];
    assert.deepEqual(
      await as('--subject', 'alice', ...update),
      failed('Query error: Forbidden: update 15'),
    );
    for (const subject of ['alice', '13']) {
      assert.deepEqual(
        await as('--subject', subject, '--changes', '((secret: * *)) ()'),
        missing('secret', 'restriction'),
      );
    }
    assert.deepEqual(
      await as('--subject', 'alice', '() ((Note secret))'),
      missing('secret', 'substitution'),
    );
    assert.deepEqual(
      await as('--subject', 'alice', '() ((Note nothing))'),
      missing('nothing', 'substitution'),
    );
    assert.deepEqual(
      await clink(dir, 'data.links', '--after'),
      ok(WORLD_AFTER),
      'nothing was written',
    );
    for (const [from, to] of [
      ['Note', 'Secret'],
      ['Secret', 'Note'],
    ]) {
      const change = `((note: ${from} note)) ((note: ${to} note))`;
      assert.deepEqual(await as('--subject', 'boss', '--changes', change), ok(`${change}\n`));
    }
    assert.deepEqual(await clink(dir, 'data.links', '--after'), ok(WORLD_AFTER));
  });

  it('a subject sees nothing under rules apart that allow nothing', async () => {
    const dir = await world();
    const rules = join(dir, 'rules.links');
    const seen = await clink(
      dir,
      'data.links',
      '--permissions',
      rules,
      '--subject',
      'alice',
      '--after',
    );
    assert.deepEqual(seen, ok());
  });

  it('a guest reads nothing and creates nothing', async () => {
    const dir = await world();
    const as = (...args) => clink(dir, 'data.links', '--permissions', 'self', ...args);
    assert.deepEqual(await as('--after'), ok());
    assert.deepEqual(await as('() ((1 2))'), missing('1', 'substitution'));
    assert.deepEqual(
      await as('--auto-create-missing-references', '() ((1 2))'),
      failed('Query error: Forbidden: create'),
    );
    for (const subject of ['nobody', '99999999999']) {
      assert.deepEqual(
        await as('--subject', subject, '--after'),
        failed(`Unknown subject '${subject}'.`),
      );
    }
    assert.deepEqual(await clink(dir, 'data.links', '--after'), ok(WORLD_AFTER));
  });

  it('token commands need a token table and a subject', async () => {
    const dir = await world();
    const on = (...args) => clink(dir, 'data.links', ...args);
    assert.deepEqual(
      await on('--subject', 'alice', '--grant-token', 'x'),
      failed('--grant-token and --revoke-token need --tokens.'),
    );
    assert.deepEqual(
      await on('--tokens', 'self', '--grant-token', 'x'),
      failed('--grant-token needs a --subject.'),
    );
    assert.deepEqual(await on('--tokens', 'self', '--revoke-token', 'x'), ok('Unknown token.\n'));
    assert.deepEqual(
      await on('--permissions', 'self', '--always', '() ((1 1))'),
      failed('--always, --once and --never cannot be used with --permissions.'),
    );
  });

  it('tokens are stored as links, in the data', async () => {
    const dir = await world();
    const tokens = (...args) => clink(dir, 'data.links', '--tokens', 'self', ...args);
    const grant = ['--subject', 'alice', '--grant-token', 'alice-secret'];
    assert.deepEqual(await tokens(...grant), ok('Token granted to subject 13: 26\n'));
    assert.deepEqual(await tokens(...grant), ok('Token granted to subject 13: 26\n'));
    assert.deepEqual(await tokens('--revoke-token', 'alice-secret'), ok('Token revoked.\n'));
    assert.deepEqual(await tokens('--revoke-token', 'alice-secret'), ok('Unknown token.\n'));
    const after = `${WORLD_AFTER}(Token: Token Token)\n(25: Token alice)\n`;
    assert.deepEqual(await clink(dir, 'data.links', '--after'), ok(after));
  });

  it('tokens are stored as links, apart', async () => {
    const dir = await world();
    const tokens = (...args) =>
      clink(dir, 'data.links', '--tokens', join(dir, 'tokens.links'), ...args);
    const grant = ['--subject', 'alice', '--grant-token', 'alice-secret'];
    assert.deepEqual(await tokens(...grant), ok('Token granted to subject 13: 3\n'));
    assert.deepEqual(
      await tokens(...grant, '--revoke-token', 'alice-secret'),
      ok('Token granted to subject 13: 3\nToken revoked.\n'),
    );
    assert.deepEqual(await tokens('--revoke-token', 'alice-secret'), ok('Unknown token.\n'));
    assert.deepEqual(
      await clink(dir, 'tokens.links', '--after'),
      ok('(Token: Token Token)\n(2: Token 13)\n'),
    );
    assert.deepEqual(await clink(dir, 'data.links', '--after'), ok(WORLD_AFTER));
  });
});

describe('clink triggers, end to end', { skip: endToEnd }, () => {
  /** A directory whose `db` holds the two points the trigger scenarios start from. */
  async function seeded(db = 'data.links') {
    const dir = fresh();
    assert.deepEqual(await clink(dir, db, '--query', '() ((1 1) (2 2))'), ok());
    return dir;
  }
  const stored = (kind) => ok(`${kind} persistent transformation trigger stored: 17\n`);
  const removed = (count) => ok(`Persistent transformation triggers removed: ${count}\n`);
  const dump = (dir, ...args) => clink(dir, 'data.links', ...args, '--after');

  it('an always trigger fires on later writes and keeps firing', async () => {
    const dir = await seeded();
    assert.deepEqual(await clink(dir, 'data.links', '--always', TRIGGER), stored('Always'));
    assert.deepEqual(await clink(dir, 'data.links', '--always', TRIGGER), stored('Always'));
    assert.deepEqual(await dump(dir), ok('(1: 1 1)\n(2: 2 2)\n'), 'not fired yet');
    assert.deepEqual(await clink(dir, 'data.links', '--query', '() ((2 1))'), ok());
    assert.deepEqual(await dump(dir), ok('(1: 1 2)\n(2: 2 2)\n(3: 2 1)\n'));
    assert.deepEqual(await clink(dir, 'data.links', '--query', '((1: 1 2)) ((1: 1 1))'), ok());
    assert.deepEqual(await clink(dir, 'data.links', '--query', '() ((2 2))'), ok());
    assert.deepEqual(await dump(dir), ok('(1: 1 2)\n(2: 2 2)\n(3: 2 1)\n'));
  });

  it('a once trigger fires only once', async () => {
    const dir = await seeded();
    assert.deepEqual(await clink(dir, 'data.links', '--once', TRIGGER), stored('Once'));
    assert.deepEqual(await clink(dir, 'data.links', '--query', '() ((2 1))'), ok());
    assert.deepEqual(await dump(dir), ok('(1: 1 2)\n(2: 2 2)\n(3: 2 1)\n'));
    assert.deepEqual(await clink(dir, 'data.links', '--query', '((1: 1 2)) ((1: 1 1))'), ok());
    assert.deepEqual(await clink(dir, 'data.links', '--query', '() ((2 2))'), ok());
    assert.deepEqual(await dump(dir), ok('(1: 1 1)\n(2: 2 2)\n(3: 2 1)\n'));
  });

  it('never removes a stored trigger, and nothing from an empty store', async () => {
    const dir = await seeded();
    assert.deepEqual(await clink(dir, 'data.links', '--never', TRIGGER), removed(0));
    assert.deepEqual(await clink(dir, 'data.links', '--always', TRIGGER), stored('Always'));
    assert.deepEqual(await clink(dir, 'data.links', '--never', TRIGGER), removed(1));
    assert.deepEqual(await clink(dir, 'data.links', '--never', TRIGGER), removed(0));
    assert.deepEqual(await clink(dir, 'data.links', '--query', '() ((2 1))'), ok());
    assert.deepEqual(await dump(dir), ok('(1: 1 1)\n(2: 2 2)\n(3: 2 1)\n'));
  });

  it('triggers are stored in the sidecar next to the database', async () => {
    const dir = await seeded();
    assert.deepEqual(await clink(dir, 'data.links', '--always', TRIGGER), stored('Always'));
    assert.equal(existsSync(join(dir, 'data.triggers.links')), true);
    assert.deepEqual(await dump(dir), ok('(1: 1 1)\n(2: 2 2)\n'));
  });

  it('--triggers-file puts the store where it is asked to', async () => {
    const dir = await seeded();
    const elsewhere = ['--triggers-file', join(dir, 'elsewhere.links')];
    assert.deepEqual(
      await clink(dir, 'data.links', ...elsewhere, '--once', TRIGGER),
      stored('Once'),
    );
    assert.equal(existsSync(join(dir, 'elsewhere.links')), true);
    assert.equal(existsSync(join(dir, 'data.triggers.links')), false);
    assert.deepEqual(await clink(dir, 'data.links', '--query', '() ((2 1))'), ok());
    assert.deepEqual(await dump(dir), ok('(1: 1 1)\n(2: 2 2)\n(3: 2 1)\n'));
    assert.deepEqual(await clink(dir, 'data.links', ...elsewhere, '--query', '() ((2 3))'), ok());
    assert.deepEqual(await dump(dir), ok('(1: 1 2)\n(2: 2 2)\n(3: 2 1)\n(4: 2 3)\n'));
  });

  it('embedded triggers stay in the main database', async () => {
    const dir = await seeded();
    const embedded = await clink(dir, 'data.links', '--embed-triggers', '--always', TRIGGER);
    assert.deepEqual(embedded, ok('Always persistent transformation trigger stored: 19\n'));
    assert.equal(existsSync(join(dir, 'data.triggers.links')), false);
    const condition = "'__persistent_transformation:condition:((1: 1 1))'";
    const substitution = "'__persistent_transformation:substitution:((1: 1 2))'";
    const schema =
      '(Type: Type Type)\n(Trigger: Trigger Trigger)\n(Once: Once Once)\n(Always: Always Always)\n' +
      '(Condition: Condition Condition)\n(Substitution: Substitution Substitution)\n' +
      '(9: Type Trigger)\n(10: Trigger Once)\n(11: Trigger Always)\n(12: Type Condition)\n' +
      '(13: Type Substitution)\n' +
      `(${condition}: ${condition} ${condition})\n` +
      `(${substitution}: ${substitution} ${substitution})\n` +
      `(16: Condition ${condition})\n(17: Substitution ${substitution})\n(18: 16 17)\n(19: Always 18)\n`;
    assert.deepEqual(await dump(dir), ok(`(1: 1 1)\n(2: 2 2)\n${schema}`));
    assert.deepEqual(
      await clink(dir, 'data.links', '--embed-triggers', '--query', '() ((2 1))'),
      ok(),
    );
    assert.deepEqual(await dump(dir), ok(`(1: 1 2)\n(2: 2 2)\n${schema}(20: 2 1)\n`));
  });

  it('refuses two trigger commands and a trigger command without a query', async () => {
    const dir = fresh();
    assert.deepEqual(
      await clink(dir, 'data.links', '--always', '--once', TRIGGER),
      failed('Only one of --always, --once, or --never can be used at a time.'),
    );
    for (const args of [['--always'], ['--always', '  '], ['--never', '']]) {
      assert.deepEqual(
        await clink(dir, 'data.links', ...args),
        failed('--always, --once, and --never require a query.'),
        args.join(' '),
      );
    }
  });
});
