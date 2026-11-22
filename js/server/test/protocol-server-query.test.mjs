// Checked request execution (src/protocol-server-query.js): ports the request
// cases of rust/src/protocol/server_query.rs, of rust/tests/subscriptions_tests.rs
// and of rust/tests/checked_sessions_tests.rs to the u32 store of this port.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { link, NULL_LINK } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { LinksOperation } from '../src/protocol-links-operations.js';
import {
  errorMessage,
  formatDocument,
  parseDocument,
  queryDocument,
} from '../src/protocol-lino.js';
import {
  executeObservedRequest,
  executeRequest,
  executeRequestFor,
  referenceName,
} from '../src/protocol-server-query.js';
import { QueryLimits } from '../src/query-options.js';
import { QueryProcessor } from '../src/query-processor.js';

/** A store of links and names that counts its saves and may carry query limits. */
class Store extends NamedTypesDecorator {
  #limits;
  saves = 0;

  constructor(limits = null) {
    super(LinkStorage.inMemory(), LinkStorage.inMemory());
    this.#limits = limits;
  }

  queryLimits() {
    return this.#limits;
  }

  save() {
    this.saves += 1;
    super.save();
  }
}

const processor = new QueryProcessor(false);

/** Runs `text` (parsed) and returns the reply as text. */
const run = (storage, text) =>
  formatDocument(executeRequest(storage, processor, parseDocument(text)));

/** The limits of a causal request, with `changes` applied. */
const limits = (changes) => new QueryLimits({ ...QueryLimits.causal(), ...changes });

test('a substitution query replies with its changes and saves them', () => {
  const storage = new Store();
  const changed = [];
  const reply = executeObservedRequest(storage, processor, parseDocument('() ((1 1))'), changed);
  assert.equal(formatDocument(reply), '() ((1: 1 1))');
  assert.deepEqual(changed, [[NULL_LINK, link(1, 1, 1)]]);
  assert.ok(storage.saves > 0);
  assert.equal(run(storage, '() ((2 2))'), '() ((2: 2 2))');
  assert.equal(run(storage, '((1: 1 1)) ((1: 1 2))'), '((1: 1 1)) ((1: 1 2))');
  assert.equal(run(storage, '((1: 1 2)) ()'), '((1: 1 2)) ()');
});

test('a query that changes nothing does not save', () => {
  const storage = new Store();
  run(storage, '() ((1 1))');
  const saves = storage.saves;
  assert.equal(run(storage, '() ()'), '');
  assert.equal(storage.saves, saves);
});

test('an operation replies with its document and records its changes', () => {
  const storage = new Store();
  const changed = [];
  const run = (operation) =>
    formatDocument(executeObservedRequest(storage, processor, operation.toDocument(), changed));
  assert.equal(run(LinksOperation.create(1, 1)), '() ((1: 1 1))');
  assert.equal(run(LinksOperation.update(1, 1, 2)), '((1: 1 1)) ((1: 1 2))');
  assert.equal(run(LinksOperation.count([])), '(count: 1)');
  // Where the names store keeps the name.
  assert.match(run(LinksOperation.setName(1, 'one')), /^\(link: [0-9]+\)$/);
  assert.equal(run(LinksOperation.delete(1)), '((1: 1 2)) ()');
  // Only create, update and delete change links.
  assert.deepEqual(changed, [
    [NULL_LINK, link(1, 1, 1)],
    [link(1, 1, 1), link(1, 1, 2)],
    [link(1, 1, 2), NULL_LINK],
  ]);
});

test('the empty request lists every link by index, named where it has a name', () => {
  const storage = new Store();
  for (const query of ['() ((1 1))', '() ((2 2))', '() ((1 2))']) run(storage, query);
  storage.setName(2, 'two');
  const listing = '(1: 1 1)\n(two: two two)\n(3: 1 two)';
  assert.equal(formatDocument(executeRequest(storage, processor, [])), listing);
  assert.equal(formatDocument(executeRequest(storage, processor, queryDocument(''))), listing);
  assert.equal(formatDocument(executeRequest(storage, processor, queryDocument(' \t\n'))), listing);
  assert.equal(formatDocument(executeRequest(new Store(), processor, [])), '');
});

