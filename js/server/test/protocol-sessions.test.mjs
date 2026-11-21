// Sessions of a links server (src/protocol-sessions.js): ports
// rust/tests/links_sessions_tests.rs, rust/tests/sessions_faults_tests.rs,
// rust/tests/checked_sessions_tests.rs and the permission cases of
// rust/tests/triplets_protocol_tests.rs, without the network.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { link, NULL_LINK } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import {
  AccessPolicy,
  LinksTokenTable,
  PermissionMarkers,
  PermissionPolicy,
  PermissionsDecorator,
} from '../src/permissions.js';
import { errorMessage, parseDocument, queryDocument } from '../src/protocol-lino.js';
import { LinksSessions } from '../src/protocol-sessions.js';
import { QueryLimits } from '../src/query-options.js';
import { UndoableLinksDecorator } from '../src/undoable-links.js';
import { all, World } from './permissions-world.mjs';

/** The address the next link gets. */
const next = (store) => store.allLinks().length + 1;

/** How many links `subject` sees. */
const seenBy = (w, subject) => w.as(subject).allLinks().length;

/** The names of `links`, by index. */
const namesOf = (store, links) =>
  new Map(links.map((value) => [value.index, store.getName(value.index)]));

const AUTH_FAILED = "(error: 'Forbidden: authentication failed')";

test('authenticates with the callback', () => {
  const sessions = new LinksSessions(LinkStorage.inMemory()).withAuthenticator((_, token) =>
    token === 'secret' ? 7 : null,
  );
  const writer = sessions.open();
  assert.equal(sessions.request(writer, '(auth: wrong)'), AUTH_FAILED);
  assert.equal(sessions.subject(writer), 0);
  assert.equal(sessions.request(writer, '(auth: secret)'), '(subject: 7)');
  assert.equal(sessions.subject(writer), 7);
  assert.equal(sessions.request(writer, '() ((1 1))'), '() ((1: 1 1))');
  assert.deepEqual(sessions.sessions(), [writer]);
  sessions.close(writer);
  assert.deepEqual(sessions.sessions(), []);
  assert.equal(sessions.subject(writer), null);
  assert.equal(sessions.created(writer), null);
});

test('refuses authentication without an authenticator, or one that fails', () => {
  const sessions = new LinksSessions(LinkStorage.inMemory());
  const session = sessions.open();
  assert.equal(sessions.request(session, '(auth: secret)'), AUTH_FAILED);
  // A token is one reference; an authenticator that throws, or finds nobody, fails it.
  sessions.withAuthenticator((_, token) => {
    if (token === 'boom') throw new Error('broken table');
    return token === 'nobody' ? undefined : 3;
  });
  for (const text of ['(auth: boom)', '(auth: nobody)', '(auth: a b)', '(auth: (a b))']) {
    assert.equal(sessions.request(session, text), AUTH_FAILED, text);
  }
  assert.equal(sessions.request(session, '(auth: someone)'), '(subject: 3)');
});

test('serves each subject what its permissions allow', () => {
  const w = new World(false);
  const { t, v } = w;
  const seen = seenBy(w, v.alice);
  const sessions = LinksSessions.withPermissions(w.links, w.policy);
  assert.ok(sessions.hasPermissions());
  assert.equal(sessions.store(), w.links);
  assert.equal(sessions.policy(), w.policy);
  const alice = sessions.open(v.alice);
  const boss = sessions.open(v.boss);
  const { secret } = v;
  const note = t.Note;
  assert.equal(sessions.request(alice, '(count: ())'), `(count: ${seen})`);
  assert.equal(
    sessions.request(alice, `(update: (${secret} ${note} ${secret}))`),
    `(error: 'Link not found: ${secret}')`,
  );
  // set-name answers where the names store keeps the name.
  assert.ok(sessions.request(alice, `(set-name: (${v.note} 'mine'))`).startsWith('(link: '));
  assert.equal(sessions.store().getName(v.note), 'mine');
  assert.equal(sessions.request(alice, "(get-by-name: 'secret')"), '');
  assert.equal(sessions.request(alice, "(get-by-name: 'nothing')"), '');
  assert.equal(sessions.request(alice, `(get-name: ${secret})`), '');
  assert.match(
    sessions.request(alice, '((secret: * *)) ()'),
    /Invalid reference to non-existent link 'secret'/,
  );
  assert.equal(sessions.request(boss, `(get-name: ${secret})`), '(name: secret)');
  assert.equal(sessions.request(boss, "(get-by-name: 'secret')"), `(link: ${secret})`);
  assert.equal(
    sessions.request(boss, `(update: (${secret} ${note} ${secret}))`),
    `((${secret}: ${t.Secret} ${secret})) ((${secret}: ${note} ${secret}))`,
  );
  assert.equal(sessions.request(alice, '(count: ())'), `(count: ${seen + 1})`);
  const guest = sessions.open();
  assert.equal(sessions.request(guest, '(create: (0 0))'), "(error: 'Forbidden: create')");
});

