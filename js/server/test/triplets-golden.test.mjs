// The golden transcript docs/protocol/triplets.txt, replayed on the store:
// every request a triplet store answers is run on `TripletsOverDoublets`, and
// its reply is written as the server writes it. The protocol itself, its
// subscriptions and its parse errors are the protocol tests' business.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { parseLino } from '../src/parser.js';
import { processTripletsQuery, TripletQuery } from '../src/triplets.js';
import { store } from './helpers/triplets.mjs';

const TRANSCRIPT = fileURLToPath(new URL('../../../docs/protocol/triplets.txt', import.meta.url));

/** Requests only the protocol answers, with why the store has nothing to say. */
const PROTOCOL_ONLY = new Map([
  ['(create: (1 2))', 'a create of two numbers does not parse'],
  ['(each: (1 2 3 4 5))', 'a restriction of five numbers does not parse'],
]);

/** `{ connection, request, reply }` for every request of the transcript. */
function exchanges(text) {
  const result = [];
  for (const line of text.split('\n')) {
    if (line === '' || line.startsWith('#')) continue;
    const request = /^([AB])> (.*)$/.exec(line);
    if (request != null) {
      result.push({ connection: request[1], request: request[2], reply: [] });
    } else if (/^[AB]< /.test(line)) {
      // A push: the protocol's, not the store's.
      result.push({ connection: line[0], request: null, reply: [] });
    } else {
      result.at(-1).reply.push(line);
    }
  }
  return result.filter((each) => each.request != null);
}

const number = (value) => Number(value.id);
const reference = (value) => (value.id === '*' ? null : number(value));
const write = (value) => `(${value.index}: ${value.typeId} ${value.from} ${value.to})`;

/** A restriction `(index type from to)`: a part left out or `*` matches anything. */
function restrictionOf(value) {
  const parts = (value.values ?? []).map(reference);
  return new TripletQuery(...parts);
}

/** A triplet with its parts by name where they have one, as substitution replies write it. */
function writeNamed(triplets, value) {
  const name = (id) => (id === 0 ? '0' : (triplets.getName(id) ?? String(id)));
  return `(${name(value.index)}: ${name(value.typeId)} ${name(value.from)} ${name(value.to)})`;
}

/** What the store answers to `request`, line by line. */
function answer(triplets, request) {
  // `(operation: argument)`; a substitution query has no operation.
  const [document] = parseLino(request);
  const argument = document.values[0];
  switch (document.id) {
    case 'count':
      return [`(count: ${triplets.countTriplets(restrictionOf(argument))})`];
    case 'each':
      return triplets.eachTriplet(restrictionOf(argument)).map(write);
    case 'create': {
      const [typeId, from, to] = argument.values.map(number);
      const index =
        typeId === 0 && from === 0 && to === 0
          ? triplets.createTripletPoint()
          : triplets.createTriplet(typeId, from, to);
      return [`() (${write(triplets.getTriplet(index))})`];
    }
    case 'update': {
      const [index, typeId, from, to] = argument.values.map(number);
      const before = triplets.updateTriplet(index, typeId, from, to);
      return [`(${write(before)}) (${write(triplets.getTriplet(index))})`];
    }
    case 'delete':
      return triplets.deleteTriplet(number(argument)).map((value) => `(${write(value)}) ()`);
    case 'set-name': {
      const [index, name] = argument.values.map((value) => value.id);
      triplets.setName(Number(index), name);
      // The reply is the address of the name in the protocol's name store.
      return null;
    }
    case 'get-by-name':
      return [`(link: ${triplets.getByName(argument.id)})`];
    case 'get-name':
      return [`(name: ${triplets.getName(number(argument))})`];
    case 'subscribe':
    case 'unsubscribe':
      return null;
    default: {
      const side = (value) => (value == null ? '()' : `(${writeNamed(triplets, value)})`);
      return processTripletsQuery(triplets, request).map(
        (change) => `${side(change.before)} ${side(change.after)}`,
      );
    }
  }
}

describe('docs/protocol/triplets.txt', () => {
  it('replays on the store, reply for reply', () => {
    const triplets = store();
    let replayed = 0;
    for (const { connection, request, reply } of exchanges(readFileSync(TRANSCRIPT, 'utf8'))) {
      const where = `${connection}> ${request}`;
      if (PROTOCOL_ONLY.has(request)) {
        assert.equal(reply.length, 1, where);
        assert.ok(reply[0].startsWith('! expected'), where);
        continue;
      }
      let actual;
      try {
        actual = answer(triplets, request);
      } catch (error) {
        actual = [`! ${error.message}`];
      }
      if (actual != null) {
        assert.deepEqual(actual, reply, where);
        replayed += 1;
      }
    }
    // 44 requests: all but set-name, subscribe, unsubscribe and the two parse errors.
    assert.equal(replayed, 39);
  });
});
