// The links interface as LiNo documents (src/protocol-links-operations.js):
// ports the operation and reply cases of rust/tests/remote_links_tests.rs and
// replays docs/protocol/links-operations.txt against a fresh store.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { link, NULL_LINK } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import { NamedTypesDecorator } from '../src/named-types.js';
import { BinaryLinoOptions } from '../src/protocol-binary.js';
import { ProtocolError } from '../src/protocol-errors.js';
import {
  ANY,
  LinksOperation,
  changesDocument,
  group,
  isNotification,
  linkLino,
  malformedArgument,
  malformedReply,
  matches,
  named,
  notificationDocument,
  notificationNumber,
  number,
  numbersLino,
  parseChanges,
  parseCount,
  parseLinkReply,
  parseLinks,
  parseName,
  parseNotification,
  parseNumber,
  parseNumbers,
  parseRestrictionOf,
  parseSubscription,
  parseTextNumber,
  parts,
  restrictionLino,
  subscriptionDocument,
} from '../src/protocol-links-operations.js';
import { formatDocument, parseDocument } from '../src/protocol-lino.js';
import { BinaryLinoProtocol, StreamReader, TextLinoProtocol } from '../src/protocol-protocols.js';
import { QueryLimits } from '../src/query-options.js';

const GOLDEN = new URL('../../../docs/protocol/links-operations.txt', import.meta.url);

/** A store of links and names, both in memory. */
const namedStore = () => new NamedTypesDecorator(LinkStorage.inMemory(), LinkStorage.inMemory());

/** A store that checks every request against `limits`, as a checked transport does. */
class LimitedStore extends NamedTypesDecorator {
  #limits;

  constructor(limits) {
    super(LinkStorage.inMemory(), LinkStorage.inMemory());
    this.#limits = limits;
  }

  queryLimits() {
    return this.#limits;
  }
}

const parse = (text) => parseDocument(text);

/** The error `action` throws. */
function thrown(action) {
  try {
    action();
  } catch (error) {
    return error;
  }
  assert.fail('expected an error');
}

/** A stream that hands out `bytes` once. */
const bytesReader = (bytes) => {
  let left = bytes;
  return new StreamReader({
    async read() {
      const chunk = left;
      left = Buffer.alloc(0);
      return chunk;
    },
  });
};

const PROTOCOLS = [
  new TextLinoProtocol(),
  new BinaryLinoProtocol(),
  new BinaryLinoProtocol({ options: new BinaryLinoOptions({ externalReferences: true }) }),
];

const OPERATIONS = [
  LinksOperation.count([]),
  LinksOperation.count([1]),
  LinksOperation.each([null, 2]),
  LinksOperation.each([1, null, 3]),
  LinksOperation.create(1, 2),
  LinksOperation.update(3, 4, 5),
  LinksOperation.delete(6),
  LinksOperation.getName(7),
  LinksOperation.setName(8, 'a name'),
  LinksOperation.getByName('a name'),
  LinksOperation.removeName(9),
  LinksOperation.subscribe([null, 1, null]),
  LinksOperation.unsubscribe(10),
];

test('operations round trip through their documents', async () => {
  for (const protocol of PROTOCOLS) {
    for (const operation of OPERATIONS) {
      const bytes = protocol.encode(operation.toDocument());
      const document = await protocol.readDocument(bytesReader(bytes));
      assert.deepEqual(LinksOperation.fromDocument(document), operation, operation.kind);
    }
  }
});

test('operations are written as the wire spells them', () => {
  const written = OPERATIONS.map((operation) => formatDocument(operation.toDocument()));
  assert.deepEqual(written, [
    '(count: ())',
    // `(1)` would read back as the reference `1`.
    '(count: ((1)))',
    '(each: (* 2))',
    '(each: (1 * 3))',
    '(create: (1 2))',
    '(update: (3 4 5))',
    '(delete: 6)',
    '(get-name: 7)',
    "(set-name: (8 'a name'))",
    "(get-by-name: 'a name')",
    '(remove-name: 9)',
    '(subscribe: (* 1 *))',
    '(unsubscribe: 10)',
  ]);
  assert.equal(ANY, '*');
  assert.ok(Object.isFrozen(LinksOperation.count([])));
});