test('lets a subject type the links it creates as types it may not read', () => {
  const w = new World(false);
  const { t, v } = w;
  const n = next(w.links);
  const sessions = LinksSessions.withPermissions(w.links, w.policy);
  const alice = sessions.open(v.alice);
  const note = t.Note;
  // alice may write notes, but may not read the link of the type Note.
  assert.equal(sessions.request(alice, `(each: (${note}))`), '');
  assert.equal(sessions.request(alice, '(create: (0 0))'), `() ((${n}: 0 0))`);
  assert.deepEqual([...sessions.created(alice)], [n]);
  assert.equal(
    sessions.request(alice, `(update: (${n} ${note} ${n}))`),
    `((${n}: 0 0)) ((${n}: ${note} ${n}))`,
  );
  // (n: Note n) is there already, so the next note points to another link.
  const other = v.alice;
  assert.equal(
    sessions.request(alice, `(create: (${note} ${other}))`),
    `() ((${n + 1}: ${note} ${other}))`,
  );
  // The names of types alice cannot read resolve all the same.
  assert.equal(sessions.request(alice, `() ((Note ${n + 1}))`), `() ((${n + 2}: Note ${n + 1}))`);
});

test('undoes a failed request and reports its own error', () => {
  const w = new World(false);
  const { t, v } = w;
  const n = next(w.links);
  const sessions = LinksSessions.withPermissions(w.links, w.policy);
  const alice = sessions.open(v.alice);
  const note = t.Note;
  sessions.request(alice, '(create: (0 0))');
  // Allowed before, refused as it would be after: undoing it needs no permission.
  assert.equal(
    sessions.request(alice, `(update: (${n} ${t.Secret} ${n}))`),
    `(error: 'Forbidden: update ${n}')`,
  );
  // The link is still hers.
  assert.equal(sessions.request(alice, `(each: (${n}))`), `(${n}: 0 0)`);
  assert.equal(
    sessions.request(alice, `(update: (${n} ${note} ${n}))`),
    `((${n}: 0 0)) ((${n}: ${note} ${n}))`,
  );
});

test('undoes every part of a failed substitution, including hidden ones', () => {
  const w = new World(false);
  const { t, v } = w;
  const sessions = LinksSessions.withPermissions(w.links, w.policy);
  const store = sessions.store();
  const alice = sessions.open(v.alice);
  // Public is a type she may refer to, as she reads it, yet not write.
  const note = t.Note;
  const publicType = t.Public;
  let before = all(store);
  const names = namesOf(store, before);
  const changed = [];
  const reply = sessions.executeObserved(
    alice,
    parseDocument(`() ((${note} ${note}) (${publicType} ${publicType}))`),
    changed,
  );
  assert.match(errorMessage(reply), /^Forbidden: /);
  assert.deepEqual(changed, []);
  assert.deepEqual(all(store), before);
  assert.deepEqual(namesOf(store, before), names);
  // Deleting a note deletes the links that use it and its name; the request
  // fails at its forbidden update after the delete, and all of it is undone.
  const k = next(store);
  const { shared } = v;
  const noteLink = v.note;
  assert.equal(
    sessions.request(alice, `(create: (${t.Shared} ${noteLink}))`),
    `() ((${k}: ${t.Shared} ${noteLink}))`,
  );
  before = all(store);
  const request = `((${noteLink}: * *) (${shared}: * *)) ((${shared}: ${publicType} ${shared}))`;
  assert.equal(sessions.request(alice, request), `(error: 'Forbidden: update ${shared}')`);
  assert.deepEqual(all(store), before);
  assert.equal(store.getName(noteLink), 'note');
  assert.equal(store.getName(shared), 'shared');
  // Allowed, the same request deletes both and the name.
  const boss = sessions.open(v.boss);
  sessions.request(boss, request);
  assert.ok(!store.exists(noteLink));
  assert.ok(!store.exists(k));
  assert.equal(store.getName(noteLink), null);
});

