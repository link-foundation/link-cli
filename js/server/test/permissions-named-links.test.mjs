// PermissionsDecorator at its edges: the cases of
// rust/tests/permissions_edges_tests.rs and the view-level cases of
// rust/tests/permissions_references_tests.rs, with every method of the
// decorator asked once. The rules themselves are in permissions.test.mjs.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { errorChain } from '../src/error-report.js';
import { link, NULL_LINK } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import { NamedTypeLinks } from '../src/named-type-links.js';
import { Forbidden } from '../src/permissions-named-links.js';
import {
  AccessPolicy,
  PermissionAccess,
  PermissionMarkers,
  PermissionsDecorator,
} from '../src/permissions.js';
import { all, errorOf, World } from './permissions-world.mjs';

const MARKERS = new PermissionMarkers(1, 2, 3, 4);
const MISSING = 9999;

/** Every right as plain data. */
class Plain extends AccessPolicy {
  constructor({ hidden = [], locked = [], referable = [], noCreate = false } = {}) {
    super();
    this.hidden = new Set(hidden);
    this.locked = new Set(locked);
    this.referable = new Set(referable);
    this.noCreate = noCreate;
  }

  markers() {
    return MARKERS;
  }

  canRead(_data, _subject, value) {
    return !this.hidden.has(value.index);
  }

  canWrite(_data, _subject, value) {
    return !this.locked.has(value.index);
  }

  canCreate(_data, _subject) {
    return !this.noCreate;
  }

  canReference(_data, _subject, value) {
    return this.referable.has(value.index);
  }
}

/** Steps a write of `Scripted` takes besides the one it was asked for. */
const Also = {
  create: (source, target) => ({ kind: 'create', source, target }),
  update: (at, source, target) => ({ kind: 'update', at, source, target }),
  delete: (at) => ({ kind: 'delete', at }),
  nothing: { kind: 'nothing' },
  fail: { kind: 'fail' },
};

/** A store whose update also runs a script, and whose deletes can fail. */
class Scripted extends NamedTypeLinks {
  constructor(count) {
    super();
    this.links = LinkStorage.inMemory();
    for (let i = 0; i < count; i++) this.links.create(0, 0);
    this.also = [];
    this.deletesFail = false;
  }

  updateObserved(id, source, target, observer) {
    const before = this.links.updateObserved(id, source, target, observer);
    // The script runs once: the undo of this write comes through here too.
    const steps = this.also;
    this.also = [];
    for (const step of steps) {
      if (step.kind === 'create') {
        const made = this.links.create(step.source, step.target);
        observer(NULL_LINK, link(made, step.source, step.target));
      } else if (step.kind === 'update') {
        this.links.updateObserved(step.at, step.source, step.target, observer);
      } else if (step.kind === 'delete') {
        this.links.deleteObserved(step.at, observer);
      } else if (step.kind === 'nothing') {
        observer(NULL_LINK, NULL_LINK);
      } else {
        throw new Error('the store refused');
      }
    }
    return before;
  }

  deleteObserved(id, observer) {
    if (this.deletesFail) throw new Error('deletes fail');
    return this.links.deleteObserved(id, observer);
  }
}

for (const name of [
  'create',
  'ensureCreated',
  'getLink',
  'exists',
  'allLinks',
  'queryLinks',
  'search',
  'getOrCreate',
  'getName',
  'setName',
  'getByName',
  'removeName',
  'save',
]) {
  Scripted.prototype[name] = function forward(...args) {
    return this.links[name](...args);
  };
}

const view = (store, policy = new Plain()) => new PermissionsDecorator(store, policy, 0);

const scripted = (also, policy = new Plain()) => {
  const store = new Scripted(4);
  store.also = also;
  return view(store, policy);
};

const isForbidden = (action) => errorOf(action) === 'forbidden';
const isNotFound = (action) => errorOf(action) === 'not-found';

