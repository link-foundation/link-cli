import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { CausalVersion } from '../src/engine-causal-version.js';
import { CausalProjection } from '../src/engine-causal-projection.js';

const author = n => BigInt(n).toString(16).padStart(32, '0');
const row = name => ({ source: '0', target: '2', name });
const wire = values => values.map(CausalVersion.encode);
test('independent projection vectors select from complete siblings before visibility', () => {
  const { cases } = JSON.parse(readFileSync(new URL('../../../docs/protocol/causal-projection-v1.json', import.meta.url)));
  assert.equal(cases.length, 132);
  for (const fixture of cases) {
    const values = fixture.input.map(value => CausalVersion.decode(JSON.stringify(value)));
    for (const input of [values, [...values].reverse()]) {
      const before = wire(input);
      if (!fixture.valid) assert.throws(() => new CausalProjection(input, fixture.policy),
        fixture.error === 'conflict' ? /conflict/i : /invalid/i, fixture.name);
      else {
        const projected = new CausalProjection(input, fixture.policy);
        assert.deepEqual(wire(projected.versions()), fixture.retained.map(JSON.stringify), fixture.name);
        assert.deepEqual(wire(projected.selected()), fixture.selected.map(JSON.stringify), fixture.name);
        assert.deepEqual(projected.rows(), fixture.rows.map(([index, source, target, name]) =>
          ({ index, source, target, name })), fixture.name);
      }
      assert.deepEqual(wire(input), before, fixture.name);
    }
  }
});

test('production projection owns caller and returned values and retains losing live siblings', () => {
  const live = CausalVersion.advance('1', author(1), '0', [], row('\ufeff雪😀\0'));
  const deleted = CausalVersion.advance('1', author(2), '0', [], null);
  const values = [live, deleted], expected = wire(values);
  const projected = new CausalProjection(values, 'max-dot');
  values[0].payload.name = 'caller'; values[0].clock[0].counter = '99';
  values.push(live);
  assert.deepEqual(projected.rows(), []);
  assert.deepEqual(wire(projected.versions()), expected);
  const retained = projected.versions(); retained[0].payload.name = 'returned';
  retained.pop(); projected.selected()[0].clock[0].counter = '99';
  assert.deepEqual(wire(projected.versions()), expected);
  const visible = new CausalProjection([CausalVersion.advance('2', author(3), '0', [], row(''))], 'reject');
  visible.rows()[0].name = 'returned';
  assert.deepEqual(visible.rows(), [{ index: '2', source: '0', target: '2', name: '' }]);
});

test('projection bounds complete supplied input before deduplication or dominance', () => {
  const value = CausalVersion.advance('1', author(1), '0', [], row('row'));
  for (const policy of [undefined, null, 'first', 1, {}, []]) {
    assert.throws(() => new CausalProjection([], policy), /invalid/i);
  }
  for (const options of [null, [], '', 1, () => {}, { maxVersions: 0 }, { maxVersions: 4097 },
    { maxBytes: 0 }, { maxBytes: 8388609 }, { maxVersions: 1.5 }, { maxBytes: NaN },
    { maxBytes: Infinity }, { maxVersions: '1' }]) {
    assert.throws(() => new CausalProjection([], 'reject', options), /invalid/i);
  }
  for (const input of [null, {}, '', [null], [undefined], Array(1)]) {
    assert.throws(() => new CausalProjection(input, 'reject'), /invalid/i);
  }
  assert.throws(() => new CausalProjection([value, value], 'reject', { maxVersions: 1 }), /invalid/i);
  assert.deepEqual(wire(new CausalProjection([value, value], 'reject').versions()), [CausalVersion.encode(value)]);
  assert.throws(() => new CausalProjection([value], 'reject', { maxBytes: CausalVersion.encode(value).length - 1 }), /invalid/i);
  assert.equal(new CausalProjection([value], 'reject', { maxBytes: CausalVersion.encode(value).length }).rows().length, 1);
  assert.throws(() => new CausalProjection(Array(65).fill(value), 'max-dot'), /invalid/i);
  const concurrent = Array.from({ length: 64 }, (_, index) =>
    CausalVersion.advance('1', author(index + 1), '0', [], null));
  assert.equal(new CausalProjection(concurrent, 'max-dot').versions().length, 64);
  assert.deepEqual(new CausalProjection([], 'reject', { maxVersions: 1, maxBytes: 1 }).rows(), []);
  const newer = CausalVersion.advance('1', author(1), '1', [value], null);
  assert.throws(() => new CausalProjection([value, newer], 'max-dot', { maxVersions: 1 }), /invalid/i);
});

test('projection preserves full u64 values, null names and numeric address order', () => {
  const input = ['18446744073709551615', '10', '2'].map((address, index) =>
    CausalVersion.advance(address, author(index + 1), '18446744073709551614', [],
      { source: '18446744073709551615', target: '0', name: null }));
  const projected = new CausalProjection(input, 'reject');
  assert.deepEqual(projected.rows().map(value => value.index), ['2', '10', '18446744073709551615']);
  assert.equal(projected.rows()[0].source, '18446744073709551615');
  assert.equal(projected.rows()[0].name, null);
  assert.equal(projected.selected()[0].dot.counter, '18446744073709551615');
});

test('finite admission accepts exactly 4096 versions and 8 MiB but rejects the next input', { timeout: 30000 }, () => {
  const make = (index, name = '', source = '0') => CausalVersion.advance(String(index), author(1),
    String(index - 1), [], { source, target: '0', name });
  const many = Array.from({ length: 4096 }, (_, index) => make(index + 1));
  assert.equal(new CausalProjection(many, 'reject').rows().length, 4096);
  assert.throws(() => new CausalProjection([...many, make(4097)], 'reject'), /invalid/i);
  const budget = 8388608, wide = []; let bytes = 0;
  while (true) {
    const next = make(wide.length + 1, 'a'.repeat(4096));
    const size = CausalVersion.encode(next).length;
    if (bytes + size + 256 > budget) break;
    wide.push(next); bytes += size;
  }
  let last = make(wide.length + 1);
  let rest = budget - bytes - CausalVersion.encode(last).length;
  if (rest % 2) {
    last = make(wide.length + 1, '', '10');
    rest = budget - bytes - CausalVersion.encode(last).length;
  }
  assert.ok(rest >= 0 && rest <= 8192 && rest % 2 === 0);
  last.payload.name = 'a'.repeat(rest / 2); wide.push(last);
  assert.equal(wide.reduce((sum, value) => sum + CausalVersion.encode(value).length, 0), budget);
  assert.equal(new CausalProjection(wide, 'reject').rows().length, wide.length);
  const extra = make(wide.length + 1);
  assert.throws(() => new CausalProjection([...wide, extra], 'reject'), /invalid/i);
});