test('does not give a subject a link made where its own was deleted', () => {
  const w = new World(false);
  const { v } = w;
  const n = next(w.links);
  const sessions = LinksSessions.withPermissions(w.links, w.policy);
  const alice = sessions.open(v.alice);
  const boss = sessions.open(v.boss);
  sessions.request(alice, '(create: (0 0))');
  assert.equal(sessions.request(alice, `(each: (${n}))`), `(${n}: 0 0)`);
  assert.ok(sessions.canRead(alice, link(n, 0, 0)));
  const changed = [];
  sessions.executeObserved(boss, parseDocument(`(delete: ${n})`), changed);
  assert.deepEqual(changed, [[link(n, 0, 0), NULL_LINK]]);
  assert.deepEqual([...sessions.created(alice)], []);
  assert.equal(sessions.request(boss, '(create: (0 0))'), `() ((${n}: 0 0))`);
  assert.equal(sessions.request(alice, `(each: (${n}))`), '');
  assert.ok(!sessions.canRead(alice, link(n, 0, 0)));
  assert.equal(sessions.request(alice, `(update: (${n} 0 0))`), `(error: 'Link not found: ${n}')`);
});

test('serves rules in a store of their own alike', () => {
  const w = new World(true);
  const { v } = w;
  const expected = seenBy(w, v.alice);
  const sessions = LinksSessions.withPermissions(w.links, w.policy);
  const alice = sessions.open(v.alice);
  assert.equal(sessions.request(alice, '(count: ())'), `(count: ${expected})`);
  assert.match(
    sessions.request(alice, '((secret: * *)) ()'),
    /Invalid reference to non-existent link 'secret'/,
  );
  assert.ok(sessions.request(alice, `(set-name: (${v.note} 'mine'))`).startsWith('(link: '));
  assert.equal(sessions.store().getName(v.note), 'mine');
});

test('without permissions every session sees the whole store', () => {
  const w = new World(false);
  const { v } = w;
  const count = w.links.allLinks().length;
  const sessions = new LinksSessions(w.links);
  assert.ok(!sessions.hasPermissions());
  assert.equal(sessions.policy(), null);
  assert.equal(sessions.store(), w.links);
  const alice = sessions.open(v.alice);
  assert.equal(sessions.request(alice, '(count: ())'), `(count: ${count})`);
  assert.equal(sessions.request(alice, "(get-by-name: 'secret')"), `(link: ${v.secret})`);
  assert.ok(sessions.canRead(alice, w.link(v.secret)));
});

test('authenticates with a token table in links', () => {
  const w = new World(false);
  const { v } = w;
  const seen = seenBy(w, v.alice);
  const table = new LinksTokenTable(w.links);
  assert.equal(table.authenticate('alice-token'), null);
  const tokenLink = table.grant('alice-token', v.alice);
  table.grant('alice-second-token', v.alice);
  table.grant('boss-token', v.boss);
  assert.equal(table.authenticate('alice-token'), v.alice);
  assert.equal(table.authenticate('alice-second-token'), v.alice);
  assert.equal(table.authenticate('boss-token'), v.boss);
  assert.equal(table.authenticate('wrong'), null);
  // The store holds a hash of the token, never the token itself.
  const stored = w.links.getName(tokenLink);
  assert.equal(stored, LinksTokenTable.nameOf('alice-token'));
  assert.ok(!stored.includes('alice-token'));
  assert.equal(table.grant('alice-token', v.boss), tokenLink);
  assert.equal(table.authenticate('alice-token'), v.boss);

  const sessions = LinksSessions.withPermissions(w.links, w.policy).withAuthenticator(
    (store, token) => new LinksTokenTable(store).authenticate(token),
  );
  const session = sessions.open();
  assert.equal(sessions.request(session, '(create: (0 0))'), "(error: 'Forbidden: create')");
  assert.equal(sessions.request(session, '(auth: alice-second-token)'), `(subject: ${v.alice})`);
  assert.equal(sessions.request(session, '(count: ())'), `(count: ${seen})`);
  const again = new LinksTokenTable(sessions.store());
  assert.ok(again.revoke('alice-second-token'));
  assert.ok(!again.revoke('alice-second-token'));
  assert.equal(again.authenticate('alice-second-token'), null);
  assert.equal(sessions.request(session, '(auth: alice-second-token)'), AUTH_FAILED);
});

