// The triplets of a server in triplet mode as LiNo documents
// (src/protocol-triplets-operations.js): ports the document and local-store
// cases of rust/tests/triplets_protocol_tests.rs and replays
// docs/protocol/triplets.txt through sessions in triplet mode.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { LinkStorage } from '../src/link-storage.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { ArityRange } from '../src/protocol-binary-packet.js';
import { BinaryLinoOptions } from '../src/protocol-binary.js';
import { ProtocolError } from '../src/protocol-errors.js';
import { LinksOperation } from '../src/protocol-links-operations.js';
import { errorMessage, formatDocument, parseDocument } from '../src/protocol-lino.js';
import { MessageFormat } from '../src/protocol-protocols.js';
import { LinksSessions } from '../src/protocol-sessions.js';
import {
  TripletsOperation,
  canonicalTriplets,
  executeObservedTripletsRequest,
  executeTripletsRequest,
  matchesTriplet,
  packTriplets,
  parseTriplet,
  parseTripletChanges,
  parseTripletNotification,
  parseTriplets,
  restrictionOfQuery,
  sameTriplet,
  tripletChangesDocument,
  tripletLino,
  tripletNotificationDocument,
  tripletQuery,
} from '../src/protocol-triplets-operations.js';
import { TripletQuery, TripletsOverDoublets, triplet, tripletChange } from '../src/triplets.js';

const GOLDEN = new URL('../../../docs/protocol/triplets.txt', import.meta.url);

const parse = (text) => parseDocument(text);

const namedStore = () => new NamedTypesDecorator(LinkStorage.inMemory(), LinkStorage.inMemory());

/** Triplets over a fresh store, with the markers `Pair` and `Nothing` named. */
const localTriplets = () => TripletsOverDoublets.withNamedMarkers(namedStore());

/** The error `action` throws. */
function thrown(action) {
  try {
    action();
  } catch (error) {
    return error;
  }
  assert.fail('expected an error');
}

test('operations round trip through their documents', () => {
  for (const [operation, text] of [
    [TripletsOperation.count([]), '(count: ())'],
    [TripletsOperation.each([null, 4, null, 0]), '(each: (* 4 * 0))'],
    [TripletsOperation.create(4, 5, 6), '(create: (4 5 6))'],
    [TripletsOperation.update(9, 4, 5, 0), '(update: (9 4 5 0))'],
    [TripletsOperation.delete(9), '(delete: 9)'],
    [TripletsOperation.subscribe([null, 4]), '(subscribe: (* 4))'],
  ]) {
    assert.equal(formatDocument(operation.toDocument()), text);
    assert.deepEqual(TripletsOperation.fromDocument(parse(text)), operation);
  }
  for (const other of ['(get-name: 1)', '() ((1 2 3))', '(1: 2 3 4)', '', 'x', '(count: 1 2)']) {
    assert.equal(TripletsOperation.fromDocument(parse(other)), null, other);
  }
  assert.ok(Object.isFrozen(TripletsOperation.delete(1)));
});

test('misshapen operations are malformed', () => {
  for (const [text, detail] of [
    ['(create: (1 2))', 'expected 3 numbers, found (1 2)'],
    ['(update: (1 2 3))', 'expected 4 numbers, found (1 2 3)'],
    ['(delete: x)', "expected a number, found 'x'"],
    ['(each: (1 2 3 4 5))', 'expected restriction, found (1 2 3 4 5)'],
  ]) {
    const error = thrown(() => TripletsOperation.fromDocument(parse(text)));
    assert.ok(error instanceof ProtocolError, text);
    assert.equal(error.message, `malformed message: ${detail}`, text);
  }
});

test('restrictions are matched part by part', () => {
  const value = triplet(1, 2, 3, 4);
  for (const [restriction, expected] of [
    [[], true],
    [[1], true],
    [[2], false],
    [[null, 2], true],
    [[null, 3], false],
    [[null, null, 3], true],
    [[null, null, null, 4], true],
    [[null, null, null, 5], false],
    [[1, 2, 3, 4], true],
    [[null, null, null, null, null], false],
  ]) {
    assert.equal(matchesTriplet(value, restriction), expected, JSON.stringify(restriction));
  }
  const query = tripletQuery([5, null, 7]);
  assert.deepEqual(query, new TripletQuery(5, null, 7, null));
  assert.deepEqual(restrictionOfQuery(query), [5, null, 7, null]);
});

