// QueryProcessor paths no clink command line reaches: the admission limits of
// a checked store (rust/src/query_options.rs `QueryLimits`), the defensive
// fallbacks of rust/src/query_processor/{matching,mutations,resolution}.rs and
// rust/src/query_processor_substitution.rs, called directly.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { ANY, link } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import { searchUnspecified, solutionsAreCompatible } from '../src/query-matching.js';
import { Pattern, QueryLimits, ResolvedLink } from '../src/query-options.js';
import { QueryProcessor } from '../src/query-processor.js';
import { preserveExistingSubstitutionParts } from '../src/query-substitution.js';

/** A checked store: the in-memory store with admission limits selected. */
class LimitedStorage extends LinkStorage {
  constructor(overrides) {
    super('');
    this.limits = new QueryLimits({ ...QueryLimits.causal(), ...overrides });
  }

  queryLimits() {
    return this.limits;
  }
}

const quiet = () => new QueryProcessor(false, { stderr: () => {} });

function pointLinks(storage, count) {
  for (let id = 1; id <= count; id += 1) storage.update(storage.create(0, 0), id, id);
  return storage;
}

const plain = (changes) =>
  changes.map((pair) => pair.map((value) => (value === null ? null : { ...value })));

describe('QueryProcessor options', () => {
  it('defaults to no trace, validation and stderr diagnostics', () => {
    const processor = new QueryProcessor();
    assert.equal(processor.trace, false);
    assert.equal(processor.autoCreateMissingReferences, false);
    assert.equal(processor.validateReferences, true);
    assert.equal(typeof processor.stderr, 'function');
  });

  it('switches auto-creation of missing references', () => {
    const processor = quiet();
    assert.equal(processor.withAutoCreateMissingReferences(1), processor);
    assert.equal(processor.autoCreateMissingReferences, true);
    const storage = LinkStorage.inMemory();
    assert.deepEqual(plain(processor.processQuery(storage, '() ((7 7))')), [
      [null, { index: 7, source: 7, target: 7 }],
    ]);
    processor.withAutoCreateMissingReferences(false);
    assert.throws(() => processor.processQuery(storage, '() ((9 9))'), /non-existent link '9'/);
  });
});

describe('QueryProcessor limits', () => {
  it('limits the query bytes', () => {
    const storage = new LimitedStorage({ maxBytes: 9 });
    assert.throws(() => quiet().processQuery(storage, '() ((1 1))'), {
      message: 'Query error: query byte limit exceeded',
    });
    assert.throws(() => quiet().processQuery(storage, '() ((é é))'), {
      message: 'Query error: query byte limit exceeded',
    });
  });

  it('limits the result count', () => {
    const storage = pointLinks(new LimitedStorage({ maxResults: 1 }), 2);
    assert.throws(() => quiet().processQuery(storage, '((* *)) ()'), {
      message: 'Query error: query result limit exceeded',
    });
    assert.equal(storage.allLinks().length, 2, 'nothing is written');
    assert.equal(quiet().processQuery(storage, '((1: 1 1)) ()').length, 1);
  });

  it('limits the bindings and their bytes', () => {
    for (const overrides of [{ maxBindings: 0 }, { maxBindingBytes: 69 }]) {
      const storage = pointLinks(new LimitedStorage(overrides), 1);
      assert.throws(() => quiet().processQuery(storage, '(($index: * *)) ()'), {
        message: 'Query error: query binding memory limit exceeded',
      });
    }
    const storage = pointLinks(new LimitedStorage({ maxBindings: 1, maxBindingBytes: 70 }), 1);
    assert.equal(quiet().processQuery(storage, '(($i: * *)) ()').length, 1);
  });

  it('charges a preview reply against the byte budget', () => {
    // `() ((1 1))` over `(1: 1 1)` reports it unchanged: references 1 1 1, twice.
    const restriction = { id: null, values: null };
    const substitution = { id: null, values: [{ id: null, values: [{ id: '1' }, { id: '1' }] }] };
    quiet().preview(pointLinks(new LimitedStorage({ maxBytes: 6 }), 1), restriction, substitution);
    assert.throws(
      () =>
        quiet().preview(
          pointLinks(new LimitedStorage({ maxBytes: 5 }), 1),
          restriction,
          substitution,
        ),
      { message: 'query reply byte limit exceeded' },
    );
  });
});

/** `(1: 1 1) (2: 2 2) (3: 1 2)`. */
function three() {
  const storage = pointLinks(LinkStorage.inMemory(), 2);
  storage.create(1, 2);
  return storage;
}

/** A store that claims every address exists and holds none of them. */
const hollow = (overrides = {}) => ({
  exists: () => true,
  getLink: () => null,
  search: () => 4,
  setName: () => {},
  ...overrides,
});