test('a session that is not open is opened by its first request, as the default subject', () => {
  const w = new World(false);
  const sessions = LinksSessions.withPermissions(w.links, w.policy).withSubject(w.v.boss);
  assert.equal(sessions.request(41, "(get-by-name: 'secret')"), `(link: ${w.v.secret})`);
  assert.deepEqual(sessions.sessions(), [41]);
  assert.equal(sessions.subject(41), w.v.boss);
  // open() hands out its own numbers, the default subject unless told another.
  const first = sessions.open();
  assert.equal(sessions.subject(first), w.v.boss);
  assert.equal(sessions.subject(sessions.open(w.v.alice)), w.v.alice);
  assert.deepEqual(sessions.sessions(), [0, 1, 41]);
});

// sessions_faults_tests.rs: a store that fails, or panics, mid request.

/** Steps until the store fails, or panics once when told to. */
class Steps {
  left = Infinity;
  panics = false;
  /** Whether the budget comes back after a panic, as it does in Rust. */
  refills = true;

  step() {
    if (this.left === 0 && this.panics) {
      this.panics = false;
      if (this.refills) this.left = Infinity;
      // JavaScript has no panic. A thrown value no error handler can read
      // escapes every `catch` that turns errors into replies, as a panic does.
      throw new Proxy(
        {},
        {
          get() {
            throw new Error('injected panic');
          },
        },
      );
    }
    if (this.left === 0) throw new Error('injected fault');
    this.left -= 1;
  }
}

/** A store whose writes and name lookups each take a step. */
class Faulty extends LinkStorage {
  constructor(steps) {
    super('');
    this.steps = steps;
  }

  queryLimits() {
    return QueryLimits.causal();
  }

  tryCreate(source, target) {
    this.steps.step();
    return this.create(source, target);
  }

  tryEnsureCreated(id) {
    this.steps.step();
    return this.ensureCreated(id);
  }

  updateObserved(id, source, target, observer) {
    this.steps.step();
    return super.updateObserved(id, source, target, observer);
  }

  deleteObserved(id, observer) {
    this.steps.step();
    return super.deleteObserved(id, observer);
  }

  getByName(name) {
    this.steps.step();
    return super.getByName(name);
  }
}

/** Everyone may do everything, except read a link it was told not to. */
class Open extends AccessPolicy {
  constructor(hidden) {
    super();
    this.hidden = hidden;
  }

  markers() {
    return new PermissionMarkers(1, 2, 3, 4);
  }

  canRead(_data, _subject, value) {
    return value.index !== this.hidden;
  }

  canWrite() {
    return true;
  }

  canCreate() {
    return true;
  }

  canReference() {
    return true;
  }
}

function faultySessions(steps, hidden) {
  const store = new Faulty(steps);
  store.create(1, 1);
  return LinksSessions.withPermissions(store, new Open(hidden));
}

const TWO_LINKS = '() ((7: 7 7) (8: 8 8))';

test('a malformed request is answered as an error', () => {
  const sessions = faultySessions(new Steps(), 0);
  const session = sessions.open();
  for (const [text, expected] of [
    ['(get-name: x)', 'expected a number'],
    ['(get-by-name: (a b))', 'expected get-by-name'],
  ]) {
    const reply = sessions.execute(session, parseDocument(text));
    assert.ok(errorMessage(reply).includes(expected), text);
  }
});

test('a failed name lookup is answered as an error', () => {
  const steps = new Steps();
  const sessions = faultySessions(steps, 0);
  const session = sessions.open();
  steps.left = 0;
  const reply = sessions.execute(session, parseDocument('(get-by-name: a)'));
  assert.equal(errorMessage(reply), 'injected fault');
});

test('a failed undo is reported along with the failure', () => {
  const steps = new Steps();
  const sessions = faultySessions(steps, 0);
  const session = sessions.open();
  // Nothing was written, so there is nothing to undo.
  steps.left = 0;
  let reply = sessions.execute(session, queryDocument(TWO_LINKS));
  assert.equal(errorMessage(reply), 'injected fault');
  assert.equal(sessions.store().allLinks().length, 1);
  // The first write landed and the budget is gone before its undo.
  steps.left = 1;
  reply = sessions.execute(session, queryDocument(TWO_LINKS));
  assert.equal(errorMessage(reply), 'injected fault; undoing it failed: injected fault');
  assert.equal(sessions.store().allLinks().length, 2);
});