test('two triplets are the same when every part is', () => {
  assert.equal(sameTriplet(null, null), true);
  assert.equal(sameTriplet(triplet(1, 2, 3, 4), null), false);
  assert.equal(sameTriplet(null, triplet(1, 2, 3, 4)), false);
  assert.equal(sameTriplet(triplet(1, 2, 3, 4), triplet(1, 2, 3, 4)), true);
  for (const other of [
    triplet(9, 2, 3, 4),
    triplet(1, 9, 3, 4),
    triplet(1, 2, 9, 4),
    triplet(1, 2, 3, 9),
  ]) {
    assert.equal(sameTriplet(triplet(1, 2, 3, 4), other), false);
  }
});

test('every parser reads both forms of a triplet', () => {
  const value = triplet(1, 2, 3, 4);
  for (const text of ['(1: 2 3 4)', '(1: (2 3 4))']) {
    assert.deepEqual(parseTriplets(parse(text)), [value]);
  }
  for (const text of ['((1: 2 3 4)) ()', '((1: (2 3 4))) ()', '(1: 2 3 4) ()']) {
    assert.deepEqual(parseTripletChanges(parse(text)), [tripletChange(value, null)]);
  }
  const changes = [tripletChange(null, value)];
  const document = tripletNotificationDocument(3, changes);
  assert.equal(formatDocument(document), '(notify: 3)\n() ((1: 2 3 4))');
  assert.deepEqual(parseTripletNotification(document), { subscription: 3, changes });
  assert.equal(formatDocument([tripletLino(value)]), '(1: 2 3 4)');
  assert.equal(
    formatDocument(tripletChangesDocument([tripletChange(value, triplet(1, 2, 3, 5))])),
    '((1: 2 3 4)) ((1: 2 3 5))',
  );
});

test('misshapen triplets and changes are malformed', () => {
  for (const [text, detail] of [
    ['x', 'expected (index: type from to), found x'],
    ['(1 2 3 4)', 'expected (index: type from to), found (1 2 3 4)'],
    ['(1: 2 3)', 'expected (index: type from to), found (1: 2 3)'],
    ['(1: (2 3))', 'expected (index: type from to), found (1: (2 3))'],
    ['(1: (a: 2 3 4))', 'expected (index: type from to), found (1: (a: 2 3 4))'],
    ['(x: 2 3 4)', "expected a number, found 'x'"],
    ['(1: 2 (3 4) 5)', 'expected a number, found (3 4)'],
  ]) {
    const error = thrown(() => parseTriplet(parse(text)[0]));
    assert.ok(error instanceof ProtocolError, text);
    assert.equal(error.message, `malformed message: ${detail}`, text);
  }
  for (const [text, detail] of [
    ['(a b c)', 'expected a change reply, found a b c'],
    ['x ()', 'expected a change side, found x'],
    ['(1 2) ()', 'expected a change side, found (1 2)'],
  ]) {
    assert.equal(
      thrown(() => parseTripletChanges(parse(text))).message,
      `malformed message: ${detail}`,
    );
  }
  assert.equal(
    thrown(() => parseTripletNotification(parse('(count: 1)'))).message,
    'malformed message: expected a notify reply, found (count: 1)',
  );
});

test('packing changes only the triplets of a format that holds three but not five', () => {
  const document = parse('((1: 2 3 4)) ((1: 2 3 5))\n(count: 1)\n(name: a)\nx');
  const binary = (arity) => MessageFormat.binary(new BinaryLinoOptions({ arity }));
  const packed = packTriplets(document, binary(ArityRange.between(2, 3)));
  assert.equal(formatDocument(packed), '((1: (2 3 4))) ((1: (2 3 5)))\n(count: 1)\n(name: a)\nx');
  assert.deepEqual(canonicalTriplets(packed), document);
  for (const format of [
    MessageFormat.TEXT,
    { options: null },
    {},
    binary(ArityRange.DOUBLETS),
    binary(ArityRange.atLeast(1)),
  ]) {
    assert.deepEqual(packTriplets(document, format), document);
  }
  // A named link of one value that is not three references stays as it is.
  const other = parse('(1: (2 3))\n(1: x)\n(1: (a: 2 3 4))');
  assert.deepEqual(canonicalTriplets(other), other);
});

