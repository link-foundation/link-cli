// A synthetic, nonfunctional AWS example key demonstrates the ignored-log gap.
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';

const directory = 'experiments/issue-165/ignored-secret-fixture';
const path = `${directory}/example.log`;
mkdirSync(directory, { recursive: true });
try {
  writeFileSync(path, `aws_secret_access_key = "${'aB3dE5fG7h'.repeat(4)}"\n`);
  const cli = 'js/node_modules/secretlint/bin/secretlint.js';
  const before = spawnSync(process.execPath, [cli, '--secretlintignore', '.gitignore', path]);
  assert.equal(before.status, 0, 'Original command skips the ignored log');
  const after = spawnSync(process.execPath, ['.github/scripts/check-secrets.mjs', path]);
  assert.equal(
    after.status,
    1,
    `Fixed command detects the synthetic example credential: ${after.stdout} ${after.stderr}`,
  );
  console.log('PASS: original command skips ignored log; fixed command rejects it');
} finally {
  rmSync(directory, { recursive: true, force: true });
}