test('a panicking store rolls the request back and keeps the session', () => {
  const steps = new Steps();
  const sessions = faultySessions(steps, 0);
  const session = sessions.open(5);
  steps.left = 1;
  steps.panics = true;
  assert.throws(() => sessions.execute(session, queryDocument(TWO_LINKS)), /injected panic/);
  assert.equal(sessions.store().allLinks().length, 1);
  assert.deepEqual(sessions.sessions(), [session]);
  assert.equal(sessions.subject(session), 5);
  const reply = sessions.execute(session, queryDocument(TWO_LINKS));
  assert.equal(errorMessage(reply), null);
  assert.equal(sessions.store().allLinks().length, 3);
});

test('a panic whose undo fails too is still rethrown, the session kept', () => {
  const steps = new Steps();
  steps.refills = false;
  const sessions = faultySessions(steps, 0);
  const session = sessions.open();
  steps.left = 1;
  steps.panics = true;
  // The undo of the first write runs out of steps: Rust ignores it while unwinding.
  assert.throws(() => sessions.execute(session, queryDocument(TWO_LINKS)), /injected panic/);
  assert.equal(sessions.store().allLinks().length, 2);
  assert.deepEqual(sessions.sessions(), [session]);
});

test('the policy and the store can be taken back', () => {
  const sessions = faultySessions(new Steps(), 5);
  assert.equal(sessions.policy().hidden, 5);
  // A session that is not open reads as a guest under the same policy.
  const session = sessions.open();
  sessions.close(session);
  assert.ok(sessions.canRead(session, link(4, 1, 1)));
  assert.ok(!sessions.canRead(session, link(5, 1, 1)));
  assert.equal(sessions.store().allLinks().length, 1);
});

// checked_sessions_tests.rs: the limits of the store bound every request.

/** A store of links with the limits of checked_sessions_tests.rs. */
class Checked extends LinkStorage {
  constructor() {
    super('');
  }

  queryLimits() {
    return new QueryLimits({ ...QueryLimits.causal(), maxBytes: 64, maxDepth: 4 });
  }
}

test('permission and undo layers preserve checked query admission', () => {
  const store = new Checked();
  const policy = PermissionPolicy.inData(store);
  const view = new PermissionsDecorator(new UndoableLinksDecorator(store), policy, 0);
  assert.equal(view.queryLimits().maxDepth, 4);
  const sessions = new LinksSessions(view);
  const session = sessions.open();
  const document = queryDocument('x'.repeat(65));
  assert.match(errorMessage(sessions.execute(session, document)), /byte limit/);
  const text = `${'(n: '.repeat(12)}x${')'.repeat(12)}`;
  assert.match(sessions.request(session, text), /error:/);
  assert.match(errorMessage(sessions.execute(session, parseDocument(text))), /depth limit/);
  // Within the limits a request runs; the view is a guest's, who sees nothing.
  assert.equal(sessions.request(session, '(count: ())'), '(count: 0)');
});

test('the lossless envelope keeps observed changes for subscribers', () => {
  const sessions = new LinksSessions(LinkStorage.inMemory());
  const session = sessions.open();
  const changes = [];
  const reply = sessions.executeObserved(session, queryDocument('() ((1: 1 1))'), changes);
  assert.equal(errorMessage(reply), null);
  assert.deepEqual(changes, [[NULL_LINK, link(1, 1, 1)]]);
});

test('missing references are created only when asked to', () => {
  const plain = new LinksSessions(
    new NamedTypesDecorator(LinkStorage.inMemory(), LinkStorage.inMemory()),
  );
  const session = plain.open();
  assert.match(plain.request(session, '() ((a b))'), /error:/);
  const auto = new LinksSessions(
    new NamedTypesDecorator(LinkStorage.inMemory(), LinkStorage.inMemory()),
  ).withAutoCreateMissingReferences(true);
  assert.equal(errorMessage(parseDocument(auto.request(auto.open(), '() ((a b))'))), null);
  assert.ok(auto.store().getByName('a') > 0);
});

// triplets_protocol_tests.rs: sessions in triplet mode.

