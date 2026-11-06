#!/usr/bin/env node
// Git is the inventory: force-added evidence must be scanned even when ignored.
import { execFileSync, spawnSync } from 'node:child_process';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const files = process.argv.slice(2);
if (!files.length) {
  files.push(
    ...execFileSync('git', ['ls-files', '-z'], { cwd: root, encoding: 'utf8' })
      .split('\0')
      .filter(Boolean),
  );
}
if (!files.length) throw new Error('No tracked files; secrets scan did not run');
for (let offset = 0; offset < files.length; offset += 100) {
  const result = spawnSync(
    process.execPath,
    [
      resolve(root, 'js/node_modules/secretlint/bin/secretlint.js'),
      '--no-gitignore',
      '--no-glob',
      ...files.slice(offset, offset + 100),
    ],
    { cwd: root, stdio: 'inherit' },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
console.log(`Secrets scan: ${files.length} tracked files checked`);
