// Tests of the doublet helpers of src/link.js, the port of `GenericLink<u32>`
// in rust/src/link.rs (cases of rust/tests/link_tests.rs).
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  formatPlain,
  isFullPoint,
  isNull,
  isPartialPoint,
  isU32Text,
  link,
  NULL_LINK,
  parseU32,
  sameLink,
} from '../src/link.js';

describe('link', () => {
  it('is a frozen record of index, source and target', () => {
    const value = link(1, 2, 3);
    assert.deepEqual({ ...value }, { index: 1, source: 2, target: 3 });
    assert.ok(Object.isFrozen(value));
    assert.ok(isNull(NULL_LINK));
    assert.ok(!isNull(link(0, 0, 1)));
  });

  it('formatPlain is the `(index source target)` form of GenericLink::format', () => {
    assert.equal(formatPlain(link(1, 2, 3)), '(1 2 3)');
    assert.equal(formatPlain(NULL_LINK), '(0 0 0)');
    assert.equal(formatPlain(link(0xffffffff, 0, 7)), '(4294967295 0 7)');
  });

  it('tells full and partial points and equal links apart', () => {
    assert.ok(isFullPoint(link(2, 2, 2)));
    assert.ok(!isFullPoint(link(2, 2, 1)));
    assert.ok(isPartialPoint(link(2, 2, 1)) && isPartialPoint(link(2, 1, 2)));
    assert.ok(!isPartialPoint(link(2, 1, 1)));
    assert.ok(sameLink(link(1, 2, 3), link(1, 2, 3)));
    assert.ok(!sameLink(link(1, 2, 3), null) && sameLink(null, null));
  });
});

describe('isU32Text and parseU32', () => {
  it('accept what str::parse::<u32>() accepts', () => {
    assert.equal(parseU32('0'), 0);
    assert.equal(parseU32('+42'), 42);
    assert.equal(parseU32('007'), 7);
    assert.equal(parseU32('4294967295'), 0xffffffff);
  });

  it('reject overflow, signs, blanks and other characters', () => {
    for (const text of ['4294967296', '-1', '', '+', ' 1', '1 ', '1.0', '0x1', '++1']) {
      assert.equal(isU32Text(text), false, JSON.stringify(text));
      assert.equal(parseU32(text), null, JSON.stringify(text));
    }
  });

  it('reject values that are not text at all', () => {
    for (const value of [null, undefined, 1, ['1'], { toString: () => '1' }]) {
      assert.equal(isU32Text(value), false, String(value));
      assert.equal(parseU32(value), null, String(value));
    }
  });
});
