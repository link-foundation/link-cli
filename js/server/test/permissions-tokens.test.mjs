// LinksTokenTable over the in-memory store: the port of the token table cases
// of rust/tests/small_api_edges_tests.rs, with the addresses rust/src/permissions/tokens.rs
// gives (`clink --tokens tokens.links --subject 13 --grant-token alice-secret`
// prints `Token granted to subject 13: 3` on a fresh table).
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { LinkStorage } from '../src/link-storage.js';
import { LinksTokenTable } from '../src/permissions-tokens.js';

const parts = (store, id) => {
  const found = store.getLink(id);
  return found == null ? null : [found.source, found.target];
};

/** A table over a fresh store holding a subject with the token `secret`. */
function tableWithAToken() {
  const table = new LinksTokenTable(LinkStorage.inMemory());
  const subject = table.store().create(0, 0);
  const link = table.grant('secret', subject);
  assert.equal(table.authenticate('secret'), subject);
  return { table, store: table.store(), subject, link };
}

/** `store`, except that `overrides` answer in its place. */
const overriding = (store, overrides) =>
  Object.create(
    store,
    Object.fromEntries(Object.entries(overrides).map(([key, value]) => [key, { value }])),
  );

describe('LinksTokenTable.nameOf', () => {
  it('is the SHA-256 of the UTF-8 token, in lowercase hex', () => {
    assert.equal(LinksTokenTable.TOKEN_NAME, 'Token');
    assert.equal(
      LinksTokenTable.nameOf('secret'),
      'sha256:2bb80d537b1da3e38bd30361aa855686bde0eacd7162fef6a25fe97bf527a25b',
    );
    assert.equal(
      LinksTokenTable.nameOf(''),
      'sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855',
    );
    assert.equal(
      LinksTokenTable.nameOf('тест'),
      'sha256:e34f6dec12c4f4599eba078f31ae8139420d21b1bd2d7ced7d22b09c2074fb48',
    );
  });
});

describe('LinksTokenTable', () => {
  it('stores a token as (t: (Token subject) t), named after its hash', () => {
    const store = LinkStorage.inMemory();
    const table = new LinksTokenTable(store);
    assert.equal(table.store(), store);
    // The addresses of `--tokens tokens.links --subject 13 --grant-token alice-secret`.
    assert.equal(table.grant('alice-secret', 13), 3);
    assert.equal(store.getByName('Token'), 1);
    assert.deepEqual(parts(store, 1), [1, 1]);
    assert.deepEqual(parts(store, 2), [1, 13]);
    assert.deepEqual(parts(store, 3), [2, 3]);
    assert.equal(store.getName(3), LinksTokenTable.nameOf('alice-secret'));
    assert.equal(store.getByName('alice-secret'), null, 'the token itself is never stored');
    assert.equal(table.authenticate('alice-secret'), 13);
  });

  it('knows no token in an empty table, nor one it was not given', () => {
    const empty = new LinksTokenTable(LinkStorage.inMemory());
    assert.equal(empty.authenticate('secret'), null);
    assert.equal(empty.revoke('secret'), false);
    assert.equal(empty.store().allLinks().length, 0, 'asking writes nothing');

    const { table } = tableWithAToken();
    assert.equal(table.authenticate('other'), null);
    assert.equal(table.revoke('other'), false);
    assert.equal(table.authenticate('secret') != null, true);
  });

  it('needs a subject', () => {
    const { table, store } = tableWithAToken();
    const before = store.allLinks().length;
    assert.throws(() => table.grant('other', 0), { message: 'A token needs a subject' });
    assert.equal(table.authenticate('other'), null);
    assert.equal(store.allLinks().length, before, 'a refused grant writes nothing');
  });

  it('grants again in place: the same link, now of the new subject', () => {
    const { table, store, subject, link } = tableWithAToken();
    const count = store.allLinks().length;
    assert.equal(table.grant('secret', subject), link, 'granting twice is idempotent');
    assert.equal(store.allLinks().length, count);
    const other = store.create(0, 0);
    assert.equal(table.grant('secret', other), link);
    assert.equal(table.authenticate('secret'), other);
    assert.equal(store.search(store.getByName('Token'), other) != null, true);
  });

  it('gives each token a link of its own, and many tokens to a subject', () => {
    const { table, subject, link } = tableWithAToken();
    const second = table.grant('second', subject);
    assert.notEqual(second, link);
    assert.equal(table.authenticate('second'), subject);
    assert.equal(table.authenticate('secret'), subject);
  });

  it('revokes a token once: its link and name go, the pair stays', () => {
    const { table, store, subject, link } = tableWithAToken();
    const pair = store.getLink(link).source;
    assert.equal(table.revoke('secret'), true);
    assert.equal(store.exists(link), false);
    assert.equal(store.getByName(LinksTokenTable.nameOf('secret')), null);
    assert.deepEqual(parts(store, pair), [store.getByName('Token'), subject]);
    assert.equal(table.authenticate('secret'), null);
    assert.equal(table.revoke('secret'), false, 'revoking twice is idempotent');
    // A revoked token can be granted again, at a fresh link.
    assert.equal(table.grant('secret', subject) > 0, true);
    assert.equal(table.authenticate('secret'), subject);
  });

  it('authenticates nobody with a token whose link was changed', () => {
    // No longer the link of a token: it does not point at itself.
    let { table, store, subject, link } = tableWithAToken();
    store.update(link, subject, subject);
    assert.equal(table.authenticate('secret'), null);
    assert.equal(table.revoke('secret'), false);

    // No pair at all, or itself where the pair should be.
    ({ table, store, link } = tableWithAToken());
    store.update(link, 0, link);
    assert.equal(table.authenticate('secret'), null);
    ({ table, store, link } = tableWithAToken());
    store.update(link, link, link);
    assert.equal(table.authenticate('secret'), null);

    // A pair that is not of the token point.
    ({ table, store, subject, link } = tableWithAToken());
    const other = store.create(subject, subject);
    store.update(link, other, link);
    assert.equal(table.authenticate('secret'), null);

    // A pair that is gone.
    ({ table, store, link } = tableWithAToken());
    const pair = store.getLink(link).source;
    store.update(link, 0, link);
    store.delete(pair);
    store.update(link, pair, link);
    assert.equal(table.authenticate('secret'), null);

    // A pair of nobody.
    ({ table, store, link } = tableWithAToken());
    store.update(store.getLink(link).source, store.getByName('Token'), 0);
    assert.equal(table.authenticate('secret'), null);
  });

  it('authenticates nobody when the named link cannot be read', () => {
    const { store, link } = tableWithAToken();
    const blind = overriding(store, { getLink: (id) => (id === link ? null : store.getLink(id)) });
    const table = new LinksTokenTable(blind);
    assert.equal(table.authenticate('secret'), null);
    assert.equal(table.revoke('secret'), false);
  });

  it('grants at a fresh link when the name holds no link', () => {
    // The name answers 0, or a link that is gone: neither is reused.
    for (const answer of [0, 99]) {
      const { store, subject } = tableWithAToken();
      const name = LinksTokenTable.nameOf('stale');
      const stale = overriding(store, {
        getByName: (wanted) => (wanted === name ? answer : store.getByName(wanted)),
      });
      const granted = new LinksTokenTable(stale).grant('stale', subject);
      assert.notEqual(granted, answer);
      assert.deepEqual(parts(store, granted).slice(1), [granted]);
      assert.equal(store.getName(granted), name);
    }
  });
});
