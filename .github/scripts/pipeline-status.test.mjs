import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import test from 'node:test';
import { checkPipeline } from './pipeline-status.mjs';

test('terminal gate fails on cancelled, timed out, failed, missing and wholly skipped prerequisites', () => {
  for (const result of ['failure', 'cancelled', 'timed_out', undefined]) {
    assert.throws(() => checkPipeline({ lint: { result }, test: { result: 'skipped' } }), /lint/);
  }
  for (const needs of [{}, null, [], { lint: { result: 'skipped' } }]) assert.throws(() => checkPipeline(needs));
  assert.deepEqual(checkPipeline({ lint: { result: 'success' }, release: { result: 'skipped' } }), ['release']);
});

test('every workflow terminal gate depends on every other job and executes after failed prerequisites', () => {
  for (const name of readdirSync(new URL('../workflows/', import.meta.url)).filter((name) => name.endsWith('.yml'))) {
    const text = readFileSync(new URL(`../workflows/${name}`, import.meta.url), 'utf8');
    const jobs = [...text.slice(text.indexOf('\njobs:\n')).matchAll(/^  ([\w-]+):$/gm)].map((m) => m[1]);
    const gate = text.slice(text.indexOf('\n  pipeline-status:'));
    assert.ok(jobs.includes('pipeline-status'), `${name}: missing gate`);
    for (const job of jobs.filter((job) => job !== 'pipeline-status')) assert.ok(gate.includes(`      - ${job}\n`), `${name}: unguarded ${job}`);
    assert.match(gate, /if: \$\{\{ !cancelled\(\)(?: && github.event.workflow_run.conclusion == 'failure')? \}\}/);
    assert.match(gate, /NEEDS_JSON: \$\{\{ toJSON\(needs\) \}\}/);
  }
});
