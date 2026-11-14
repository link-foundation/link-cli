// Tests of `formatStructure` (src/structure-format.js), the port of
// rust/src/structure_format.rs, over a stub store: the source spine is
// expanded, references are resolved deepest first, a missing link is a
// `NotFoundError`, and an address already being expanded is only referenced.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { NotFoundError } from '../src/errors.js';
import { link } from '../src/link.js';
import { formatStructure } from '../src/structure-format.js';

/** A stub store of `links` that records the references it resolves, in order. */
function store(links, names = new Map()) {
  const byAddress = new Map(links.map((value) => [value.index, value]));
  return { byAddress, names, resolved: [] };
}

const get = (context, id) => context.byAddress.get(id) ?? null;
const exists = (context, id) => context.byAddress.has(id);
const reference = (context, id) => {
  context.resolved.push(id);
  return context.names.get(id) ?? String(id);
};

const format = (context, id, visited = new Set()) =>
  formatStructure(context, id, visited, get, exists, reference);

// `() ((1 1) (1 2) (2 3))` in the Rust clink.
const links = [link(1, 1, 1), link(2, 1, 2), link(3, 2, 3)];

describe('formatStructure', () => {
  it('expands the source spine as the Rust clink --structure does', () => {
    assert.equal(format(store(links), 1), '(1: 1 1)');
    assert.equal(format(store(links), 2), '(2: (1: 1 1) 2)');
    assert.equal(format(store(links), 3), '(3: (2: (1: 1 1) 2) 3)');
  });

  it('resolves the terminal first, then target and index from the deepest frame', () => {
    const context = store(links);
    format(context, 3);
    assert.deepEqual(context.resolved, [1, 1, 1, 2, 2, 3, 3]);
  });

  it('stops at a source that is not a link and uses its reference', () => {
    const context = store([link(5, 9, 5)], new Map([[9, 'nine']]));
    assert.equal(format(context, 5), '(5: nine 5)');
  });

  it('fails with NotFoundError for a missing link', () => {
    // The Rust clink: `Error: Link not found: 99`.
    assert.throws(
      () => format(store(links), 99),
      (error) => {
        assert.ok(error instanceof NotFoundError);
        assert.equal(error.message, 'Link not found: 99');
        assert.equal(error.id, 99);
        return true;
      },
    );
  });

  it('references an address the caller is already expanding instead of expanding it', () => {
    const visited = new Set([3]);
    const context = store(links, new Map([[3, 'three']]));
    assert.equal(format(context, 3, visited), 'three');
    assert.deepEqual(context.resolved, [3]);
    // A visited source ends the spine with its reference.
    assert.equal(format(store(links), 3, new Set([1])), '(3: (2: 1 2) 3)');
  });

  it('leaves the visited set as it found it', () => {
    const visited = new Set([7]);
    format(store(links), 3, visited);
    assert.deepEqual([...visited], [7]);
    assert.equal(format(store(links), 3, visited), '(3: (2: (1: 1 1) 2) 3)');
  });
});