test('only operation-shaped documents are operations', () => {
  for (const query of ['() ((1 1))', '((1: 1 1)) ()', '(unknown: 1)', '(count: 1 2)', 'x', '']) {
    assert.equal(LinksOperation.fromDocument(parse(query)), null, query);
  }
  const malformed = [
    '(count: (1 2 3 4))',
    '(each: (x))',
    '(create: (1))',
    '(update: (1 2))',
    '(delete: x)',
    '(delete: -1)',
    '(get-name: (1 2))',
    '(set-name: (1))',
    '(set-name: (1 (a b)))',
    '(get-by-name: (a b))',
    '(remove-name: 4294967296)',
    '(subscribe: (1 2 3 4))',
    '(unsubscribe: (1 2))',
  ];
  for (const text of malformed) {
    const error = thrown(() => LinksOperation.fromDocument(parse(text)));
    assert.ok(error instanceof ProtocolError, text);
    assert.equal(error.kind, 'malformed', text);
  }
  assert.equal(
    thrown(() => LinksOperation.fromDocument(parse('(set-name: (1 (a b)))'))).message,
    'malformed message: expected set-name, found (1 (a b))',
  );
  assert.equal(
    thrown(() => LinksOperation.fromDocument(parse('(get-by-name: (a b))'))).message,
    'malformed message: expected get-by-name, found (a b)',
  );
  assert.equal(
    thrown(() => LinksOperation.fromDocument(parse('(create: (1))'))).message,
    'malformed message: expected 2 numbers, found 1',
  );
  assert.equal(
    thrown(() => LinksOperation.fromDocument(parse('(count: (1 2 3 4))'))).message,
    'malformed message: expected restriction, found (1 2 3 4)',
  );
});

test('every operation names its addresses', () => {
  assert.deepEqual(
    OPERATIONS.map((operation) => operation.addresses()),
    [[], [1], [2], [1, 3], [1, 2], [3, 4, 5], [6], [7], [8], [], [9], [1], [10]],
  );
});

test('the argument parsers', () => {
  assert.deepEqual(parts(parse('(1 2)')[0]), ['1', '2']);
  assert.deepEqual(parts('1'), ['1']);
  const namedLink = parse('(a: b)')[0];
  assert.deepEqual(parts(namedLink), [namedLink]);
  assert.equal(parseTextNumber('4294967295'), 4294967295);
  assert.equal(
    thrown(() => parseTextNumber('4294967296')).message,
    "malformed message: expected a number, found '4294967296'",
  );
  assert.equal(
    thrown(() => parseNumber(group(['1']))).message,
    'malformed message: expected a number, found ((1))',
  );
  assert.deepEqual(parseNumbers(parse('(1 2 3)')[0], 3), [1, 2, 3]);
  assert.deepEqual(parseRestrictionOf(parse('(* 2)')[0], 3), [null, 2]);
  assert.deepEqual(parseRestrictionOf('*', 3), [null]);
  assert.equal(
    malformedArgument('things', group(['a', 'b'])).message,
    'malformed message: expected things, found (a b)',
  );
  assert.equal(
    malformedReply('count', parse('(a b)')).message,
    'malformed message: expected a count reply, found a b',
  );
});

test('the reply builders', () => {
  assert.deepEqual(restrictionLino([null, 2]), group([ANY, '2']));
  assert.deepEqual(numbersLino([1, 2]), group(['1', '2']));
  assert.deepEqual(linkLino(link(1, 2, 3)), named('1', ['2', '3']));
  assert.equal(number(7), '7');
  assert.equal(
    formatDocument(
      changesDocument([
        [NULL_LINK, link(1, 0, 0)],
        [link(1, 0, 0), NULL_LINK],
      ]),
    ),
    '() ((1: 0 0))\n((1: 0 0)) ()',
  );
  assert.equal(formatDocument(subscriptionDocument(4)), '(subscription: 4)');
  assert.equal(
    formatDocument(notificationDocument(4, [[link(1, 1, 1), link(1, 1, 2)]])),
    '(notify: 4)\n((1: 1 1)) ((1: 1 2))',
  );
});