describe('QueryProcessor matching', () => {
  it('tells compatible solutions', () => {
    const existing = new Map([['$x', 1]]);
    assert.equal(solutionsAreCompatible(existing, new Map([['$x', 1]])), true);
    assert.equal(solutionsAreCompatible(existing, new Map([['$y', 2]])), true);
    assert.equal(solutionsAreCompatible(existing, new Map([['$x', 2]])), false);
  });

  it('finds the lowest link with an unspecified part', () => {
    const storage = three();
    assert.equal(searchUnspecified(storage, ANY, 2), 2);
    assert.equal(searchUnspecified(storage, 1, ANY), 1);
    assert.equal(searchUnspecified(storage, 1, 2), 3);
    assert.equal(searchUnspecified(storage, 2, 1), null);
    assert.equal(searchUnspecified(storage, ANY, 9), null);
  });

  it('matches a leaf pattern against every link', () => {
    const storage = three();
    const processor = quiet();
    assert.deepEqual(
      processor.matchPattern(storage, new Pattern('$x'), new Map()),
      [1, 2, 3].map((id) => new Map([['$x', id]])),
    );
    assert.deepEqual(processor.matchPattern(storage, new Pattern('2'), new Map()), [new Map()]);
    assert.deepEqual(processor.matchedLinks(storage, new Pattern('2'), new Map()), [link(2, 2, 2)]);
    assert.equal(processor.matchedLinks(storage, new Pattern('*'), new Map()).length, 3);
  });

  it('matches a missing part, and nothing below a missing link', () => {
    const storage = three();
    const processor = quiet();
    const solution = new Map([['$a', 1]]);
    assert.deepEqual(processor.recursiveMatchSubpattern(storage, null, 1, solution), [solution]);
    const pair = new Pattern('', new Pattern('*'), new Pattern('*'));
    assert.deepEqual(processor.recursiveMatchSubpattern(storage, pair, 9, new Map()), []);
  });

  it('keeps only compatible extensions of each partial solution', () => {
    const processor = quiet();
    processor.matchPattern = (_storage, pattern) =>
      pattern.index === 'a'
        ? [new Map([['$x', 1]])]
        : [
            new Map([['$x', 2]]),
            new Map([
              ['$x', 1],
              ['$y', 3],
            ]),
          ];
    assert.deepEqual(processor.findAllSolutions(three(), [new Pattern('a'), new Pattern('b')]), [
      new Map([
        ['$x', 1],
        ['$y', 3],
      ]),
    ]);
  });
});

describe('QueryProcessor mutations', () => {
  it('reports a null link as no link', () => {
    const changes = [];
    const storage = {
      updateObserved: (_id, _source, _target, observer) => observer(link(0, 0, 0), link(1, 2, 3)),
      deleteObserved: (_id, observer) => observer(link(1, 2, 3), link(0, 0, 0)),
    };
    quiet().updateObserved(storage, 1, 2, 3, changes);
    quiet().deleteObserved(storage, 1, changes);
    assert.deepEqual(changes, [
      [null, link(1, 2, 3)],
      [link(1, 2, 3), null],
    ]);
  });

  it('falls back to the requested link when the store cannot read it back', () => {
    const changes = [];
    const processor = quiet();
    assert.equal(
      processor.createOrUpdateResolvedLink(hollow(), new ResolvedLink(5, 0, 0), changes),
      5,
    );
    assert.equal(
      processor.createOrUpdateResolvedLink(hollow(), new ResolvedLink(0, 1, 2), changes),
      4,
    );
    assert.equal(processor.ensureIndexedLink(hollow(), 6, ANY, ANY, changes), 6);
    assert.equal(processor.ensureDoublet(hollow(), 1, 2, changes), 4);
    assert.deepEqual(changes, [
      [link(5, 0, 0), link(5, 0, 0)],
      [link(4, 1, 2), link(4, 1, 2)],
      [link(6, 0, 0), link(6, 0, 0)],
      [link(4, 1, 2), link(4, 1, 2)],
    ]);
  });

  it('ensures an empty reference as the null address', () => {
    const changes = [];
    assert.equal(quiet().ensureLinkCreated(three(), { id: null, values: null }, changes), 0);
    assert.deepEqual(changes, []);
  });
});

describe('QueryProcessor resolution', () => {
  it('rejects a link pattern missing a part', () => {
    const processor = quiet();
    const storage = three();
    assert.throws(
      () =>
        processor.resolvePatternReadonly(
          storage,
          new Pattern('', null, new Pattern('1')),
          new Map(),
          false,
        ),
      { message: 'Invalid link format: Invalid source pattern' },
    );
    assert.throws(
      () =>
        processor.resolvePatternReadonly(
          storage,
          new Pattern('', new Pattern('1')),
          new Map(),
          true,
        ),
      { message: 'Invalid link format: Invalid target pattern' },
    );
  });
});

describe('preserveExistingSubstitutionParts', () => {
  const pattern = new Pattern('3', new Pattern('$s'), new Pattern('$t'));

  it('keeps unbound parts of a link it has not visited', () => {
    const solution = new Map();
    const parts = { source: 0, target: 0 };
    preserveExistingSubstitutionParts(three(), pattern, solution, 3, parts, new Set());
    assert.deepEqual(parts, { source: 1, target: 2 });
    assert.deepEqual(
      solution,
      new Map([
        ['$s', 1],
        ['$t', 2],
      ]),
    );
  });

  it('leaves a part pointing into the links being resolved unbound', () => {
    const solution = new Map([['$t', 7]]);
    const parts = { source: 0, target: 0 };
    preserveExistingSubstitutionParts(three(), pattern, solution, 3, parts, new Set([1]));
    assert.deepEqual(parts, { source: 0, target: 7 });
    assert.deepEqual(solution, new Map([['$t', 7]]));
  });

  it('skips a link it is already resolving', () => {
    const parts = { source: 0, target: 0 };
    preserveExistingSubstitutionParts(three(), pattern, new Map(), 3, parts, new Set([3]));
    assert.deepEqual(parts, { source: 0, target: 0 });
  });

  it('reports a link the store cannot read back', () => {
    assert.throws(
      () => preserveExistingSubstitutionParts(hollow(), pattern, new Map(), 5, {}, new Set()),
      { message: 'Link not found: 5' },
    );
  });
});
