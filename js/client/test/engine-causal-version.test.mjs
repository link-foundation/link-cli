import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { CausalVersion } from '../src/engine-causal-version.js';

const vectors = JSON.parse(readFileSync(new URL('../../../docs/protocol/causal-version-v1.json', import.meta.url)));
const record = value => CausalVersion.decode(JSON.stringify(value));
const wires = values => values.map(value => CausalVersion.encode(value));

test('independent causal record vectors preserve canonical bytes and reject malformed inputs', () => {
  for (const fixture of vectors.cases) {
    if (fixture.valid) {
      const decoded = CausalVersion.decode(fixture.record);
      assert.equal(CausalVersion.encode(decoded), fixture.canonical, fixture.name);
    } else assert.throws(() => CausalVersion.decode(fixture.record), undefined, fixture.name);
  }
});

test('causal merge preserves incomparable update/deletion and rejects contradictory identities before pruning', () => {
  for (const fixture of vectors.merges) {
    const left = fixture.left.map(record), right = fixture.right.map(record);
    const before = structuredClone([left, right]);
    if (fixture.valid === false) assert.throws(() => CausalVersion.merge(left, right), undefined, fixture.name);
    else {
      const expected = fixture.expected.map(value => JSON.stringify(value));
      assert.deepEqual(wires(CausalVersion.merge(left, right)), expected, fixture.name);
      assert.deepEqual(wires(CausalVersion.merge(right, left)), expected, fixture.name);
    }
    assert.deepEqual([left, right], before);
  }
});

test('bounded independent delivery schedules converge and satisfy merge laws', () => {
  const events = vectors.events.map(value => [record(value)]);
  function* permutations(values) {
    if (values.length === 0) { yield []; return; }
    for (let i = 0; i < values.length; i++)
      for (const rest of permutations(values.filter((_, n) => n !== i))) yield [values[i], ...rest];
  }
  let orders = 0;
  for (const order of permutations(events)) {
    let state = [];
    for (const event of order) state = CausalVersion.merge(state, event);
    assert.deepEqual(wires(state), [JSON.stringify(vectors.events[3])]);
    orders++;
  }
  assert.equal(orders, 24);
  const states = [[], ...events, CausalVersion.merge(events[1], events[2])];
  for (const a of states) for (const b of states) for (const c of states) {
    assert.deepEqual(wires(CausalVersion.merge(a, a)), wires(a));
    assert.deepEqual(wires(CausalVersion.merge(a, b)), wires(CausalVersion.merge(b, a)));
    assert.deepEqual(wires(CausalVersion.merge(CausalVersion.merge(a, b), c)),
      wires(CausalVersion.merge(a, CausalVersion.merge(b, c))));
  }
});

test('counter advancement joins observed versions and rejects exhaustion without mutating input', () => {
  for (const fixture of vectors.advances) {
    const observed = fixture.observed.map(record), before = structuredClone(observed);
    const row = fixture.payload === null ? null : { source: fixture.payload[0], target: fixture.payload[1],
      name: fixture.payload[2] === null ? null : Buffer.from(fixture.payload[2], 'hex').toString('utf8') };
    const apply = () => CausalVersion.advance(fixture.address, fixture.author, fixture.lastCounter, observed, row);
    if (fixture.valid === false) assert.throws(apply, undefined, fixture.name);
    else assert.equal(CausalVersion.encode(apply()), JSON.stringify(fixture.expected), fixture.name);
    assert.deepEqual(observed, before);
  }
  const [a, b] = vectors.authors, [initial, update, deletion] = vectors.events.map(record);
  const siblings = CausalVersion.merge([update], [deletion]);
  const next = CausalVersion.advance('1', a, '2', siblings, { source: '4', target: '5', name: 'resolved' });
  assert.equal(CausalVersion.encode(next), JSON.stringify(vectors.events[3]));
  assert.equal(CausalVersion.advance('1', b, '7', [initial], null).dot.counter, '8');
  assert.equal(CausalVersion.advance('1', a, '0', [], null).dot.counter, '1');
  assert.equal(CausalVersion.advance('1', a, '0', [update], null).dot.counter, '3');
  const onlyB = record(['causal-version1', '1', [b, '1'], [[b, '1']], null]);
  assert.deepEqual(CausalVersion.advance('1', a, '0', [onlyB], null).clock.map(item => item.author), [a, b]);
  const before = structuredClone(siblings);
  assert.throws(() => CausalVersion.advance('1', a, '18446744073709551615', siblings, null));
  assert.deepEqual(siblings, before);
  assert.deepEqual(CausalVersion.merge([next], siblings), [next]);
});

test('public values are validated and merge results own nested state', () => {
  const initial = record(vectors.events[0]);
  for (const value of [null, 1, [], {}, { ...initial, address: '0' },
    { ...initial, dot: null }, { ...initial, clock: null },
    { ...initial, payload: { source: '2', target: '3', name: '\ud800' } },
    { ...initial, clock: [{ author: vectors.authors[0], counter: '0' }] }])
    assert.throws(() => CausalVersion.encode(value));
  for (const value of [null, 1, {}, Array(1), Array(65).fill(initial)])
    assert.throws(() => CausalVersion.merge(value, []));
  assert.throws(() => CausalVersion.encode({ ...initial, payload: { source: '0', target: '0', name: '雪'.repeat(1366) } }));
  const siblings = Array.from({ length: 65 }, (_, n) => {
    const author = (n + 1).toString(16).padStart(32, '0');
    return record(['causal-version1', '1', [author, '1'], [[author, '1']], null]);
  });
  assert.throws(() => CausalVersion.merge(siblings.slice(0, 64), siblings.slice(64)));
  const merged = CausalVersion.merge([initial], []);
  merged[0].clock[0].counter = '7'; merged[0].payload.name = 'caller';
  assert.equal(CausalVersion.encode(initial), JSON.stringify(vectors.events[0]));
  for (const [address, author, counter, observed, payload] of [
    ['0', vectors.authors[0], '0', [], null],
    ['1', '', '0', [], null], ['1', vectors.authors[0], '-1', [], null],
    ['2', vectors.authors[0], '0', [initial], null],
    ['1', vectors.authors[0], '0', [], {}],
  ]) assert.throws(() => CausalVersion.advance(address, author, counter, observed, payload));
});