test('replies parse back to what they carry', () => {
  assert.equal(parseCount(parse('(count: 3)')), 3);
  assert.deepEqual(parseLinks(parse('(1: 2 3)')), [link(1, 2, 3)]);
  assert.deepEqual(parseChanges(parse('() ((1: 0 0))\n((1: 0 0)) ()')), [
    [NULL_LINK, link(1, 0, 0)],
    [link(1, 0, 0), NULL_LINK],
  ]);
  // A side that is a single named link may lose its wrapper.
  assert.deepEqual(parseChanges(parse('(1: 1 1) (1: 1 2)')), [[link(1, 1, 1), link(1, 1, 2)]]);
  assert.equal(parseName(parse("(name: 'a name')")), 'a name');
  assert.equal(parseName([]), null);
  assert.equal(parseLinkReply(parse('(link: 5)')), 5);
  assert.equal(parseLinkReply([]), null);
  assert.equal(parseSubscription(parse('(subscription: 2)')), 2);
});

test('malformed replies are rejected', () => {
  for (const reply of [
    '(count: 1 2)',
    '(count: x)',
    '(name: a b)',
    '(link: 1 2)',
    '(1 2)',
    '((1 2) x)',
  ]) {
    const document = parse(reply);
    for (const parser of [parseCount, parseLinks, parseChanges, parseName, parseLinkReply]) {
      const error = thrown(() => parser(document));
      assert.ok(error instanceof ProtocolError && error.kind === 'malformed', `${reply}`);
    }
  }
  // Each kind of misshapen change side and link.
  for (const [reply, detail] of [
    ['(1 2) ()', 'expected a change side, found (1 2)'],
    ['((1 2)) ()', 'expected (index: source target), found (1 2)'],
    ['((1: 2)) ()', 'expected (index: source target), found (1: 2)'],
    ['((x: 1 2)) ()', "expected a number, found 'x'"],
    ['((1: (2 3) 4)) ()', 'expected a number, found (2 3)'],
    ['x ()', 'expected a change side, found x'],
  ]) {
    assert.equal(thrown(() => parseChanges(parse(reply))).message, `malformed message: ${detail}`);
  }
  assert.equal(
    thrown(() => parseChanges(parse('(a b c)'))).message,
    // A top-level group is written without its parentheses.
    'malformed message: expected a change reply, found a b c',
  );
  assert.equal(
    thrown(() => parseLinks(parse('x'))).message,
    'malformed message: expected (index: source target), found x',
  );
  assert.equal(
    thrown(() => parseName(parse('(count: 1)'))).message,
    'malformed message: expected a name reply, found (count: 1)',
  );
  assert.equal(
    thrown(() => parseName(parse('(name: a)\n(name: b)'))).message,
    'malformed message: expected a name reply, found (name: a)\n(name: b)',
  );
  assert.equal(
    thrown(() => parseName(parse('(name: (a b))'))).message,
    'malformed message: expected a name reply, found (name: (a b))',
  );
  assert.equal(
    thrown(() => parseCount(parse('x'))).message,
    'malformed message: expected a count reply, found x',
  );
  assert.equal(
    thrown(() => parseSubscription(parse('(count: 1)'))).message,
    'malformed message: expected a subscription reply, found (count: 1)',
  );
});

test('notifications are told apart from replies', () => {
  assert.equal(notificationNumber(parse('(notify: 3)\n() ((1: 1 1))')), 3);
  assert.equal(isNotification(parse('(notify: 3)')), true);
  for (const reply of [
    '',
    'x',
    '(count: 1)',
    '(notify: 1 2)',
    '(notify: (1 2))',
    '(notify: x)',
    '(notify: -1)',
    '(notify: 4294967296)',
  ]) {
    assert.equal(notificationNumber(parse(reply)), null, reply);
    assert.equal(isNotification(parse(reply)), false, reply);
  }
  assert.deepEqual(parseNotification(parse('(notify: 3)\n() ((1: 1 1))')), {
    subscription: 3,
    changes: [[NULL_LINK, link(1, 1, 1)]],
  });
  assert.deepEqual(parseNotification(parse('(notify: 0)')), { subscription: 0, changes: [] });
  assert.equal(
    thrown(() => parseNotification(parse('(count: 1)'))).message,
    'malformed message: expected a notify reply, found (count: 1)',
  );
});

