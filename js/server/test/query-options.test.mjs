// The shapes a query passes through: rust/src/query_options.rs,
// rust/src/query_types.rs and rust/src/query_reply_budget.rs.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { link } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import {
  Pattern,
  QueryLimits,
  QueryOptions,
  QueryReplyBudget,
  ResolvedLink,
  utf8Length,
} from '../src/query-options.js';

describe('QueryOptions', () => {
  it('holds the query and the trace flag', () => {
    assert.deepEqual({ ...new QueryOptions('() ((1 1))') }, { query: '() ((1 1))', trace: false });
    assert.deepEqual({ ...new QueryOptions('q', 1) }, { query: 'q', trace: true });
  });
});

describe('QueryLimits', () => {
  it('has the causal limits of Rust', () => {
    const limits = QueryLimits.causal();
    assert.equal(limits.maxAddress, 18446744073709551615n);
    assert.equal(limits.maxBytes, 1048576);
    assert.equal(limits.maxDepth, 64);
    assert.equal(limits.maxNodes, 4096);
    assert.equal(limits.maxResults, 4096);
    assert.equal(limits.maxBindings, 16384);
    assert.equal(limits.maxBindingBytes, 4194304);
    assert.ok(Object.isFrozen(limits));
  });

  it('keeps the address limit as a BigInt', () => {
    const limits = new QueryLimits({ ...QueryLimits.causal(), maxAddress: 7 });
    assert.equal(limits.maxAddress, 7n);
  });
});

describe('utf8Length', () => {
  it('counts UTF-8 bytes as Rust str::len does', () => {
    assert.equal(utf8Length(''), 0);
    assert.equal(utf8Length('a'), 1);
    assert.equal(utf8Length('é'), 2);
    assert.equal(utf8Length('€'), 3);
    assert.equal(utf8Length('😀'), 4);
    assert.equal(utf8Length('a߿ࠀ￿\u{10000}'), 1 + 2 + 3 + 3 + 4);
  });
});

describe('QueryReplyBudget', () => {
  const storage = () => {
    const links = LinkStorage.inMemory();
    const id = links.getOrCreateNamed('héllo');
    return { links, id };
  };

  it('charges nothing without limits', () => {
    const { links, id } = storage();
    const budget = new QueryReplyBudget(null);
    assert.equal(budget.reference(links, id), 'héllo');
    assert.equal(budget.reference(links, 42), '42');
    assert.equal(budget.remaining, null);
  });

  it('charges the UTF-8 bytes of each name or number', () => {
    const { links, id } = storage();
    const budget = new QueryReplyBudget(new QueryLimits({ ...QueryLimits.causal(), maxBytes: 9 }));
    assert.equal(budget.reference(links, id), 'héllo');
    assert.equal(budget.remaining, 3);
    assert.equal(budget.reference(links, 123), '123');
    assert.equal(budget.remaining, 0);
    assert.throws(() => budget.reference(links, 4), {
      message: 'query reply byte limit exceeded',
    });
  });
});

describe('Pattern and ResolvedLink', () => {
  it('tells leaves from links', () => {
    assert.equal(new Pattern('$x').isLeaf(), true);
    assert.equal(new Pattern('', new Pattern('1')).isLeaf(), false);
    assert.equal(new Pattern('', null, new Pattern('1')).isLeaf(), false);
  });

  it('turns a resolved link into a link', () => {
    const resolved = new ResolvedLink(1, 2, 3);
    assert.equal(resolved.name, null);
    assert.deepEqual(resolved.toLink(), link(1, 2, 3));
    assert.equal(new ResolvedLink(1, 2, 3, '"a"').name, '"a"');
  });
});