describe('PermissionsDecorator edges', () => {
  it('asks a policy the same questions whatever it is', () => {
    const policy = new Plain({ hidden: [1], locked: [2], referable: [1], noCreate: true });
    const data = LinkStorage.inMemory();
    assert.equal(policy.markers().allowRead, MARKERS.allowRead);
    assert.equal(policy.canRead(data, 0, link(1, 0, 0)), false);
    assert.equal(policy.canWrite(data, 0, link(2, 0, 0)), false);
    assert.equal(policy.canCreate(data, 0), false);
    assert.equal(policy.canReference(data, 0, link(1, 0, 0)), true);
    assert.equal(new AccessPolicy().canReference(data, 0, link(1, 0, 0)), false);
  });

  it('hands out its parts', () => {
    const store = new Scripted(1);
    const links = view(store);
    links.policy().noCreate = true;
    assert.ok(isForbidden(() => links.tryCreate(0, 0)));
    assert.equal(links.inner(), store);
    const [inner, policy] = links.partsMut();
    assert.equal(inner, store);
    assert.equal(policy, links.policy());
  });

  it('keeps the rules to their own store and skips marks of nothing', () => {
    const apart = new World(true);
    assert.notEqual(apart.policy.rulesMut(), null);
    const together = new World(false);
    assert.equal(together.policy.rulesMut(), null);

    const marker = together.markers.of(PermissionAccess.Read);
    const granted = together.policy.granted(together.links, marker);
    together.links.create(marker, 0);
    together.links.create(marker, 9_999);
    assert.deepEqual(together.policy.granted(together.links, marker), granted);
  });

  it('answers a write to nothing as the store does', () => {
    const links = view(new Scripted(1));
    assert.throws(() => links.update(0, 0, 0));
    assert.ok(isNotFound(() => links.update(99, 0, 0)));
    assert.throws(() => links.delete(0));
    assert.ok(isNotFound(() => links.delete(99)));
  });

  it('finds no hidden link and forbids a locked one', () => {
    const policy = new Plain({ hidden: [1], locked: [2, 3] });
    const links = view(new Scripted(4), policy);
    assert.ok(isNotFound(() => links.update(1, 0, 0)));
    assert.ok(isNotFound(() => links.delete(1)));
    assert.ok(isForbidden(() => links.update(2, 0, 0)));
    assert.ok(isForbidden(() => links.delete(2)));
    assert.throws(() => links.update(2, 0, 0), { message: 'Forbidden: update 2' });
    assert.throws(() => links.delete(2), { message: 'Forbidden: delete 2' });
    // A link it may write cannot be made into a link it may not write.
    links.policy().locked = new Set([3]);
    assert.ok(isForbidden(() => links.update(3, 4, 4)));
    assert.deepEqual(links.update(2, 4, 4), link(2, 0, 0));
  });

  it('needs every right and a free address for a point at an address', () => {
    const links = view(new Scripted(2));
    assert.equal(links.tryEnsureCreated(2), 2, 'it is there');
    assert.throws(() => links.tryEnsureCreated(0), /unsupported link address 0/);
    assert.equal(links.created().has(0), false);

    links.policy().hidden = new Set([1, 7]);
    assert.ok(
      isNotFound(() => links.tryEnsureCreated(7)),
      'hidden and not referable',
    );
    links.policy().referable = new Set([1, 7]);
    assert.throws(() => links.tryEnsureCreated(1), { message: 'Link 1 exists' });
    assert.equal(links.ensureCreated(7), 7);
    assert.ok(links.created().has(7));

    links.policy().noCreate = true;
    assert.ok(isForbidden(() => links.tryEnsureCreated(9)));
  });

  it('refuses through the calls that answer no error in Rust', () => {
    const links = view(new Scripted(1), new Plain({ noCreate: true }));
    for (const action of [
      () => links.create(0, 0),
      () => links.ensureCreated(5),
      () => links.getOrCreate(9, 9),
    ]) {
      assert.throws(action, /Forbidden/);
    }
  });

  it('takes the answers about addresses and saving from the store', () => {
    const links = view(new Scripted(1));
    assert.equal(links.claims(1), null);
    assert.deepEqual(links.nextCreatedAddresses(1), [2]);
    assert.equal(links.referenceExists(1), true);
    links.save();
  });

  it('lets names follow the rights of their links', () => {
    const store = new Scripted(4);
    store.setName(1, 'hidden');
    store.setName(2, 'locked');
    store.setName(3, 'free');
    const links = view(store, new Plain({ hidden: [1], locked: [2] }));

    assert.equal(links.getName(1), null);
    assert.equal(links.getName(2), 'locked');
    assert.equal(links.getByName('hidden'), null);
    assert.equal(links.getByName('locked'), 2);
    assert.equal(links.getByName('absent'), null);

    assert.ok(isNotFound(() => links.setName(1, 'x')));
    assert.ok(isNotFound(() => links.setName(99, 'x')));
    assert.throws(() => links.setName(2, 'x'), { message: 'Forbidden: set-name 2' });
    assert.throws(
      () => links.setName(4, 'locked'),
      { message: 'Forbidden: set-name 4' },
      'it takes a name from a locked link',
    );
    assert.equal(links.setName(4, 'hidden'), 4, "the rules below decide a hidden holder's name");
    assert.equal(links.setName(3, 'free'), 3, "a link's own name");

    links.removeName(1);
    assert.equal(links.inner().getName(1), null, 'the name moved to link 4');
    assert.throws(() => links.removeName(2), { message: 'Forbidden: remove-name 2' });
    links.removeName(3);
    assert.equal(links.getName(3), null);
    links.removeName(99);
  });

  it('needs the right to create a link a write creates', () => {
    let links = scripted([Also.create(0, 0)]);
    links.update(1, 1, 1);
    assert.equal(links.inner().exists(5), true);

    links = scripted([Also.create(0, 0)], new Plain({ noCreate: true }));
    assert.throws(() => links.update(1, 1, 1), { message: 'Forbidden: update 1' });
    assert.equal(links.inner().exists(5), false, 'undone');
    assert.deepEqual(links.inner().getLink(1), link(1, 0, 0));

    links = scripted([Also.create(1, 1)], new Plain({ locked: [5] }));
    assert.ok(
      isForbidden(() => links.update(1, 1, 1)),
      'a new link it may not write',
    );
    assert.equal(links.inner().exists(5), false);
  });

  it('needs the right to write a link a write changes', () => {
    const locked = () => new Plain({ locked: [2] });
    let links = scripted([Also.update(2, 2, 2)], locked());
    assert.ok(isForbidden(() => links.update(1, 1, 1)));
    assert.deepEqual(links.inner().getLink(2), link(2, 0, 0));

    links = scripted([Also.delete(2)], locked());
    assert.ok(isForbidden(() => links.update(1, 1, 1)));
    assert.equal(links.inner().exists(2), true, 'undone');

    // Unchanged in the end, or still its own without a type.
    links = scripted([Also.update(2, 3, 3), Also.update(2, 0, 0), Also.nothing], locked());
    links.update(1, 1, 1);
    links = scripted([Also.update(2, 0, 0)], locked()).withCreated(new Set([2]));
    links.update(1, 1, 1);
    links = scripted([Also.update(2, 0, 3)], locked()).withCreated(new Set([2]));
    links.update(1, 1, 1);
    assert.deepEqual(links.inner().getLink(2), link(2, 0, 3));

    // A link it may write may change with the write.
    links = scripted([Also.update(2, 3, 3)]);
    links.update(1, 1, 1);
    assert.deepEqual(links.inner().getLink(2), link(2, 3, 3));

    // A link that went away stops being the subject's own.
    links = scripted([Also.delete(3)]).withCreated(new Set([3]));
    const seen = [];
    links.updateObserved(1, 1, 1, (before, after) => seen.push([before, after]));
    assert.equal(links.created().has(3), false);
    assert.deepEqual(seen[0], [link(1, 0, 0), link(1, 1, 1)]);
  });

  it('reports every change of a write, the null link for none', () => {
    const links = scripted([Also.nothing]);
    const seen = [];
    links.updateObserved(1, 1, 1, (before, after) => seen.push([before, after]));
    assert.deepEqual(seen, [
      [link(1, 0, 0), link(1, 1, 1)],
      [NULL_LINK, NULL_LINK],
    ]);
    const deleted = [];
    assert.deepEqual(
      links.deleteObserved(2, (before, after) => deleted.push([before, after])),
      link(2, 0, 0),
    );
    assert.deepEqual(deleted.at(-1), [link(2, 0, 0), NULL_LINK]);
  });

  it('undoes a failed write and tells of a failed undo', () => {
    const steps = () => [Also.create(0, 0), Also.fail];
    let links = scripted(steps());
    assert.throws(() => links.update(1, 1, 1), { message: 'the store refused' });
    assert.equal(links.inner().exists(5), false, 'undone');
    assert.deepEqual(links.inner().getLink(1), link(1, 0, 0));

    links = scripted(steps());
    links.inner().deletesFail = true;
    let told = null;
    try {
      links.update(1, 1, 1);
    } catch (error) {
      told = error;
    }
    assert.deepEqual(
      errorChain(told).map((error) => error.message),
      ['and undoing it failed: deletes fail', 'the store refused'],
    );

    links = scripted([Also.delete(2)]);
    links.policy().locked = new Set([2]);
    links.inner().deletesFail = true;
    // Deleting is the store's step, so a store that refuses it refuses the write.
    assert.throws(() => links.update(1, 1, 1));
  });
});

