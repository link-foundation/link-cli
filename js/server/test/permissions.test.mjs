// PermissionPolicy, PermissionMarkers and AccessPolicy: the cases of
// rust/tests/permissions_decorator_tests.rs and the cases of
// rust/tests/permissions_integration_tests.rs that need no other layer
// (roots of any address, policies with type systems of their own or no rules).
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { link } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import {
  AccessPolicy,
  Forbidden,
  LinksTokenTable,
  parsePermissionAccess,
  PermissionAccess,
  PermissionMarkers,
  PermissionPolicy,
  PermissionsDecorator,
} from '../src/permissions.js';
import { TypeMarkers, TypesDecorator } from '../src/types.js';
import { all, errorOf, indexes, World } from './permissions-world.mjs';

/** A new link `(l: source target)` (`make` of rust/tests/common/union_world.rs). */
function make(links, source, target) {
  const id = links.tryCreate(0, 0);
  links.update(id, source, target);
  return id;
}

/** A new link `(l: type l)`. */
function typed(links, type) {
  const id = links.tryCreate(0, 0);
  links.update(id, type, id);
  return id;
}

/** A policy of the application's own: anyone reads, no one writes. */
class ReadOnly extends AccessPolicy {
  constructor(markers) {
    super();
    this.markersOf = markers;
  }

  markers() {
    return this.markersOf;
  }

  canRead() {
    return true;
  }

  canWrite() {
    return false;
  }

  canCreate() {
    return false;
  }
}

describe('PermissionAccess', () => {
  it('parses read, write and reference only', () => {
    assert.equal(parsePermissionAccess('read'), PermissionAccess.Read);
    assert.equal(PermissionPolicy.parseAccess('write'), PermissionAccess.Write);
    assert.equal(PermissionPolicy.parseAccess('reference'), PermissionAccess.Reference);
    assert.throws(() => PermissionPolicy.parseAccess('own'), { message: 'Unknown access: own' });
  });
});

describe('PermissionMarkers', () => {
  it('are named points, found again rather than made twice', () => {
    const w = new World();
    const m = w.markers;
    for (const [marker, name] of [
      [m.allowRead, 'AllowRead'],
      [m.allowWrite, 'AllowWrite'],
      [m.anything, 'Anything'],
      [m.allowReference, 'AllowReference'],
    ]) {
      assert.deepEqual(w.link(marker), link(marker, marker, marker));
      assert.equal(w.links.getByName(name), marker);
    }
    assert.deepEqual(PermissionMarkers.ensure(w.links), m);
    assert.deepEqual([m.allowRead, m.allowWrite, m.anything, m.allowReference], [3, 4, 5, 6]);
    assert.equal(Object.isFrozen(m), true);
  });

  it('mark each access with its own marker', () => {
    const m = new PermissionMarkers(1, 2, 3, 4);
    assert.equal(m.of(PermissionAccess.Read), 1);
    assert.equal(m.of(PermissionAccess.Write), 2);
    assert.equal(m.of(PermissionAccess.Reference), 4);
  });
});

