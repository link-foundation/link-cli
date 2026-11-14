// Unit tests for src/link-reference-validator.js, the port of
// rust/src/link_reference_validator.rs.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';

import {
  LinkReferenceValidator,
  compareCodePoints,
  debugString,
  writeStderrLine,
} from '../src/link-reference-validator.js';
import { link } from '../src/link.js';
import { LinkStorage } from '../src/link-storage.js';
import { parseLino } from '../src/parser.js';

const patterns = (text) => parseLino(text);

describe('compareCodePoints', () => {
  it('orders by code point, then by length', () => {
    assert.ok(compareCodePoints('a', 'b') < 0);
    assert.ok(compareCodePoints('\u{1f600}', '￿') > 0);
    assert.ok(compareCodePoints('ab', 'a') > 0);
    assert.equal(compareCodePoints('ab', 'ab'), 0);
  });
});

describe('debugString', () => {
  it('escapes as Rust str Debug does', () => {
    assert.equal(debugString('plain é'), '"plain é"');
    assert.equal(debugString('q"\\'), '"q\\"\\\\"');
    assert.equal(debugString('\n\r\t\0'), '"\\n\\r\\t\\0"');
    assert.equal(debugString('\u0001\u007f\u009f '), '"\\u{1}\\u{7f}\\u{9f} "');
  });
});

describe('writeStderrLine', () => {
  it('writes one line to process stderr', () => {
    const written = [];
    const original = process.stderr.write;
    process.stderr.write = (chunk) => {
      written.push(chunk);
      return true;
    };
    try {
      writeStderrLine('line');
      new LinkReferenceValidator(true).traceMsg('traced');
    } finally {
      process.stderr.write = original;
    }
    assert.deepEqual(written, ['line\n', 'traced\n']);
  });
});

describe('LinkReferenceValidator', () => {
  it('defaults to no trace and no auto-creation', () => {
    const validator = new LinkReferenceValidator();
    assert.equal(validator.trace, false);
    assert.equal(validator.autoCreateMissingReferences, false);
    assert.equal(validator.envelopedNames, false);
    assert.equal(validator.name('"a"'), '"a"');
    assert.equal(LinkReferenceValidator.forQuery(false, false).name('"a"'), 'a');
  });

  it('validates decoded identifiers from a custom front end', () => {
    const storage = LinkStorage.inMemory();
    storage.getOrCreateNamed('a');
    const lines = [];
    const validator = new LinkReferenceValidator(true, false, {
      stderr: (line) => lines.push(line),
    });
    assert.deepEqual(
      validator.validateLinksExistOrWillBeCreated(
        storage,
        patterns('((a 1))'),
        patterns('((b: a 1))'),
      ),
      [],
    );
    assert.deepEqual(lines, [
      '[ValidateLinksExistOrWillBeCreated] Starting validation',
      '[ValidateLinksExistOrWillBeCreated] Numeric links to be created: {}',
      '[ValidateLinksExistOrWillBeCreated] Named links to be created: {"b"}',
      "[ValidateReferencesInPattern] Named link 'a' reference validated in restriction pattern",
      '[ValidateReferencesInPattern] Link 1 reference validated in restriction pattern',
      "[ValidateReferencesInPattern] Named link 'a' reference validated in substitution pattern",
      '[ValidateReferencesInPattern] Link 1 reference validated in substitution pattern',
      '[ValidateLinksExistOrWillBeCreated] Validation completed',
    ]);
  });

  it('reads an identifier without its trailing colons, as trim_end_matches does', () => {
    const storage = LinkStorage.inMemory();
    const lines = [];
    const validator = new LinkReferenceValidator(true, false, {
      stderr: (line) => lines.push(line),
    });
    const substitution = patterns('((b: 1 1) (c: 1 1) (7: 1 1))');
    substitution[0].values[0].id = 'b:::';
    substitution[0].values[1].id = ':::';
    substitution[0].values[2].id = '7::';
    validator.validateLinksExistOrWillBeCreated(storage, [], substitution);
    assert.ok(
      lines.includes('[ValidateLinksExistOrWillBeCreated] Numeric links to be created: {7, 1}'),
    );
    assert.ok(
      lines.includes('[ValidateLinksExistOrWillBeCreated] Named links to be created: {"b"}'),
    );
    // CodeQL js/polynomial-redos: a long run of colons is trimmed in linear time.
    substitution[0].values[0].id = `b${':'.repeat(200_000)}x`;
    const started = performance.now();
    validator.validateLinksExistOrWillBeCreated(storage, [], substitution);
    assert.ok(performance.now() - started < 1000);
  });

  it('rejects a missing reference unless asked to create it', () => {
    const storage = LinkStorage.inMemory();
    assert.throws(
      () =>
        new LinkReferenceValidator().validateLinksExistOrWillBeCreated(
          storage,
          patterns('((x 5))'),
          [],
        ),
      {
        message:
          "Query error: Invalid reference to non-existent link 'x' in restriction pattern. Link 'x' does not exist and will not be created by this operation. Use --auto-create-missing-references to create missing references as point links.",
      },
    );
  });

  it('auto-creates missing numeric and named references', () => {
    const storage = LinkStorage.inMemory();
    const created = new LinkReferenceValidator(false, true).validateLinksExistOrWillBeCreated(
      storage,
      patterns('((y 3) (x 3))'),
      [],
    );
    assert.deepEqual(created, [
      [null, link(3, 0, 0)],
      [link(3, 0, 0), link(3, 3, 3)],
      // Ensuring 3 leaves 1 and 2 as free addresses, reused last-freed first.
      [null, link(2, 2, 2)],
      [null, link(1, 1, 1)],
    ]);
    assert.equal(storage.getByName('x'), 2);
    assert.equal(storage.getByName('y'), 1);
  });

  it('skips references that came to exist before they were created', () => {
    const storage = LinkStorage.inMemory();
    storage.getOrCreateNamed('a');
    const validator = new LinkReferenceValidator(false, true);
    const plan = {
      missingReferences: [
        { identifier: '1', patternType: 'restriction', numericId: 1 },
        { identifier: 'a', patternType: 'restriction', numericId: null },
      ],
      compositePairsToBeCreated: new Set(),
    };
    assert.deepEqual(validator.autoCreate(storage, plan), []);
  });
});