test('the envelope keeps the quoted references of the original text', () => {
  const storage = new Store();
  const reply = executeRequestFor(storage, processor, queryDocument("() (('a b': 'a b' 'a b'))"));
  assert.equal(errorMessage(reply), null);
  assert.equal(storage.getByName('a b'), 1);
  assert.equal(formatDocument(reply), "() (('a b': 'a b' 'a b'))");
  // The envelope holds the query as text, so an operation in it is a query.
  const envelope = executeRequestFor(storage, processor, queryDocument('(count: ())'));
  assert.notEqual(formatDocument(envelope), '(count: 1)');
});

test('a misshapen envelope is an error reply', () => {
  for (const text of ['(query: (a b))', '(query: a b)']) {
    assert.equal(
      errorMessage(executeRequest(new Store(), processor, parseDocument(text))),
      'expected (query: original-query-text)',
      text,
    );
  }
});

test('failures become error replies with their causes', () => {
  const storage = new Store();
  assert.equal(
    errorMessage(executeRequest(storage, processor, LinksOperation.update(9, 1, 1).toDocument())),
    'Link not found: 9',
  );
  assert.match(
    errorMessage(executeRequest(storage, processor, parseDocument('((x: * *)) ()'))),
    /Invalid reference to non-existent link 'x'/,
  );
  assert.equal(
    errorMessage(executeRequest(storage, processor, LinksOperation.subscribe([]).toDocument())),
    'subscriptions belong to a connection to a links server',
  );
  assert.match(
    errorMessage(executeRequest(storage, processor, parseDocument('(count: (1 2 3 4))'))),
    /expected restriction/,
  );
});

test('a checked store admits only documents within its limits', () => {
  const storage = new Store(limits({ maxBytes: 64, maxDepth: 4 }));
  assert.match(
    errorMessage(executeRequest(storage, processor, queryDocument('x'.repeat(65)))),
    /byte limit/,
  );
  const deep = `${'(n: '.repeat(12)}x${')'.repeat(12)}`;
  assert.match(
    errorMessage(executeRequest(storage, processor, parseDocument(deep))),
    /depth limit/,
  );
  assert.equal(run(storage, '() ((1 1))'), '() ((1: 1 1))');
});

test('a checked store bounds the listing of every link', () => {
  const storage = new Store(limits({ maxResults: 2, maxBytes: 12 }));
  for (const query of ['() ((1 1))', '() ((2 2))']) run(storage, query);
  assert.equal(formatDocument(executeRequest(storage, processor, [])), '(1: 1 1)\n(2: 2 2)');
  // Six references of one byte each, then a name of seven bytes does not fit.
  storage.setName(2, 'seven77');
  assert.equal(
    errorMessage(executeRequest(storage, processor, [])),
    'query reply byte limit exceeded',
  );
  run(storage, '() ((3 3))');
  assert.equal(
    errorMessage(executeRequest(storage, processor, [])),
    'protocol result limit exceeded',
  );
});

test('a checked store bounds the names in the reply to a query', () => {
  // Every query below fits in 24 bytes; three references of 11 bytes do not.
  const storage = new Store(limits({ maxBytes: 24 }));
  run(storage, '() ((1 1))');
  storage.setName(1, 'a-long-name');
  assert.equal(errorMessage(executeRequest(storage, processor, parseDocument('() ((2 2))'))), null);
  assert.equal(
    errorMessage(executeRequest(storage, processor, parseDocument('((1: 1 1)) ((1: 1 2))'))),
    'query reply byte limit exceeded',
  );
});

test('a reference is its name, or else its number', () => {
  const storage = new Store();
  run(storage, '() ((1 1))');
  assert.equal(referenceName(storage, 1), '1');
  storage.setName(1, 'one');
  assert.equal(referenceName(storage, 1), 'one');
});
