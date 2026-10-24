import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

const workflow = readFileSync(new URL('../workflows/js-client.yml', import.meta.url), 'utf8');
const step = workflow.split('      - name: Dry-run the release\n')[1]?.split(/\n      - name:/)[0];
assert.ok(step, 'npm release must validate pull requests');
const run = /^        run: (.+)$/m.exec(step);
assert.ok(run, 'release validation needs a run command');
const script = run[1] === '|'
  ? step.slice(run.index + run[0].length).split('\n').filter((line) => line.startsWith('          ')).map((line) => line.slice(10)).join('\n')
  : run[1];

test('npm release validation uses the actual registry lookup result', () => {
  assert.match(step, /PUBLISHED: \$\{\{ steps\.check\.outputs\.published \}\}/);
});

for (const published of ['true', 'false']) {
  test(`npm release validation succeeds when published=${published}`, () => {
    const root = mkdtempSync(join(tmpdir(), 'npm-release-136-'));
    try {
      writeFileSync(join(root, 'npm'), `#!/usr/bin/env bash
set -eu
printf '%s\\n' "$*" >> "$INVOCATIONS"
if [[ "$PUBLISHED" == true && "$1" == publish ]]; then
  echo 'You cannot publish over the previously published versions: 0.1.0.' >&2
  exit 1
fi
`, { mode: 0o755 });
      const invocations = join(root, 'invocations');
      const result = spawnSync('bash', ['-euo', 'pipefail', '-c', script], {
        env: { ...process.env, PATH: `${root}:${process.env.PATH}`, PUBLISHED: published, INVOCATIONS: invocations },
        encoding: 'utf8', timeout: 10_000,
      });
      assert.equal(result.status, 0, result.stdout + result.stderr);
      assert.equal(readFileSync(invocations, 'utf8').trim(), published === 'true' ? 'pack --dry-run' : 'publish --dry-run --access public');
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  });
}
