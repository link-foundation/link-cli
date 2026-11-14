// Stored triggers: the cases of rust/tests/persistent_transformations_tests.rs
// and rust/tests/persistent_transformations_edges_tests.rs, over the store of
// link-storage.js; expected texts are those of the Rust `clink`.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, existsSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { link } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import {
  deleteTriggerRoot,
  INTERNAL_NAME_PREFIX,
  makeTriggersDatabaseFilename,
  PersistentTransformation,
  PersistentTransformationDecorator,
  PersistentTransformationKind as Kind,
  PersistentTransformationQuery,
  storeTriggerIn,
  TriggerStore,
  triggersIn,
} from '../src/persistent-transformations.js';
import { QueryProcessor } from '../src/query-processor.js';

const REWRITE = '(((1: 1 1)) ((1: 1 2)))';
const RESTORE = '(((1: 1 2)) ((1: 1 1)))';
const CREATE_SELF_LINK = '(() ((1: 1 1)))';
const OTHER = '(((5: 5 5)) ((5: 5 6)))';
const MISSING_PARTS =
  'Persistent transformation query must contain a condition and a substitution.';
const ZZZ_ERROR =
  "Query error: Invalid reference to non-existent link 'zzz' in substitution pattern. " +
  "Link 'zzz' does not exist and will not be created by this operation. " +
  'Use --auto-create-missing-references to create missing references as point links.';

const store = () => LinkStorage.inMemory();
const embedded = (options) => PersistentTransformationDecorator.embedded(store(), false, options);
const sidecar = () =>
  PersistentTransformationDecorator.withSidecar(
    store(),
    store(),
    false,
  ).withAutoCreateMissingReferences(true);
const processQuery = (decorator, query) =>
  new QueryProcessor(false).withAutoCreateMissingReferences(true).processQuery(decorator, query);
const lines = () => {
  const written = [];
  return { written, write: (text) => written.push(text) };
};
const withDirectory = (action) => {
  const directory = mkdtempSync(join(tmpdir(), 'clink-triggers-'));
  try {
    action(directory);
  } finally {
    rmSync(directory, { recursive: true, force: true });
  }
};

describe('PersistentTransformationQuery', () => {
  it('accepts the wrapped and the bare form', () => {
    const wrapped = PersistentTransformationQuery.parse(REWRITE);
    const bare = PersistentTransformationQuery.parse('((1: 1 1)) ((1: 1 2))');
    assert.deepEqual(wrapped, bare);
    assert.equal(wrapped.condition, '((1: 1 1))');
    assert.equal(wrapped.substitution, '((1: 1 2))');
    assert.equal(wrapped.query(), REWRITE);
  });

  it('needs both of its parts', () => {
    for (const query of ['(1)', '((1: 1 1))', '()', '', 'x']) {
      assert.throws(() => PersistentTransformationQuery.parse(query), { message: MISSING_PARTS });
    }
  });

  it('formats the halves as the Rust clink does', () => {
    const cases = [
      ['((a b) ("x y": c))', "((a b) ('x y': c))"],
      ['(() (()))', '(() (()))'],
      ['((1: 1 1)) ((1: 1 2)) ((3 3))', REWRITE],
      ["(('it''s' x) (y))", '(("it\'s" x) (y))'],
      ['(a b)', '(a b)'],
      ['((n: ) (m))', '(n (m))'],
      ['()\n(())', '(() (()))'],
      ['(a)\n(b)', '((a) (b))'],
    ];
    for (const [query, normalised] of cases) {
      assert.equal(PersistentTransformationQuery.parse(query).query(), normalised, query);
    }
  });
});

describe('makeTriggersDatabaseFilename', () => {
  it('follows the names database convention, with the paths of Rust', () => {
    const cases = [
      ['/tmp/example.links', '/tmp/example.triggers.links'],
      ['example.links', 'example.triggers.links'],
      ['data.links', 'data.triggers.links'],
      ['a/x.y.links', 'a/x.y.triggers.links'],
      ['./a/b/z.links', './a/b/z.triggers.links'],
      ['a//b/w', 'a//b/w.triggers.links'],
      ['a/./b/.hidden', 'a/./b/.hidden.triggers.links'],
      ['a/b/c.', 'a/b/c.triggers.links'],
      ['dir//x.links', 'dir/x.triggers.links'],
      ['./x', './x.triggers.links'],
      ['.hidden', '.hidden.triggers.links'],
      ['a.b.c', 'a.b.triggers.links'],
      ['a/./q.links', 'a/q.triggers.links'],
      ['a/b/..x', 'a/b/..triggers.links'],
      ['', '.triggers.links'],
      ['/', '.triggers.links'],
      ['.', '.triggers.links'],
      ['/x.links', '/x.triggers.links'],
      ['a/b/', 'a/b.triggers.links'],
      ['a/b/..', 'a/b/.triggers.links'],
    ];
    for (const [database, sidecarPath] of cases) {
      assert.equal(makeTriggersDatabaseFilename(database), sidecarPath, database);
    }
  });
});