describe('PermissionPolicy', () => {
  it('re-exports the token table and the decorator', () => {
    assert.equal(typeof LinksTokenTable, 'function');
    assert.equal(new Forbidden('x') instanceof Error, true);
  });

  it('implements NamedTypeLinks with the stored policy or an own one', () => {
    const w = new World();
    assert.equal(w.as(w.v.alice).allLinks().length, 3);
    const total = w.links.allLinks().length;
    const readOnly = new PermissionsDecorator(w.links, new ReadOnly(w.markers), 0);
    assert.equal(readOnly.allLinks().length, total);
    assert.equal(
      errorOf(() => readOnly.tryCreate(0, 0)),
      'forbidden',
    );
    // A policy that says nothing of references refers to no unreadable link.
    assert.equal(new ReadOnly(w.markers).canReference(w.links, 1, w.link(1)), false);
  });

  for (const separate of [false, true]) {
    const where = separate ? 'rules apart' : 'rules in data';

    it(`answers by type (${where})`, () => {
      const w = new World(separate);
      const { alice, boss } = w.v;
      const [note, shared, secret] = [w.v.note, w.v.shared, w.v.secret].map((id) => w.link(id));
      const { links, policy } = w;
      assert.equal(policy.canWrite(links, alice, note), true);
      assert.equal(policy.canRead(links, alice, shared), true);
      assert.equal(policy.canRead(links, alice, secret), false);
      assert.equal(policy.canWrite(links, boss, secret), true);
      assert.equal(policy.canRead(links, 0, note), false);
      assert.equal(policy.canCreate(links, alice), true);
      assert.equal(policy.canCreate(links, 0), false);
      assert.deepEqual(policy.types(), w.types);
      assert.deepEqual(policy.markers(), w.markers);
      if (separate) {
        // Three rules, each a pair and its mark.
        assert.equal(w.rules().allLinks().length, 6);
        assert.equal(policy.rulesMut(), w.rules());
      } else {
        assert.equal(policy.rulesMut(), null);
        assert.equal(w.rules(), w.links);
      }
    });

    it(`shows a subject only what it may read (${where})`, () => {
      const w = new World(separate);
      const { t, v } = w;
      const view = w.as(v.alice);
      // The sub-type Shared is of type Note; its Extends pair is not.
      assert.deepEqual(indexes(view), [t.Shared, v.note, v.shared]);
      assert.equal(view.queryLinks(null, null, null).length, 3);
      assert.equal(view.exists(v.secret), false);
      assert.equal(view.exists(v.note), true);
      assert.equal(
        errorOf(() => view.delete(v.secret)),
        'not-found',
      );
      assert.equal(
        errorOf(() => view.update(v.secret, t.Note, v.secret)),
        'not-found',
      );
    });

    it(`refuses writes, the store unchanged (${where})`, () => {
      const w = new World(separate);
      const { t, v } = w;
      const before = all(w.links);
      const view = w.as(v.alice);
      assert.equal(
        errorOf(() => view.update(v.note, t.Secret, v.note)),
        'forbidden',
      );
      assert.equal(
        errorOf(() => view.update(v.shared, t.Public, v.shared)),
        'forbidden',
      );
      view.update(v.shared, t.Note, v.shared);
      view.update(v.shared, t.Shared, v.shared);
      assert.deepEqual(all(view.inner()), before);
      const id = view.tryCreate(0, 0);
      view.update(id, 0, id);
      view.update(id, t.Note, id);
      assert.equal(
        errorOf(() => view.update(id, t.Secret, id)),
        'forbidden',
      );
      view.delete(id);
      assert.deepEqual(all(view.inner()), before);
      assert.equal(
        errorOf(() => w.as(0).tryCreate(0, 0)),
        'forbidden',
      );
      assert.deepEqual(all(w.links), before);
    });

    it(`gives a rule given twice once (${where})`, () => {
      const w = new World(separate);
      const { t } = w;
      const count = w.rules().allLinks().length;
      const rule = w.rules().search(t.User, t.Note);
      assert.notEqual(rule, null);
      assert.equal(w.allow(PermissionAccess.Write, t.User, t.Note), rule);
      assert.equal(w.rules().allLinks().length, count);
      assert.equal(w.policy.canWrite(w.links, w.v.alice, w.link(w.v.note)), true);
      // The same pair may carry both marks.
      assert.equal(w.allow(PermissionAccess.Read, t.User, t.Note), rule);
      assert.equal(w.rules().allLinks().length, count + 1);
    });
  }

  it('keeps the hierarchy from who may write its values', () => {
    const w = new World();
    const { t, v } = w;
    const pair = w.links.search(t.Shared, t.Public);
    assert.notEqual(pair, null);
    w.allow(PermissionAccess.Reference, t.User, t.Admin);
    const view = w.as(v.alice);
    // The link of Note itself is of type Doc.
    assert.equal(
      errorOf(() => view.update(t.Note, t.Admin, t.Note)),
      'not-found',
    );
    // The pair Shared -> Public is of type Extends.
    assert.equal(
      errorOf(() => view.update(pair, t.Shared, t.Admin)),
      'not-found',
    );
    const more = view.tryCreate(0, 0);
    view.update(more, t.Shared, t.Admin);
    const mark = view.tryCreate(0, 0);
    assert.equal(
      errorOf(() => view.update(mark, w.types.extends, more)),
      'forbidden',
    );
    w.as(v.boss).update(pair, t.Shared, t.Public);
  });

  it('lets a subject refer to itself, by a reference rule, or to an object of its rules', () => {
    const w = new World();
    const { t, v, links, policy } = w;
    assert.equal(policy.canReference(links, v.alice, w.link(v.alice)), true);
    assert.equal(policy.canReference(links, 0, w.link(1)), false);
    assert.equal(policy.canReference(links, v.alice, w.link(v.boss)), false);
    assert.equal(policy.canReference(links, v.alice, w.link(t.Note)), true);
    assert.equal(policy.canReference(links, v.alice, w.link(t.Public)), true);
    w.allow(PermissionAccess.Reference, t.User, t.Admin);
    assert.equal(policy.canReference(links, v.alice, w.link(v.boss)), true);
    assert.equal(policy.canReference(links, v.alice, w.link(t.Admin)), true);
  });

  it('refuses a marker as a subject', () => {
    const w = new World();
    const m = w.markers;
    for (const [access, marker] of [
      [PermissionAccess.Read, m.allowRead],
      [PermissionAccess.Write, m.allowWrite],
      [PermissionAccess.Reference, m.allowReference],
    ]) {
      assert.throws(() => w.allow(access, marker, 1), { message: 'A marker cannot be a subject' });
    }
  });

  it('skips a mark of the null link and a mark of a missing rule', () => {
    const w = new World(true);
    const rules = w.rules();
    const marker = w.markers.allowRead;
    const before = w.policy.granted(w.links, marker);
    make(rules, marker, 0);
    const gone = make(rules, 1, 1);
    make(rules, marker, gone);
    rules.delete(gone);
    assert.deepEqual(w.policy.granted(w.links, marker), before);
    // A mark at the marker's own address of a store of rules is a mark.
    const apart = LinkStorage.inMemory();
    const pairAt = make(apart, 7, 8);
    const markAt = make(apart, 9, pairAt);
    apart.update(markAt, markAt, pairAt);
    const policy = new PermissionPolicy(w.markers, w.types, apart);
    assert.deepEqual(policy.granted(w.links, markAt), [[7, 8]]);
    // No rule of a marker allows nothing.
    assert.equal(policy.allows(w.links, w.markers.allowWrite, w.v.boss, w.link(1)), false);
  });

  it('owns what it created while untyped', () => {
    const w = new World();
    const { t, v } = w;
    let view = w.as(v.alice);
    const id = view.tryCreate(0, 0);
    assert.equal(view.exists(id), true);
    assert.equal(view.owns(view.inner().getLink(id)), true);
    const created = new Set(view.created());
    // Another subject does not own it, nor does a new view.
    assert.equal(w.as(v.alice).exists(id), false);
    view = w.as(v.alice).withCreated(created);
    view.update(id, t.Note, id);
    assert.equal(view.owns(view.inner().getLink(id)), false);
    // Undoing the update that typed it makes it its own again.
    view.inner().update(id, 0, 0);
    assert.equal(view.owns(view.inner().getLink(id)), true);
    // A self-reference has a type, itself, so the policy judges it.
    assert.equal(view.owns(link(id, id, id)), false);
    view.forget(id);
    assert.equal(view.exists(id), false);
    assert.equal(
      errorOf(() => w.as(0).tryCreate(0, 0)),
      'forbidden',
    );
  });

  it('refuses a delete that would take a hidden link with it', () => {
    const w = new World();
    const { t, v } = w;
    // The secret refers to alice's note: deleting the note deletes it.
    const boss = w.as(v.boss);
    const hiddenUse = boss.tryCreate(0, 0);
    boss.update(hiddenUse, t.Secret, v.note);
    w.links.setName(hiddenUse, 'use');
    const before = all(w.links);
    assert.equal(
      errorOf(() => w.as(v.alice).delete(v.note)),
      'forbidden',
    );
    assert.deepEqual(all(w.links), before);
    assert.equal(w.links.getName(v.note), 'note');
    assert.equal(w.links.getName(hiddenUse), 'use');
    // Boss may delete both.
    w.as(v.boss).delete(v.note);
    assert.equal(w.links.exists(hiddenUse), false);
    assert.equal(w.links.exists(v.note), false);
  });

  it('refuses a merge that would repoint a hidden link', () => {
    const w = new World();
    const { t, v } = w;
    const boss = w.as(v.boss);
    // Two equal notes but for their targets; the secret refers to one.
    const other = boss.tryCreate(0, 0);
    boss.update(other, t.Note, v.shared);
    const hiddenUse = boss.tryCreate(0, 0);
    boss.update(hiddenUse, t.Secret, other);
    const before = all(w.links);
    assert.deepEqual(w.link(v.note), link(v.note, t.Note, v.note));
    assert.equal(
      errorOf(() => w.as(v.alice).update(other, t.Note, v.note)),
      'forbidden',
    );
    assert.deepEqual(all(w.links), before);
    // Boss may.
    w.as(v.boss).update(other, t.Note, v.note);
    assert.equal(w.links.exists(other), false);
    assert.deepEqual(w.link(hiddenUse), link(hiddenUse, t.Secret, v.note));
  });

  it('reports every change of a write', () => {
    const w = new World();
    const view = w.as(w.v.alice);
    const id = view.tryCreate(0, 0);
    const changes = [];
    view.updateObserved(id, w.t.Note, id, (before, after) => changes.push([before, after]));
    assert.deepEqual(changes, [[link(id, 0, 0), link(id, w.t.Note, id)]]);
  });

  it('names need the right to write', () => {
    const w = new World();
    const { t, v } = w;
    let view = w.as(v.alice);
    // Names follow references: the name of a hidden link is an unknown name.
    assert.equal(view.getByName('secret'), null);
    assert.equal(view.getName(t.Note), 'Note');
    view.setName(v.note, 'my note');
    assert.equal(view.inner().getName(v.note), 'my note');
    assert.equal(
      errorOf(() => view.setName(v.secret, 'x')),
      'not-found',
    );
    assert.equal(
      errorOf(() => w.as(0).setName(v.note, 'y')),
      'not-found',
    );
    view = w.as(v.alice);
    // Taking the name of a link it may not write is taking a write.
    assert.equal(
      errorOf(() => view.setName(v.note, 'secret')),
      'forbidden',
    );
    assert.equal(view.inner().getByName('secret'), v.secret);
    view.removeName(v.secret);
    assert.equal(w.links.getName(v.secret), 'secret');
    w.as(v.alice).removeName(v.note);
    assert.equal(w.links.getName(v.note), null);
  });

  it('writes at the null link are not found and change nothing', () => {
    const w = new World();
    const { t, v } = w;
    const before = all(w.links);
    const view = w.as(v.alice);
    assert.equal(
      errorOf(() => view.update(0, t.Note, v.note)),
      'not-found',
    );
    assert.equal(
      errorOf(() => view.delete(0)),
      'not-found',
    );
    assert.deepEqual(all(w.links), before);
  });
});