describe('PermissionsDecorator parts', () => {
  it('keeps a subject and the links it made', () => {
    const links = view(new Scripted(1));
    assert.equal(links.subject(), 0);
    links.setSubject(7);
    assert.equal(links.subject(), 7);
    const first = links.created();
    assert.equal(links.withCreated(new Set([1])), links);
    assert.deepEqual([...links.replaceCreated(first)], [1]);
    assert.equal(links.created(), first);
    links.remember(9);
    assert.ok(links.created().has(9));
    links.forget(9);
    assert.equal(links.created().has(9), false);
  });

  it('reads triplets: a point it made is its own', () => {
    const links = view(new Scripted(1), new Plain({ hidden: [5, 6], locked: [5, 6] }));
    links.remember(5);
    assert.equal(links.owns(link(5, 0, 0)), true);
    assert.equal(links.owns(link(5, 5, 5)), false);
    assert.equal(links.ownsTriplet(link(5, 5, 5)), true);
    assert.equal(links.ownsTriplet(link(5, 1, 5)), false);
    assert.equal(links.ownsTriplet(link(6, 6, 6)), false);
    assert.equal(links.canReadTriplet(link(5, 5, 5)), true);
    assert.equal(links.canReadTriplet(link(6, 6, 6)), false);
    assert.equal(links.canReadTriplet(link(7, 7, 7)), true);
    assert.equal(links.canWriteTriplet(link(5, 5, 5)), true);
    assert.equal(links.canWriteTriplet(link(6, 6, 6)), false);
    assert.equal(links.canWriteTriplet(link(7, 7, 7)), true);
    assert.equal(links.policyCanWrite(link(5, 0, 0)), false, 'ownership aside');
    assert.equal(links.policyCanWrite(link(7, 0, 0)), true);
    assert.equal(links.policyCanCreate(), true);
  });

  it('forwards the query limits and keeps only new addresses of a preview', () => {
    const store = new Scripted(1);
    const links = view(store);
    assert.equal(links.queryLimits(), null);
    assert.equal(links.reserveQueryAllocations(null, null, null), null);
    const asked = [];
    const reserving = Object.create(store, {
      reserveQueryAllocations: {
        value: (...args) => {
          asked.push(args);
          return new Set([1, 50]);
        },
      },
      queryLimits: { value: () => ({ limit: 3 }) },
      claims: { value: (id) => id === 50 },
    });
    const over = view(reserving);
    assert.deepEqual([...over.reserveQueryAllocations('p', 'r', 's')], [50]);
    assert.deepEqual(asked, [['p', 'r', 's']]);
    assert.deepEqual(over.queryLimits(), { limit: 3 });
    assert.equal(over.claims(50), true);
  });

  it('reads only what it may read', () => {
    const links = view(new Scripted(3), new Plain({ hidden: [1] }));
    assert.deepEqual(
      links.allLinks().map((value) => value.index),
      [2, 3],
    );
    assert.deepEqual(
      links.queryLinks().map((value) => value.index),
      [2, 3],
    );
    assert.deepEqual(links.queryLinks(1), []);
    assert.deepEqual(links.queryLinks(2, 0, 0), [link(2, 0, 0)]);
    assert.equal(links.getLink(1), null);
    assert.equal(links.getLink(99), null);
    assert.equal(links.exists(2), true);
    assert.equal(links.search(0, 0), null, 'the lowest match is hidden');
    assert.equal(links.search(4, 4), null);
    links.policy().hidden = new Set();
    assert.equal(links.search(0, 0), 1);
  });

  it('gets or creates, and creates and ensures through the checked calls', () => {
    const links = view(new Scripted(1));
    assert.equal(links.getOrCreate(0, 0), 1);
    const made = links.getOrCreate(1, 1);
    assert.equal(made, 2);
    assert.equal(links.created().has(made), true);
    assert.equal(links.create(0, 0), 3);
    assert.equal(links.ensureCreated(10), 10);
    assert.equal(links.ensureCreated(10), 10, 'it is there');
    assert.deepEqual([...links.created()].sort(), [10, 2, 3]);
    // Creating a link from a type it may not write is forbidden.
    links.policy().locked = new Set([0]);
    assert.throws(() => links.create(1, 1), { message: 'Forbidden: create' });
    const typeless = links.create(0, 1);
    assert.deepEqual(links.getLink(typeless), link(typeless, 0, 1));
  });

  it('lets its own untyped link take a type and lose it', () => {
    const links = view(new Scripted(1), new Plain({ locked: [2] }));
    const own = links.tryCreate(0, 0);
    assert.equal(own, 2);
    assert.deepEqual(links.update(own, 0, 1), link(own, 0, 0), 'still untyped');
    assert.ok(
      isForbidden(() => links.update(own, 1, 1)),
      'a type makes it a locked link',
    );
    assert.deepEqual(links.delete(own), link(own, 0, 1));
    assert.equal(links.inner().exists(own), false);
  });

  it('saves through the store', () => {
    const store = new Scripted(0);
    let saved = 0;
    store.save = () => {
      saved += 1;
    };
    view(store).save();
    assert.equal(saved, 1);
  });

  it('is a Forbidden error with what it refused', () => {
    const error = new Forbidden('update 7');
    assert.ok(error instanceof Error);
    assert.equal(error.name, 'Forbidden');
    assert.equal(error.what, 'update 7');
    assert.equal(error.message, 'Forbidden: update 7');
  });
});