test('restrictions match like the raw links interface', () => {
  const value = link(1, 2, 3);
  const shapes = [
    [[], true],
    [[null], true],
    [[1], true],
    [[2], false],
    [[null, 2], true],
    [[null, 3], true],
    [[null, 1], false],
    [[2, 2], false],
    [[1, 2, 3], true],
    [[null, 3, null], false],
    [[null, 2, 4], false],
    [[2, 2, 3], false],
    [[null, null, null, null], false],
  ];
  for (const [restriction, expected] of shapes) {
    assert.equal(matches(value, restriction), expected, JSON.stringify(restriction));
  }
});

/** The exchanges of docs/protocol/links-operations.txt: `[request, reply]`. */
function conversation() {
  const lines = readFileSync(GOLDEN, 'utf8')
    .split('\n')
    .filter((line) => line !== '' && !line.startsWith('#'));
  const exchanges = [];
  for (const line of lines) {
    if (line.startsWith('> ')) exchanges.push([line.slice(2), []]);
    else exchanges.at(-1)[1].push(line);
  }
  return exchanges.map(([request, reply]) => [request, reply.join('\n')]);
}

test('a store answers the shared conversation (docs/protocol/links-operations.txt)', () => {
  const exchanges = conversation();
  assert.equal(exchanges.length, 19);
  const storage = namedStore();
  for (const [request, reply] of exchanges) {
    const operation = LinksOperation.fromDocument(parse(request));
    assert.notEqual(operation, null, request);
    if (reply.startsWith('! ')) {
      assert.equal(thrown(() => operation.execute(storage)).message, reply.slice(2), request);
    } else {
      assert.equal(formatDocument(operation.execute(storage)), reply, request);
    }
  }
});

test('subscriptions are not for a store to answer', () => {
  for (const operation of [LinksOperation.subscribe([]), LinksOperation.unsubscribe(1)]) {
    assert.equal(
      thrown(() => operation.execute(namedStore())).message,
      'subscriptions belong to a connection to a links server',
    );
  }
});

test('a request for a missing link counts nothing', () => {
  const storage = namedStore();
  storage.create(1, 1);
  assert.equal(formatDocument(LinksOperation.count([9]).execute(storage)), '(count: 0)');
  assert.equal(formatDocument(LinksOperation.each([9]).execute(storage)), '');
  assert.equal(formatDocument(LinksOperation.each([1]).execute(storage)), '(1: 1 1)');
  assert.equal(formatDocument(LinksOperation.getByName('none').execute(storage)), '');
});

test('a checked store bounds every operation', () => {
  const limits = new QueryLimits({
    ...QueryLimits.causal(),
    maxAddress: 10,
    maxBytes: 8,
    maxResults: 1,
  });
  const storage = new LimitedStore(limits);
  const refused = (operation) => thrown(() => operation.execute(storage)).message;
  assert.equal(
    refused(LinksOperation.count([1, 2, 3, 4])),
    'expected at most three restriction parts',
  );
  assert.equal(refused(LinksOperation.setName(1, 'nine char')), 'protocol byte limit exceeded');
  assert.equal(refused(LinksOperation.getByName('нижняя')), 'protocol byte limit exceeded');
  assert.equal(refused(LinksOperation.create(1, 11)), 'protocol address out of range');
  assert.equal(refused(LinksOperation.each([null, 11])), 'protocol address out of range');
  assert.equal(refused(LinksOperation.delete(4294967295)), 'protocol address out of range');
  assert.equal(formatDocument(LinksOperation.create(0, 0).execute(storage)), '() ((1: 0 0))');
  assert.equal(formatDocument(LinksOperation.each([null, 0]).execute(storage)), '(1: 0 0)');
  assert.equal(formatDocument(LinksOperation.create(1, 1).execute(storage)), '() ((2: 1 1))');
  assert.equal(formatDocument(LinksOperation.count([]).execute(storage)), '(count: 2)');
  assert.equal(refused(LinksOperation.each([])), 'protocol result limit exceeded');
  // Where the names store keeps the name, as in the shared conversation.
  assert.equal(formatDocument(LinksOperation.setName(2, 'short').execute(storage)), '(link: 102)');
  assert.equal(formatDocument(LinksOperation.getByName('short').execute(storage)), '(link: 2)');
  assert.equal(formatDocument(LinksOperation.getName(2).execute(storage)), '(name: short)');
});