describe('TriggerStore', () => {
  it('is embedded or a sidecar', () => {
    assert.equal(TriggerStore.embedded().isEmbedded(), true);
    const links = store();
    const side = TriggerStore.sidecar(links);
    assert.equal(side.isEmbedded(), false);
    assert.equal(side.store, links);
  });
});

describe('PersistentTransformationDecorator', () => {
  it('stores an Always trigger in links and applies it after a write', () => {
    const decorator = sidecar();
    const root = decorator.storeTrigger(Kind.Always, REWRITE);
    assert.notEqual(root, 0);
    const stored = decorator.triggers();
    assert.equal(stored.length, 1);
    assert.equal(stored[0].kind, Kind.Always);
    assert.equal(stored[0].condition, '((1: 1 1))');
    assert.equal(stored[0].substitution, '((1: 1 2))');
    processQuery(decorator, CREATE_SELF_LINK);
    assert.deepEqual(decorator.getLink(1), link(1, 1, 2));
  });

  it('deletes a Once trigger after its first match', () => {
    const decorator = sidecar();
    decorator.storeTrigger(Kind.Once, REWRITE);
    processQuery(decorator, CREATE_SELF_LINK);
    assert.deepEqual(decorator.triggers(), []);
    processQuery(decorator, RESTORE);
    assert.deepEqual(decorator.getLink(1), link(1, 1, 1));
  });

  it('keeps firing an Always trigger', () => {
    const decorator = sidecar();
    decorator.storeTrigger(Kind.Always, REWRITE);
    processQuery(decorator, CREATE_SELF_LINK);
    assert.deepEqual(decorator.getLink(1), link(1, 1, 2));
    processQuery(decorator, RESTORE);
    assert.deepEqual(decorator.getLink(1), link(1, 1, 2));
    assert.equal(decorator.triggers().length, 1);
  });

  it('removes the matching trigger, and nothing the second time', () => {
    const decorator = sidecar();
    decorator.storeTrigger(Kind.Always, REWRITE);
    assert.equal(decorator.removeTriggers(REWRITE), 1);
    assert.deepEqual(decorator.triggers(), []);
    assert.equal(decorator.removeTriggers(REWRITE), 0);
  });

  it('stores the same trigger twice as one', () => {
    const decorator = sidecar();
    const first = decorator.storeTrigger(Kind.Always, REWRITE);
    const second = decorator.storeTrigger(Kind.Always, '((1: 1 1)) ((1: 1 2))');
    assert.equal(first, second);
    assert.equal(decorator.triggers().length, 1);
  });

  it('keeps the bookkeeping of a sidecar out of the data, and saves both', () => {
    withDirectory((directory) => {
      const database = join(directory, 'db.links');
      const open = () =>
        PersistentTransformationDecorator.withSidecar(
          new LinkStorage(database),
          new LinkStorage(makeTriggersDatabaseFilename(database)),
          false,
        );
      const decorator = open();
      decorator.storeTrigger(Kind.Always, REWRITE);
      assert.deepEqual(decorator.allLinks(), []);
      decorator.save();
      assert.equal(existsSync(makeTriggersDatabaseFilename(database)), true);
      const stored = open().triggers();
      assert.equal(stored.length, 1);
      assert.equal(stored[0].query(), REWRITE);
    });
  });

  it('keeps embedded triggers in the data, with no sidecar file', () => {
    withDirectory((directory) => {
      const database = join(directory, 'db.links');
      const decorator = PersistentTransformationDecorator.embedded(
        new LinkStorage(database),
        false,
      ).withAutoCreateMissingReferences(true);
      const root = decorator.storeTrigger(Kind.Always, REWRITE);
      assert.equal(root, 17, 'the root of the Rust clink');
      assert.equal(decorator.exists(root), true);
      assert.equal(decorator.triggers().length, 1);
      decorator.save();
      assert.equal(existsSync(database), true);
      assert.equal(existsSync(makeTriggersDatabaseFilename(database)), false);
    });
  });

  it('hands out its parts', () => {
    const decorator = embedded().withAutoCreateMissingReferences(true);
    assert.equal(decorator.autoCreateMissingReferences(), true);
    decorator.setAutoCreateMissingReferences(false);
    assert.equal(decorator.autoCreateMissingReferences(), false);
    decorator.inner().create(0, 0);
    assert.equal(decorator.inner().allLinks().length, 1);
    assert.equal(decorator.triggerStore().isEmbedded(), true);
  });

  it('reads only a whole record as a trigger', () => {
    const decorator = embedded();
    const root = decorator.storeTrigger(Kind.Always, REWRITE);
    const kept = decorator.triggers();
    assert.equal(kept.length, 1);
    assert.deepEqual(
      kept[0],
      new PersistentTransformation(root, Kind.Always, '((1: 1 1))', '((1: 1 2))'),
    );
    const links = decorator.inner();
    const { source: always, target: payload } = links.getLink(root);
    const record = links.getLink(payload);

    // No payload, a payload of nothing known, and of the wrong records.
    links.create(always, 9999);
    const unknown = links.create(9998, 9997);
    links.create(always, unknown);
    const half = links.create(record.source, 9996);
    links.create(always, half);
    const swapped = links.create(record.target, record.source);
    links.create(always, swapped);
    // Records of the right schema whose text is not the text of their part.
    const plain = links.create(0, 0);
    const condition = links.create(links.getLink(record.source).source, plain);
    const substitution = links.create(links.getLink(record.target).source, plain);
    links.create(always, links.create(condition, substitution));

    assert.deepEqual(decorator.triggers(), kept);
  });

  it('removes the triggers of a query and keeps the others, of each kind', () => {
    const decorator = embedded();
    const rewrite = decorator.storeTrigger(Kind.Always, REWRITE);
    decorator.storeTrigger(Kind.Once, REWRITE);
    const other = decorator.storeTrigger(Kind.Always, OTHER);
    assert.equal(decorator.removeTriggers('(((9: 9 9)) ((9: 9 8)))'), 0);
    assert.equal(decorator.removeTriggers(REWRITE), 2);
    assert.equal(decorator.inner().exists(rewrite), false);
    assert.deepEqual(
      decorator.triggers().map((trigger) => trigger.root),
      [other],
    );
  });

  it('fires nothing for a write that changes nothing', () => {
    const decorator = embedded().withAutoCreateMissingReferences(true);
    const first = decorator.ensureCreated(1);
    assert.equal(decorator.ensureCreated(first), first);
    const pair = decorator.getOrCreate(first, first);
    assert.equal(decorator.getOrCreate(first, first), pair);
    decorator.storeTrigger(Kind.Always, '(() ((70: 70 70)))');
    const before = decorator.allLinks().length;
    assert.equal(decorator.getOrCreate(first, first), pair);
    assert.equal(decorator.ensureCreated(first), first);
    assert.equal(decorator.tryEnsureCreated(first), first);
    assert.equal(decorator.tryGetOrCreate(first, first), pair);
    assert.equal(decorator.allLinks().length, before, 'no trigger ran');
    assert.equal(decorator.search(first, first), pair);
    assert.deepEqual(decorator.queryLinks(pair, null, null), [link(pair, first, first)]);
  });

  it('fires after every kind of write', () => {
    const writes = [
      (decorator) => decorator.create(0, 0),
      (decorator) => decorator.tryCreate(0, 0),
      (decorator) => decorator.ensureCreated(50),
      (decorator) => decorator.tryEnsureCreated(50),
      (decorator) => decorator.getOrCreate(3, 4),
      (decorator) => decorator.tryGetOrCreate(3, 4),
      (decorator) => decorator.update(decorator.inner().create(8, 8), 8, 9),
      (decorator) => decorator.delete(decorator.inner().create(8, 8)),
    ];
    for (const write of writes) {
      const decorator = sidecar();
      decorator.storeTrigger(Kind.Always, '(() ((70: 70 70)))');
      write(decorator);
      assert.deepEqual(decorator.getLink(70), link(70, 70, 70), write.toString());
    }
  });

  it('forwards names and reads to the store it decorates', () => {
    const decorator = embedded();
    const id = decorator.inner().create(0, 0);
    assert.equal(decorator.setName(id, 'named'), id);
    assert.equal(decorator.getName(id), 'named');
    assert.equal(decorator.getByName('named'), id);
    decorator.removeName(id);
    assert.equal(decorator.getName(id), null);
    assert.equal(decorator.exists(id), true);
    assert.deepEqual(decorator.queryLinks(), [link(id, 0, 0)]);
  });

  it('traces what it stores and deletes', () => {
    const out = lines();
    const replayed = [];
    const decorator = PersistentTransformationDecorator.withSidecar(store(), store(), true, {
      stdout: out,
      stderr: (line) => replayed.push(line),
    }).withAutoCreateMissingReferences(true);
    const root = decorator.storeTrigger(Kind.Always, '((1: 1 1)) ((1: 1 2))');
    const once = decorator.storeTrigger(Kind.Once, '(() ((7: 7 7)))');
    decorator.removeTriggers(REWRITE);
    decorator.create(5, 5);
    assert.deepEqual(out.written, [
      `[PersistentTransformation] Stored Always trigger #17: ${REWRITE}\n`,
      `[PersistentTransformation] Stored Once trigger #${once}: (() ((7: 7 7)))\n`,
      `[PersistentTransformation] Deleted trigger #17\n`,
      `[PersistentTransformation] Deleted trigger #${once}\n`,
    ]);
    assert.equal(root, 17);
    assert.deepEqual(decorator.getLink(7), link(7, 7, 7));
    assert.ok(replayed.length > 0, 'the replayed queries trace too');
  });

  it('surfaces a trigger failing after a write that cannot fail at the next one that can', () => {
    const failing = () => {
      const decorator = embedded();
      decorator.storeTrigger(Kind.Always, '(() ((zzz zzz)))');
      return decorator;
    };
    for (const write of [
      (decorator) => decorator.create(0, 0),
      (decorator) => decorator.ensureCreated(60),
      (decorator) => decorator.getOrCreate(60, 61),
    ]) {
      const decorator = failing();
      write(decorator);
      write(decorator);
      assert.throws(() => decorator.save(), { message: ZZZ_ERROR });
      decorator.save();
    }
    const parked = failing();
    parked.create(0, 0);
    const existing = parked.inner().create(0, 0);
    assert.throws(() => parked.tryEnsureCreated(existing), { message: ZZZ_ERROR });
    assert.equal(parked.tryEnsureCreated(existing), existing);
    for (const write of [
      (decorator) => decorator.tryCreate(0, 0),
      (decorator) => decorator.tryEnsureCreated(60),
      (decorator) => decorator.tryGetOrCreate(60, 61),
      (decorator) => decorator.update(decorator.inner().create(8, 8), 8, 9),
      (decorator) => decorator.delete(decorator.inner().create(8, 8)),
    ]) {
      assert.throws(() => write(failing()), { message: ZZZ_ERROR });
    }
  });

  it('stops replaying after a bookkeeping write that failed, as in Rust', () => {
    const triggers = store();
    const decorator = PersistentTransformationDecorator.withSidecar(store(), triggers, false);
    decorator.storeTrigger(Kind.Always, '(() ((70: 70 70)))');
    triggers.getOrCreateNamed = () => {
      throw new Error('names failed');
    };
    assert.throws(() => decorator.storeTrigger(Kind.Always, OTHER), { message: 'names failed' });
    decorator.create(0, 0);
    assert.equal(decorator.exists(70), false);
  });
});

describe('the trigger records', () => {
  it('reads no triggers from a store without the schema', () => {
    assert.deepEqual(triggersIn(store()), []);
  });

  it('stores a trigger idempotently, with internal names, and deletes its root', () => {
    const links = store();
    const parsed = PersistentTransformationQuery.parse(REWRITE);
    const root = storeTriggerIn(links, Kind.Once, parsed);
    assert.equal(storeTriggerIn(links, Kind.Once, parsed), root);
    assert.notEqual(links.getByName(`${INTERNAL_NAME_PREFIX}condition:((1: 1 1))`), null);
    assert.notEqual(links.getByName(`${INTERNAL_NAME_PREFIX}substitution:((1: 1 2))`), null);
    assert.equal(triggersIn(links)[0].kind, Kind.Once);
    assert.equal(deleteTriggerRoot(links, root), true);
    assert.equal(deleteTriggerRoot(links, root), false);
    assert.deepEqual(triggersIn(links), []);
  });
});
