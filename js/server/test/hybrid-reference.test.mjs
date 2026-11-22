// Hybrid references: external values in the upper half of the u32 range.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
  EXTERNAL_ZERO,
  externalReference,
  externalReferenceValue,
} from '../src/hybrid-reference.js';

describe('hybrid references', () => {
  it('encode external values as their wrapping negation', () => {
    assert.equal(EXTERNAL_ZERO, 0x80000000);
    assert.equal(externalReference(0), 0x80000000);
    assert.equal(externalReference(1), 0xffffffff);
    assert.equal(externalReference(84), 4294967212);
    assert.equal(externalReference(0x7fffffff), 0x80000001);
  });

  it('decode external addresses and leave internal ones', () => {
    assert.equal(externalReferenceValue(0x80000000), 0);
    assert.equal(externalReferenceValue(0xffffffff), 1);
    assert.equal(externalReferenceValue(0x80000001), 0x7fffffff);
    assert.equal(externalReferenceValue(5), null);
    assert.equal(externalReferenceValue(0x7fffffff), null);
  });
});
