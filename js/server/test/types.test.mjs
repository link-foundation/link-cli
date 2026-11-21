// The type hierarchy of issue #148, case for case the Rust
// rust/tests/types_decorator_tests.rs.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { join } from 'node:path';
import { LinkStorage } from '../src/link-storage.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { pinnedTypes } from '../src/pinned-types.js';
import {
  EXTENDS_NAME,
  ensurePoint,
  ROOT_TYPE_NAME,
  TypeMarkers,
  TypesDecorator,
} from '../src/types.js';
import { capture, scratchDirectory } from './helpers/storage.mjs';

const link = (index, source, target) => ({ index, source, target });
const types = () => TypesDecorator.withNamedMarkers(LinkStorage.inMemory());

/**
 * The world of ui-links' test helpers:
 *
 *   Type -> User -> Admin              alice: User, boss: Admin
 *   Type -> Doc -> Note -> Shared      Shared extends Public too
 *   Type -> Doc -> Secret
 *   Type -> Public
 */
function world() {
  const decorator = types();
  const t = {};
  const v = {};
  const define = (name, parents) => {
    const ids = parents.map((parent) => t[parent]);
    const id = ids.length === 0 ? decorator.defineRoot() : decorator.define(ids[0], ids.slice(1));
    decorator.setName(id, name);
    t[name] = id;
  };
  const instance = (name, typeName) => {
    v[name] = decorator.instance(t[typeName]);
    decorator.setName(v[name], name);
  };
  define('User', []);
  define('Admin', ['User']);
  define('Doc', []);
  define('Public', []);
  define('Note', ['Doc']);
  define('Shared', ['Note', 'Public']);
  define('Secret', ['Doc']);
  instance('alice', 'User');
  instance('boss', 'Admin');
  instance('note', 'Note');
  instance('shared', 'Shared');
  instance('secret', 'Secret');
  return { types: decorator, t, v };
}

