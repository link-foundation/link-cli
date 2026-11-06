#!/usr/bin/env node
// Git is the inventory: force-added evidence must be scanned even when ignored.
import { execFileSync, spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gunzipSync } from 'node:zlib';

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
const cli = resolve(root, 'js/node_modules/secretlint/bin/secretlint.js');
const plain = files.filter((file) => !file.endsWith('.log.gz'));
for (let offset = 0; offset < plain.length; offset += 100) {
  const result = spawnSync(
    process.execPath,
    [cli, '--no-gitignore', '--no-glob', ...plain.slice(offset, offset + 100)],
    { cwd: root, stdio: 'inherit' },
  );
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
// Compressed investigation logs remain part of the secrets scan. A corrupt or
// unexpectedly large archive is an error; scan archives one at a time.
for (const file of files.filter((item) => item.endsWith('.log.gz'))) {
  const input = gunzipSync(readFileSync(resolve(root, file)), {
    maxOutputLength: 128 * 1024 * 1024,
  });
  const result = spawnSync(process.execPath, [cli, '--stdinFileName', file.slice(0, -3)], {
    cwd: root,
    input,
    stdio: ['pipe', 'inherit', 'inherit'],
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}
console.log(`Secrets scan: ${files.length} tracked files checked`);