test('triplet mode without permissions answers triplets and names', () => {
  const store = new NamedTypesDecorator(LinkStorage.inMemory(), LinkStorage.inMemory());
  const sessions = new LinksSessions(store);
  assert.ok(!sessions.hasTriplets());
  const markers = sessions.ensureTriplets();
  assert.equal(sessions.ensureTriplets(), markers);
  assert.equal(sessions.withTriplets(), sessions);
  assert.ok(sessions.hasTriplets());
  const session = sessions.open();
  const changed = [];
  const reply = sessions.executeTripletsObserved(
    session,
    parseDocument('(create: (0 0 0))'),
    changed,
  );
  assert.equal(errorMessage(reply), null);
  assert.equal(changed.length, 1);
  assert.equal(changed[0].before, null);
  assert.equal(changed[0].after.index, changed[0].after.typeId);
  assert.ok(sessions.canRead(session, link(changed[0].after.index, 0, 0)));
});

test('a subject reads and writes only the triplets it may', () => {
  const w = new World(false);
  const { t, v } = w;
  const sessions = LinksSessions.withPermissions(w.links, w.policy)
    .withTriplets()
    .withAuthenticator((_, token) => v[token] ?? null);
  const alice = sessions.open();
  const boss = sessions.open();
  assert.equal(sessions.request(alice, '(auth: alice)'), `(subject: ${v.alice})`);
  assert.equal(sessions.request(boss, '(auth: boss)'), `(subject: ${v.boss})`);
  const { note, shared, secret } = v;
  // boss writes anything, so reads anything.
  assert.equal(sessions.request(boss, `(each: (${secret}))`), `(${secret}: ${t.Secret} 0 0)`);
  // alice reads the notes she may write and what is public; a secret is not there at all.
  assert.equal(sessions.request(alice, `(each: (${secret}))`), '');
  assert.equal(sessions.request(alice, `(each: (* ${t.Secret}))`), '');
  assert.equal(sessions.request(alice, `(each: (${note}))`), `(${note}: ${t.Note} 0 0)`);
  assert.equal(sessions.request(alice, `(each: (${shared}))`), `(${shared}: ${t.Shared} 0 0)`);
  assert.equal(sessions.request(alice, `(count: (${secret}))`), '(count: 0)');
  assert.ok(!sessions.canRead(alice, w.link(secret)));
  assert.ok(sessions.canRead(boss, w.link(secret)));
  // Names go through the permissions as over doublets.
  assert.equal(sessions.request(alice, `(get-name: ${secret})`), '');
  assert.equal(sessions.request(boss, `(get-name: ${secret})`), '(name: secret)');
  assert.equal(sessions.request(alice, "(get-by-name: 'secret')"), '');

  // alice may write notes: creating one, and turning it to point at another.
  const changed = [];
  const reply = sessions.executeTripletsObserved(
    alice,
    parseDocument(`(create: (${t.Note} ${note} ${shared}))`),
    changed,
  );
  assert.equal(errorMessage(reply), null);
  const created = changed[0].after;
  assert.equal(created.from, note);
  const i = created.index;
  assert.equal(
    sessions.request(alice, `(update: (${i} ${t.Note} ${shared} ${note}))`),
    `((${i}: ${t.Note} ${note} ${shared})) ((${i}: ${t.Note} ${shared} ${note}))`,
  );
  // She may not make one of the secrets, nor make anything secret; a failed
  // request reports no triplets.
  for (const forbidden of [`(create: (${t.Secret} 0 0))`, `(update: (${i} ${t.Secret} 0 0))`]) {
    const none = [];
    const answer = sessions.executeTripletsObserved(alice, parseDocument(forbidden), none);
    assert.match(errorMessage(answer), /Forbidden/, forbidden);
    assert.deepEqual(none, []);
  }
  // What she cannot read is not a triplet to her.
  for (const hidden of [
    `(delete: ${secret})`,
    `(update: (${secret} ${t.Note} 0 0))`,
    `(create: (${t.Note} ${secret} 0))`,
  ]) {
    assert.match(sessions.request(alice, hidden), /is not a triplet/, hidden);
  }
  // A malformed triplet operation is an error, not a request for names.
  assert.match(sessions.request(alice, '(create: (a))'), /error:/);
  // Boss may.
  assert.ok(
    sessions.request(boss, `(delete: ${note})`).startsWith(`((${note}: ${t.Note} 0 0)) ()`),
  );
});