describe('types decorator', () => {
  it('decorates any layer, including another decorator', () => {
    const stacked = new TypesDecorator(types(), 1, 2);
    assert.equal(stacked.root(), 1);
    assert.equal(stacked.links.exists(1), true);
    assert.deepEqual(stacked.markers(), new TypeMarkers(1, 2));
    assert.equal(new TypesDecorator(LinkStorage.inMemory(), 1).extends(), null);
  });

  it('keeps its markers as named points', () => {
    const decorator = types();
    const [root, extendsMarker] = [decorator.root(), decorator.extends()];
    assert.deepEqual(decorator.getLink(root), link(root, root, root));
    assert.deepEqual(
      decorator.getLink(extendsMarker),
      link(extendsMarker, extendsMarker, extendsMarker),
    );
    assert.equal(decorator.getByName(ROOT_TYPE_NAME), root);
    assert.equal(decorator.getByName(EXTENDS_NAME), extendsMarker);
    const again = TypesDecorator.withNamedMarkers(decorator.links);
    assert.deepEqual([again.root(), again.extends()], [root, extendsMarker]);
  });

  it('remakes a named point whose link is gone', () => {
    const links = LinkStorage.inMemory();
    links.setName(0, 'Zero');
    assert.equal(ensurePoint(links, 'Zero'), 1);
    links.setName(7, 'Gone');
    assert.equal(ensurePoint(links, 'Gone'), 2);
    assert.deepEqual(links.getLink(2), link(2, 2, 2));
    assert.equal(links.getByName('Gone'), 2);
  });

  it('follows sub-types and multiple inheritance', () => {
    const { types: decorator, t, v } = world();
    assert.equal(decorator.isA(v.shared, t.Doc), true);
    assert.equal(decorator.isA(v.shared, t.Public), true);
    assert.equal(decorator.isA(v.note, t.Public), false);
    assert.equal(decorator.isA(v.boss, t.User), true);
    assert.equal(decorator.isA(t.Shared, t.Shared), true);
    assert.equal(decorator.isOf(t.Shared, t.Shared), false);
    assert.equal(decorator.typeOf(v.secret), t.Secret);
    assert.equal(decorator.isType(t.Note), true);
    assert.equal(decorator.isType(9999), false);
    assert.deepEqual(decorator.parentsOf(t.Shared), [t.Note, t.Public]);
    assert.deepEqual(decorator.ancestorsOf(t.Shared), [
      t.Shared,
      t.Note,
      t.Public,
      t.Doc,
      decorator.root(),
    ]);
  });

  it('checks a link that is not stored yet', () => {
    const { types: decorator, t } = world();
    assert.equal(decorator.isALink(link(0, t.Note, 0), t.Doc), true);
    assert.equal(decorator.isALink(link(0, t.Note, 0), t.Public), false);
    assert.equal(decorator.isALink(link(9999, t.Note, 0), t.Doc), true);
    assert.equal(decorator.isALink(link(t.Doc, 0, 0), t.Doc), true);
  });

  it('needs an Extends marker for more parents', () => {
    const links = LinkStorage.inMemory();
    const root = ensurePoint(links, 'Type');
    const decorator = new TypesDecorator(links, root);
    assert.equal(decorator.extends(), null);
    const message = 'Multiple parents need an Extends marker.';
    assert.throws(() => decorator.define(root, [root]), { message });
    assert.throws(() => decorator.extend(root, root), { message });
    const single = decorator.define(root);
    assert.deepEqual(decorator.parentsOf(single), [root]);
    assert.equal(decorator.isExtension(single), false);
  });

  it('builds the hierarchy as links', () => {
    const decorator = types();
    const typeId = decorator.root();
    const user = decorator.define(typeId, []);
    const admin = decorator.define(user, []);
    const doc = decorator.define(typeId, []);
    const publicType = decorator.define(typeId, []);
    const note = decorator.define(doc, []);
    const news = decorator.define(note, [doc, publicType]);
    assert.deepEqual(decorator.getLink(user), link(user, typeId, user));
    assert.deepEqual(decorator.getLink(admin), link(admin, user, admin));
    assert.deepEqual(decorator.getLink(news), link(news, note, news));
    for (const parent of [doc, publicType]) {
      const pair = decorator.search(news, parent);
      assert.notEqual(decorator.search(decorator.extends(), pair), null);
      assert.equal(decorator.isExtension(pair), true);
    }
    assert.equal(decorator.isA(news, doc), true);
    assert.equal(decorator.isA(news, publicType), true);
    assert.equal(decorator.isA(news, note), true);
    assert.equal(decorator.isA(news, typeId), true);
    assert.equal(decorator.isA(news, user), false);
    assert.equal(decorator.isA(admin, user), true);
    assert.deepEqual(decorator.parentsOf(news), [note, doc, publicType]);
    const item = decorator.instance(news);
    assert.equal(decorator.isA(item, doc), true);
    assert.equal(decorator.isA(item, publicType), true);
    assert.equal(decorator.typeOf(item), news);
  });

  it('follows depth three and more', () => {
    const decorator = types();
    const a = decorator.defineRoot();
    const b = decorator.define(a);
    const c = decorator.define(b);
    const d = decorator.define(c);
    const e = decorator.define(d);
    const value = decorator.instance(e);
    for (const typeId of [e, d, c, b, a, decorator.root()]) {
      assert.equal(decorator.isA(value, typeId), true);
      assert.equal(decorator.isOf(value, typeId), true);
    }
    assert.equal(decorator.isA(a, b), false);
    assert.deepEqual(decorator.ancestorsOf(e), [e, d, c, b, a, decorator.root()]);
  });

  it('visits a diamond once', () => {
    const decorator = types();
    const top = decorator.defineRoot();
    const left = decorator.define(top);
    const right = decorator.define(top);
    const bottom = decorator.define(left, [right]);
    const value = decorator.instance(bottom);
    for (const typeId of [top, left, right]) assert.equal(decorator.isA(value, typeId), true);
    assert.deepEqual(decorator.ancestorsOf(bottom), [bottom, left, right, top, decorator.root()]);
  });

  it('terminates on an Extends cycle', () => {
    const decorator = types();
    const a = decorator.defineRoot();
    const b = decorator.defineRoot();
    decorator.extend(a, b);
    decorator.extend(b, a);
    const unrelated = decorator.defineRoot();
    const valueOfA = decorator.instance(a);
    const valueOfB = decorator.instance(b);
    const root = decorator.root();
    assert.deepEqual(decorator.parentsOf(a), [root, b]);
    assert.deepEqual(decorator.parentsOf(b), [root, a]);
    assert.equal(decorator.isA(valueOfA, b), true);
    assert.equal(decorator.isA(valueOfB, a), true);
    assert.equal(decorator.isOf(a, b), true);
    assert.equal(decorator.isOf(b, a), true);
    assert.equal(decorator.isA(valueOfA, unrelated), false);
    assert.equal(decorator.isOf(a, a), true);
    assert.deepEqual(decorator.ancestorsOf(a), [a, root, b]);
    assert.deepEqual(decorator.ancestorsOf(b), [b, root, a]);
  });

  it('refuses to extend a direct sub-type', () => {
    const decorator = types();
    const a = decorator.defineRoot();
    const b = decorator.define(a);
    assert.throws(() => decorator.extend(a, b), {
      message: `Type ${b} is a direct sub-type of ${a}: the pair (${a} ${b}) would be its definition.`,
    });
    assert.equal(decorator.isExtension(b), false);
    assert.deepEqual(decorator.parentsOf(b), [a]);
    assert.deepEqual(decorator.parentsOf(a), [decorator.root()]);
  });

  it('terminates on a source cycle', () => {
    const decorator = types();
    const a = decorator.defineRoot();
    const b = decorator.define(a);
    decorator.update(a, b, a);
    const unrelated = decorator.defineRoot();
    assert.deepEqual(decorator.parentsOf(a), [b]);
    assert.deepEqual(decorator.parentsOf(b), [a]);
    assert.equal(decorator.isOf(a, b), true);
    assert.equal(decorator.isOf(a, decorator.root()), false);
    assert.equal(decorator.isA(decorator.instance(a), unrelated), false);
    assert.deepEqual(decorator.ancestorsOf(a), [a, b]);
  });

  it('types the wiring links as Extends', () => {
    const { types: decorator, t } = world();
    const extendsMarker = decorator.extends();
    const pair = decorator.search(t.Shared, t.Public);
    const mark = decorator.search(extendsMarker, pair);
    assert.equal(decorator.isExtension(pair), true);
    assert.equal(decorator.isExtension(mark), false);
    assert.equal(decorator.isExtension(t.Shared), false);
    assert.equal(decorator.isA(pair, extendsMarker), true);
    assert.equal(decorator.isA(mark, extendsMarker), true);
    assert.equal(decorator.isA(pair, t.Shared), false);
    assert.equal(decorator.isA(pair, t.Doc), false);
    assert.equal(decorator.typeOf(mark), extendsMarker);
  });

  it('skips Extends marks on pairs to nothing', () => {
    const decorator = types();
    const a = decorator.defineRoot();
    const pair = decorator.create(a, 0);
    decorator.create(decorator.extends(), pair);
    assert.deepEqual(decorator.parentsOf(a), [decorator.root()]);
    const parents = [];
    decorator.addExtendedTypes(0, parents);
    assert.deepEqual(parents, []);
  });

  it('makes instances with a target', () => {
    const { types: decorator, t, v } = world();
    const membership = decorator.instanceWithTarget(t.User, v.boss);
    assert.deepEqual(decorator.getLink(membership), link(membership, t.User, v.boss));
    assert.equal(decorator.typeOf(membership), t.User);
    assert.equal(decorator.isType(membership), false);
    assert.equal(decorator.isA(membership, t.User), true);
    assert.equal(decorator.isA(membership, decorator.root()), true);
    assert.equal(decorator.instanceWithTarget(t.User, v.boss), membership);
    assert.throws(() => decorator.instanceWithTarget(t.User, 9999), {
      message: 'Link 9999 does not exist.',
    });
  });

  it('answers for missing links', () => {
    const { types: decorator } = world();
    const root = decorator.root();
    assert.equal(decorator.typeOf(9999), null);
    assert.equal(decorator.typeOf(0), null);
    assert.equal(decorator.isType(9999), false);
    assert.deepEqual(decorator.parentsOf(9999), []);
    assert.deepEqual(decorator.ancestorsOf(9999), [9999]);
    assert.equal(decorator.isA(9999, 9999), false);
    assert.equal(decorator.isOf(9999, root), false);
    const message = 'Link 9999 does not exist.';
    assert.throws(() => decorator.define(9999), { message });
    assert.throws(() => decorator.define(root, [9999]), { message });
    assert.throws(() => decorator.instance(9999), { message });
    assert.throws(() => decorator.extend(9999, root), { message });
  });

  it('makes the pinned types sub-types of the root', () => {
    const links = LinkStorage.inMemory();
    const [typeId, unicodeSymbol, unicodeSequence] = pinnedTypes(links, 3);
    const extendsMarker = links.create(0, 0);
    links.update(extendsMarker, extendsMarker, extendsMarker);
    const decorator = TypesDecorator.withMarkers(links, new TypeMarkers(typeId, extendsMarker));
    assert.equal(decorator.isType(typeId), true);
    assert.deepEqual(decorator.parentsOf(typeId), []);
    for (const pinnedType of [unicodeSymbol, unicodeSequence]) {
      assert.equal(decorator.isType(pinnedType), true);
      assert.equal(decorator.typeOf(pinnedType), typeId);
      assert.deepEqual(decorator.parentsOf(pinnedType), [typeId]);
      assert.equal(decorator.isOf(pinnedType, typeId), true);
    }
    const text = decorator.define(unicodeSequence, [unicodeSymbol]);
    assert.equal(decorator.isA(decorator.instance(text), typeId), true);
    assert.deepEqual(decorator.ancestorsOf(text), [text, unicodeSequence, unicodeSymbol, typeId]);
    assert.deepEqual(pinnedTypes(decorator.links, 3), [1, 2, 3]);
  });

  it('keeps the hierarchy in a named file store', () => {
    const path = join(scratchDirectory(), 'db.links');
    const open = () =>
      TypesDecorator.withNamedMarkers(NamedTypesDecorator.open(path, { stderr: capture() }));
    const first = open();
    const doc = first.defineRoot();
    const publicType = first.defineRoot();
    const note = first.define(doc);
    const news = first.define(note, [publicType]);
    first.setName(publicType, 'Public');
    first.save();
    const decorator = open();
    assert.equal(decorator.getByName('Public'), publicType);
    assert.deepEqual(decorator.parentsOf(news), [note, publicType]);
    assert.equal(decorator.isA(decorator.instance(news), publicType), true);
  });

  it('forwards every store operation', () => {
    const decorator = types();
    const before = decorator.allLinks().length;
    const created = decorator.create(1, 2);
    assert.deepEqual(decorator.getLink(created), link(created, 1, 2));
    assert.equal(decorator.ensureCreated(20), 20);
    assert.equal(decorator.tryEnsureCreated(21), 21);
    assert.equal(decorator.exists(21), true);
    assert.equal(decorator.referenceExists(21), true);
    assert.equal(decorator.tryCreate(2, 2) > 0, true);
    assert.equal(decorator.tryGetOrCreate(1, 2), created);
    assert.equal(decorator.getOrCreate(1, 2), created);
    assert.deepEqual(decorator.nextCreatedAddresses(2), decorator.nextCreatedAddresses(2));
    const changes = [];
    const observer = (left, right) => changes.push([left.index, right.index]);
    decorator.updateObserved(20, 2, 1, observer);
    decorator.deleteObserved(21, observer);
    assert.deepEqual(changes, [
      [20, 20],
      [21, 0],
    ]);
    assert.equal(decorator.allLinks().length, before + 3);
    assert.deepEqual(decorator.queryLinks(null, 2, 1), [link(20, 2, 1)]);
    assert.equal(decorator.search(2, 1), 20);
    assert.equal(decorator.setName(20, 'twenty'), 20);
    assert.equal(decorator.getName(20), 'twenty');
    decorator.removeName(20);
    assert.equal(decorator.getName(20), null);
    const named = decorator.getOrCreateNamed('named');
    assert.equal(decorator.getOrCreateNamed('named'), named);
    decorator.save();
  });
});
