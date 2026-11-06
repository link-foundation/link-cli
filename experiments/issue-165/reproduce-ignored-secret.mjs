// A synthetic, nonfunctional AWS example key demonstrates the ignored-log gap.
import assert from 'node:assert/strict';
import { mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { gzipSync } from 'node:zlib';

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
  writeFileSync(`${path}.gz`, gzipSync(`aws_secret_access_key = "${'aB3dE5fG7h'.repeat(4)}"\n`));
  const compressed = spawnSync(process.execPath, [
    '.github/scripts/check-secrets.mjs',
    `${path}.gz`,
  ]);
  assert.equal(compressed.status, 1, 'Compressed evidence must also be checked');
  console.log('PASS: original command skips ignored log; fixed command rejects it');
} finally {
  rmSync(directory, { recursive: true, force: true });
}