describe('AccessPolicy integrations', () => {
  it('roots and markers may be unnamed links of any address', () => {
    const data = LinkStorage.inMemory();
    const rules = LinkStorage.inMemory();
    // A root of its own, not the point named Type: any link may be one.
    const root = make(data, 1, 1);
    const markers = new PermissionMarkers(
      make(data, 2, 2),
      make(data, 3, 3),
      make(data, 4, 4),
      make(data, 5, 5),
    );
    const types = new TypesDecorator(data, root, null);
    const user = types.defineRoot();
    const note = types.defineRoot();
    const alice = types.instance(user);
    const mine = types.instance(note);
    const typeMarkers = types.markers();
    const other = typed(data, root);
    PermissionPolicy.allow(rules, markers, PermissionAccess.Write, user, note);
    const policy = new PermissionPolicy(markers, typeMarkers, rules);
    const view = new PermissionsDecorator(data, policy, alice);
    assert.deepEqual(indexes(view), [mine]);
    assert.equal(
      errorOf(() => view.delete(other)),
      'not-found',
    );
    assert.equal(data.getByName('Type'), null);
    assert.equal(data.getByName('AllowWrite'), null);
  });

  it('a policy may bring a type system of its own', () => {
    // A link is of the type that is its source, and nothing extends anything.
    class SourceTyped extends AccessPolicy {
      constructor(policy) {
        super();
        this.policy = policy;
      }

      markers() {
        return this.policy.markers();
      }

      static isSubject(data, anything, subject, type) {
        return (
          type === anything ||
          (subject !== 0 && (subject === type || data.getLink(subject)?.source === type))
        );
      }

      allowsWith(data, marker, subject, value) {
        const { anything } = this.markers();
        return this.policy
          .granted(data, marker)
          .some(
            ([ruleSubject, object]) =>
              SourceTyped.isSubject(data, anything, subject, ruleSubject) &&
              (object === anything || value.source === object),
          );
      }

      canRead(data, subject, value) {
        const markers = this.markers();
        return (
          this.allowsWith(data, markers.allowRead, subject, value) ||
          this.allowsWith(data, markers.allowWrite, subject, value)
        );
      }

      canWrite(data, subject, value) {
        return this.allowsWith(data, this.markers().allowWrite, subject, value);
      }

      canCreate(data, subject) {
        const markers = this.markers();
        return this.policy
          .granted(data, markers.allowWrite)
          .some(([ruleSubject]) =>
            SourceTyped.isSubject(data, markers.anything, subject, ruleSubject),
          );
      }
    }

    const data = LinkStorage.inMemory();
    const markers = PermissionMarkers.ensure(data);
    const person = make(data, 0, 0);
    const post = make(data, 0, 0);
    data.update(person, person, person);
    data.update(post, post, post);
    const alice = make(data, person, 0);
    const mine = make(data, post, alice);
    const other = make(data, person, person);
    PermissionPolicy.allow(data, markers, PermissionAccess.Write, person, post);
    const noTypes = new TypeMarkers(0, null);
    const view = new PermissionsDecorator(
      data,
      new SourceTyped(new PermissionPolicy(markers, noTypes, null)),
      alice,
    );
    // The type of posts is a post too: it starts at itself.
    assert.deepEqual(indexes(view), [post, mine]);
    assert.equal(
      errorOf(() => view.delete(other)),
      'not-found',
    );
    // This policy says nothing of references: only what it reads may be referred to.
    assert.equal(
      errorOf(() => make(view, post, person)),
      'not-found',
    );
    const draft = make(view, post, mine);
    assert.equal(
      errorOf(() => view.update(draft, person, post)),
      'forbidden',
    );
  });

  it('a policy may keep no rules at all', () => {
    // Everyone reads everything, a link is written by the subject it starts at.
    class OwnerPolicy extends AccessPolicy {
      markers() {
        return new PermissionMarkers(0, 0, 0, 0);
      }

      canRead(_data, subject) {
        return subject !== 0;
      }

      canWrite(_data, subject, value) {
        return subject !== 0 && value.source === subject;
      }

      canCreate(_data, subject) {
        return subject !== 0;
      }
    }

    const data = LinkStorage.inMemory();
    const alice = make(data, 1, 1);
    const bob = make(data, 2, 2);
    const post = make(data, bob, alice);
    const asAlice = new PermissionsDecorator(data, new OwnerPolicy(), alice);
    assert.deepEqual(indexes(asAlice), [alice, bob, post]);
    assert.equal(
      errorOf(() => asAlice.delete(post)),
      'forbidden',
    );
    const mine = make(asAlice, alice, bob);
    assert.deepEqual(asAlice.inner().getLink(mine), link(mine, alice, bob));
    assert.equal(
      errorOf(() => asAlice.update(mine, bob, bob)),
      'forbidden',
    );
    const guest = new PermissionsDecorator(data, new OwnerPolicy(), 0);
    assert.equal(
      errorOf(() => make(guest, 1, 1)),
      'forbidden',
    );
    assert.deepEqual(all(guest), []);
  });
});
