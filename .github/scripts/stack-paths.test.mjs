import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { STACK_PATHS, run, stackFiles } from './stack-paths.mjs';

test('each stack keeps the inputs its old pull_request path filter listed', () => {
  assert.deepEqual(stackFiles('rust', ['rust/src/lib.rs', 'csharp/A.cs', 'README.md']), [
    'rust/src/lib.rs',
  ]);
  assert.deepEqual(
    stackFiles('csharp', ['examples/sqlite-csharp/Program.cs', 'examples/ipc/run.sh']),
    ['examples/sqlite-csharp/Program.cs'],
  );
  for (const stack of Object.keys(STACK_PATHS)) {
    for (const shared of [
      'docs/protocol/graphql/deep-fresh.json.gz',
      'parity/golden-fixtures.json',
      '.github/workflows/javascript.yml',
      '.github/workflows/javascript-stage.yml',
      '.github/scripts/pipeline-status.mjs',
      `.github/workflows/${stack}.yml`,
    ])
      assert.deepEqual(stackFiles(stack, [shared]), [shared], `${stack}: ${shared}`);
  }
  assert.deepEqual(stackFiles('rust', ['.github/workflows/csharp.yml', 'js/src/App.jsx']), []);
  assert.throws(() => stackFiles('python', []), /Unknown stack/);
});

test('only pull requests may skip a stack, and the verdict reaches GITHUB_OUTPUT', () => {
  const dir = mkdtempSync(join(tmpdir(), 'stack-paths-'));
  try {
    const output = join(dir, 'output');
    writeFileSync(output, '');
    const lines = [];
    for (const event of ['push', 'workflow_dispatch', 'schedule'])
      assert.equal(
        run('rust', { GITHUB_EVENT_NAME: event, GITHUB_OUTPUT: output }, (l) => lines.push(l)),
        true,
      );
    assert.equal(readFileSync(output, 'utf8'), 'stack-changed=true\n'.repeat(3));
    assert.throws(() => run('rust', { GITHUB_EVENT_NAME: 'pull_request' }), /are required/);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
