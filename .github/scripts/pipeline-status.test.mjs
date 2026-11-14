import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import test from 'node:test';
import { checkPipeline } from './pipeline-status.mjs';

/** True when `workflow_call` is the workflow's only trigger. */
function isReusableOnly(text) {
  const rest = text.slice(text.indexOf('\non:\n') + 5);
  const on = rest.slice(0, rest.search(/^[A-Za-z]/m)); // up to the next top-level key
  const triggers = [...on.matchAll(/^ {2}([a-z_]+):/gm)].map((m) => m[1]);
  return triggers.length === 1 && triggers[0] === 'workflow_call';
}

test('terminal gate fails on cancelled, timed out, failed, missing and wholly skipped prerequisites', () => {
  for (const result of ['failure', 'cancelled', 'timed_out', undefined]) {
    assert.throws(() => checkPipeline({ lint: { result }, test: { result: 'skipped' } }), /lint/);
  }
  for (const needs of [{}, null, [], { lint: { result: 'skipped' } }])
    assert.throws(() => checkPipeline(needs));
  assert.deepEqual(checkPipeline({ lint: { result: 'success' }, release: { result: 'skipped' } }), [
    'release',
  ]);
});

test('every workflow terminal gate depends on every other job and executes after failed prerequisites', () => {
  for (const name of readdirSync(new URL('../workflows/', import.meta.url)).filter((name) =>
    name.endsWith('.yml'),
  )) {
    const text = readFileSync(new URL(`../workflows/${name}`, import.meta.url), 'utf8');
    // A reusable workflow reports through its caller's gate (issue #111).
    if (isReusableOnly(text)) continue;
    const jobs = [...text.slice(text.indexOf('\njobs:\n')).matchAll(/^ {2}([\w-]+):$/gm)].map(
      (m) => m[1],
    );
    const gate = text.slice(text.indexOf('\n  pipeline-status:'));
    assert.ok(jobs.includes('pipeline-status'), `${name}: missing gate`);
    for (const job of jobs.filter((job) => job !== 'pipeline-status'))
      assert.ok(gate.includes(`      - ${job}\n`), `${name}: unguarded ${job}`);
    assert.match(
      gate,
      /if: \$\{\{ !cancelled\(\)(?: && github.event.workflow_run.conclusion == 'failure')? \}\}/,
    );
    assert.match(gate, /NEEDS_JSON: \$\{\{ toJSON\(needs\) \}\}/);
  }
});

test('infrastructure reruns watch every workflow with a terminal gate (issue #172)', () => {
  const dir = new URL('../workflows/', import.meta.url);
  const rerun = readFileSync(new URL('rerun-infrastructure-failures.yml', dir), 'utf8');
  const watched = rerun
    .slice(rerun.indexOf('    workflows:\n'), rerun.indexOf('    types:'))
    .split('\n')
    .slice(1)
    .filter((line) => line.startsWith('      - '))
    .map((line) => line.slice(8).trim());
  const gated = readdirSync(dir)
    .filter((name) => name.endsWith('.yml') && name !== 'rerun-infrastructure-failures.yml')
    .map((name) => readFileSync(new URL(name, dir), 'utf8'))
    .filter((text) => !isReusableOnly(text))
    .filter((text) => text.includes('\n  pipeline-status:'))
    .map((text) => /^name: (.+)$/m.exec(text)[1].trim());
  assert.deepEqual([...watched].sort(), [...gated].sort());
});
