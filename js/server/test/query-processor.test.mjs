// QueryProcessor end to end: the query cases of rust/tests/*query*_tests.rs
// and of rust/src/query_processor tests, with the stdout, the processor trace
// and the final database that the Rust clink printed for each, kept in
// test/fixtures/query-processor-cases.json.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { describe, it } from 'node:test';
import { formatErrorReport } from '../src/error-report.js';
import { LinkStorage } from '../src/link-storage.js';
import { QueryProcessor } from '../src/query-processor.js';

const cases = JSON.parse(
  readFileSync(new URL('./fixtures/query-processor-cases.json', import.meta.url), 'utf8'),
);

// Rust prints the `HashSet` of links to create in its random order.
const sortedSet = (line) =>
  line.includes('to be created: {')
    ? line.replace(/\{(.*)\}$/, (_, items) => `{${items.split(', ').sort().join(', ')}}`)
    : line;

function run(entry) {
  const storage = LinkStorage.inMemory();
  const steps = entry.steps.map((step) => {
    const trace = [];
    const output = [];
    let failed = false;
    const processor = new QueryProcessor(Boolean(entry.trace), {
      autoCreateMissingReferences: entry.auto,
      stderr: (line) => trace.push(sortedSet(line)),
    });
    try {
      for (const [before, after] of processor.processQuery(storage, step.query)) {
        output.push(storage.formatChangeLino(before, after));
      }
    } catch (error) {
      output.push(...`Error: ${formatErrorReport(error)}`.split('\n'));
      failed = true;
    }
    const result = { query: step.query, output };
    if (failed) result.failed = true;
    if (entry.trace) result.trace = trace;
    return result;
  });
  return { steps, final: storage.linoLines() };
}

describe('QueryProcessor matches the Rust clink', () => {
  cases.forEach((entry, number) => {
    it(`${number}: ${entry.name} ${entry.steps
      .map((step) => step.query)
      .join(' ; ')
      .slice(0, 80)}`, () => {
      const actual = run(entry);
      assert.deepEqual(actual.steps, entry.steps);
      assert.deepEqual(actual.final, entry.final);
    });
  });
});