/** A note of alice's own, as the requests that follow may update it. */
function ownNote(w) {
  const view = w.as(w.v.alice);
  const id = view.tryCreate(0, 0);
  view.update(id, w.t.Note, 0);
  return id;
}

/** Everyone reads what starts at a type it names, and says nothing of references. */
class ReadsPublicOnly extends AccessPolicy {
  constructor(type, markers) {
    super();
    this.type = type;
    this.markersOf = markers;
  }

  markers() {
    return this.markersOf;
  }

  canRead(_data, _subject, value) {
    return value.source === this.type;
  }

  canWrite() {
    return true;
  }

  canCreate() {
    return true;
  }
}

describe('PermissionsDecorator references', () => {
  it('answers alike for an unreadable and a missing address', () => {
    const w = new World(false);
    const own = ownNote(w);
    const { t, v } = w;
    const before = all(w.links);
    const view = w.as(v.alice);
    for (const address of [v.secret, MISSING]) {
      const notFound = { message: `Link not found: ${address}` };
      assert.throws(() => view.tryCreate(t.Note, address), notFound);
      assert.throws(() => view.tryCreate(0, address), notFound);
      assert.throws(() => view.update(own, t.Note, address), notFound);
    }
    assert.deepEqual(all(w.links), before);
  });

  it('forbids a type it may not write for hidden and missing alike', () => {
    const w = new World(false);
    const own = ownNote(w);
    for (const type of [w.t.Secret, MISSING]) {
      assert.throws(() => w.as(w.v.alice).update(own, type, own), {
        message: `Forbidden: update ${own}`,
      });
    }
  });

  it('takes the name of an unreadable link as an unknown name', () => {
    const w = new World(false);
    assert.equal(w.as(w.v.alice).getByName('secret'), null);
    assert.equal(w.as(w.v.alice).getName(w.v.secret), null);
  });

  it('cannot take the name of an unreadable link', () => {
    const w = new World(false);
    const { note, secret, alice } = w.v;
    assert.throws(() => w.as(alice).setName(note, 'secret'), {
      message: `Forbidden: set-name ${note}`,
    });
    assert.equal(w.links.getByName('secret'), secret);
    assert.equal(w.links.getName(note), 'note');
  });

  it('refuses it in the view alone', () => {
    const w = new World(false);
    const { t, v } = w;
    const view = w.as(v.alice);
    const id = view.tryCreate(0, 0);
    for (const address of [v.secret, MISSING]) {
      assert.equal(
        errorOf(() => view.update(id, t.Note, address)),
        'not-found',
      );
      assert.equal(
        errorOf(() => view.update(id, 0, address)),
        'not-found',
      );
      assert.equal(
        errorOf(() => view.tryCreate(t.Note, address)),
        'not-found',
      );
      assert.equal(
        errorOf(() => view.tryEnsureCreated(address)),
        'not-found',
      );
      assert.equal(view.referenceExists(address), false);
    }
    assert.deepEqual(w.links.getLink(id), link(id, 0, 0));
  });

  it('covers the object of a rule and the links of its type', () => {
    const w = new World(false);
    const { t, v } = w;
    w.allow(PermissionAccess.Reference, t.User, t.Secret);
    const view = w.as(v.alice);
    view.tryCreate(t.Note, v.secret);
    view.tryCreate(t.Note, t.Secret);
    // Not its sibling, and a missing address stays missing.
    assert.equal(
      errorOf(() => view.tryCreate(t.Note, t.Admin)),
      'not-found',
    );
    assert.equal(
      errorOf(() => view.tryCreate(t.Note, MISSING)),
      'not-found',
    );
    // With the right the query answers that the address is taken.
    assert.equal(view.referenceExists(v.secret), true);
    assert.equal(view.referenceExists(MISSING), false);
    assert.equal(view.getByName('secret'), v.secret);
    assert.throws(() => view.tryEnsureCreated(v.secret), { message: `Link ${v.secret} exists` });
  });

  it('does not let the right read', () => {
    const w = new World(false);
    const { t, v } = w;
    w.allow(PermissionAccess.Reference, t.User, t.Secret);
    const view = w.as(v.alice);
    assert.equal(view.canReference(v.secret), true);
    assert.equal(view.canRead(link(v.secret, t.Secret, 0)), false);
    assert.equal(view.getLink(v.secret), null);
    assert.equal(
      view.allLinks().some((value) => value.index === v.secret),
      false,
    );
    assert.equal(
      errorOf(() => view.delete(v.secret)),
      'not-found',
    );
    assert.equal(view.getName(v.secret), 'secret');
  });

  it('applies a rule to its subjects only', () => {
    const w = new World(false);
    const { t, v } = w;
    w.allow(PermissionAccess.Reference, t.Admin, t.Secret);
    // boss is an Admin, alice only a User, and a guest nobody.
    assert.equal(w.as(v.boss).canReference(v.secret), true);
    assert.equal(w.as(v.alice).canReference(v.secret), false);
    assert.equal(w.as(0).canReference(v.secret), false);
  });

  it('lets a guest choose no address', () => {
    const w = new World(false);
    for (const address of [w.v.secret, MISSING]) {
      assert.equal(
        errorOf(() => w.as(0).tryEnsureCreated(address)),
        'forbidden',
      );
    }
  });

  it('needs no more rules for the object of a rule of the subject', () => {
    const w = new World(false);
    const { t, v } = w;
    const view = w.as(v.alice);
    // User writes Note: enough to type a link as Note, yet Note stays unread.
    assert.equal(view.getLink(t.Note), null);
    view.tryCreate(t.Note, 0);
    // User reads Public, readable links, and the subject itself.
    view.tryCreate(t.Note, t.Public);
    view.tryCreate(t.Note, v.shared);
    view.tryCreate(t.Note, v.alice);
    // Who it says nothing of: the guest.
    assert.equal(w.as(0).canReference(t.Note), false);
  });

  it('keeps a reference it holds but cannot put it back', () => {
    const w = new World(false);
    const { t, v } = w;
    const held = w.as(v.boss).tryCreate(t.Note, v.secret);
    const view = w.as(v.alice);
    // It stays, while the update changes what the subject may change.
    view.update(held, t.Shared, v.secret);
    view.update(held, t.Note, v.secret);
    // Once replaced, it cannot be put back.
    view.update(held, t.Note, 0);
    assert.equal(
      errorOf(() => view.update(held, t.Note, v.secret)),
      'not-found',
    );
  });

  it('restores the previous behaviour with anything to anything, as a choice', () => {
    const w = new World(false);
    const { t, v, markers } = w;
    w.allow(PermissionAccess.Reference, markers.anything, markers.anything);
    const view = w.as(v.alice);
    view.tryCreate(t.Note, v.secret);
    assert.equal(view.getByName('secret'), v.secret);
    // The address of a hidden link is taken, a free one is not.
    assert.throws(() => view.tryEnsureCreated(v.secret), { message: `Link ${v.secret} exists` });
    assert.equal(view.referenceExists(v.secret), true);
    assert.equal(view.referenceExists(MISSING), false);
    assert.equal(view.tryEnsureCreated(MISSING), MISSING);
    assert.equal(view.exists(MISSING), true);
    // It reads no more than it did.
    assert.equal(view.getLink(v.secret), null);
  });

  it('does not affect an administrator', () => {
    const w = new World(false);
    const { t, v } = w;
    const view = w.as(v.boss);
    const made = view.tryCreate(t.Note, v.secret);
    assert.deepEqual(w.link(made), link(made, t.Note, v.secret));
    assert.equal(view.tryEnsureCreated(12345), 12345);
  });

  it('allows references to readable links only under a policy without the right', () => {
    const w = new World(false);
    const { t, v } = w;
    const readable = w.links.tryCreate(t.Public, 0);
    const view = new PermissionsDecorator(
      w.links,
      new ReadsPublicOnly(t.Public, w.markers),
      v.alice,
    );
    view.tryCreate(0, readable);
    assert.equal(
      errorOf(() => view.tryCreate(0, v.secret)),
      'not-found',
    );
  });
});