test('a local store answers like a server in triplet mode', () => {
  const triplets = localTriplets();
  const request = (text) => formatDocument(executeTripletsRequest(triplets, parse(text)));
  assert.equal(request(''), '');
  assert.equal(request('(create: (0 0 0))'), '() ((3: 3 0 0))');
  assert.equal(request('(create: (3 0 0))'), '() ((4: 3 0 0))');
  assert.equal(request(''), '(3: 3 0 0)\n(4: 3 0 0)');
  assert.equal(
    request('(subscribe: ())'),
    "(error: 'subscriptions belong to a connection to a links server')",
  );
  assert.equal(request('(delete: 3)'), '((3: 3 0 0)) ()\n((4: 3 0 0)) ()');
});

test('the observed changes are the triplets a request changed', () => {
  const triplets = localTriplets();
  const changes = [];
  const run = (text) =>
    formatDocument(executeObservedTripletsRequest(triplets, parse(text), changes));
  assert.equal(run('(create: (0 0 0))'), '() ((3: 3 0 0))');
  triplets.setName(3, 'Type');
  assert.equal(run('(count: ())'), '(count: 1)');
  // An update to the same parts changes nothing.
  assert.equal(run('(update: (3 3 0 0))'), '((3: 3 0 0)) ((3: 3 0 0))');
  assert.equal(run('() ((Type 0 0))'), '() ((4: Type 0 0))');
  assert.equal(run('((4: Type 0 0)) ((4: Type 0 0))'), '((4: Type 0 0)) ((4: Type 0 0))');
  assert.equal(run('(get-name: 3)'), '(name: Type)');
  assert.equal(run(''), '(Type: Type 0 0)\n(4: Type 0 0)');
  assert.equal(run('(delete: 99)'), "(error: 'Link 99 is not a triplet.')");
  assert.deepEqual(changes, [
    tripletChange(null, triplet(3, 3, 0, 0)),
    tripletChange(null, triplet(4, 3, 0, 0)),
  ]);
});

test('a query that changes nothing is not saved', () => {
  const triplets = localTriplets();
  let saves = 0;
  const save = triplets.save.bind(triplets);
  triplets.save = () => {
    saves += 1;
    save();
  };
  executeTripletsRequest(triplets, parse('(create: (0 0 0))'));
  const after = saves;
  assert.equal(
    formatDocument(executeTripletsRequest(triplets, parse('((3: 3 0 0)) ((3: 3 0 0))'))),
    '((3: 3 0 0)) ((3: 3 0 0))',
  );
  assert.equal(saves, after);
  assert.equal(
    formatDocument(executeTripletsRequest(triplets, LinksOperation.count([]).toDocument())),
    '(count: 1)',
  );
});

/** The steps of docs/protocol/triplets.txt: `{ connection, push, request, reply }`. */
function conversation() {
  const steps = [];
  for (const line of readFileSync(GOLDEN, 'utf8').split('\n')) {
    if (line === '' || line.startsWith('#')) continue;
    const step = /^([AB])([<>]) (.*)$/.exec(line);
    if (step === null) steps.at(-1).reply.push(line);
    else steps.push({ connection: step[1], push: step[2] === '<', request: step[3], reply: [] });
  }
  return steps.map((step) => ({ ...step, reply: step.reply.join('\n') }));
}

test('sessions in triplet mode answer the shared conversation (docs/protocol/triplets.txt)', () => {
  const steps = conversation();
  assert.equal(steps.length, 48);
  const sessions = new LinksSessions(namedStore()).withTriplets();
  const connections = { A: sessions.open(), B: sessions.open() };
  let replayed = 0;
  for (const { connection, push, request, reply } of steps) {
    // Subscriptions and their pushes belong to a connection to a server.
    if (push || /^\((un)?subscribe:/.test(request)) continue;
    const answer = sessions.request(connections[connection], request);
    if (reply.startsWith('! ')) {
      // As in Rust, the error ends with the message (it may carry a kind).
      const message = errorMessage(parse(answer));
      assert.ok(message?.endsWith(reply.slice(2)), `${request}: ${answer}`);
    } else {
      assert.equal(answer, reply, request);
    }
    replayed += 1;
  }
  assert.equal(replayed, 42);
});
