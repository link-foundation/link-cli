// Issue #172: only `.log.gz` evidence was decompressed before the secrets
// scan; every other archive - including whole template snapshots in .tar.gz -
// reached secretlint as compressed bytes and was never really scanned.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { gzipSync } from 'node:zlib';

import {
  asOperand,
  gunzip,
  isCompressed,
  isTarball,
  readTarEntries,
} from './check-secrets.mjs';

const script = new URL('./check-secrets.mjs', import.meta.url).pathname;
const secretlint = new URL('../../js/node_modules/secretlint/bin/secretlint.js', import.meta.url)
  .pathname;
// A GitHub token pattern the recommended preset flags, assembled at runtime
// so this file itself does not contain one.
const leakedToken = `ghp_${'A'.repeat(36)}`;

function tarball(files) {
  const dir = mkdtempSync(join(tmpdir(), 'check-secrets-test-'));
  for (const [name, content] of Object.entries(files)) {
    mkdirSync(join(dir, name, '..'), { recursive: true });
    writeFileSync(join(dir, name), content);
  }
  const archive = execFileSync('tar', ['-cz', '-C', dir, '.']);
  rmSync(dir, { recursive: true, force: true });
  return archive;
}

test('every gzip archive is decompressed and tarballs are unpacked', () => {
  assert.ok(isCompressed('research/codeql-tree.json.gz'));
  assert.ok(isCompressed('ci-logs/run.log.GZ'));
  assert.ok(!isCompressed('notes.gzip.md'));
  assert.ok(isTarball('templates/rust/snapshot.tar.gz'));
  assert.ok(!isTarball('research/codeql-tree.json.gz'));
  assert.equal(asOperand('-rf'), './-rf');
  assert.equal(asOperand('/tmp/x'), '/tmp/x');

  const longName = `${'nested/'.repeat(20)}deep.txt`;
  const entries = readTarEntries(
    gunzip(tarball({ 'a.txt': 'alpha', [longName]: 'deep', 'empty.txt': '' })),
  );
  assert.deepEqual(
    entries.map(({ name, content }) => [name.replace(/^\.\//, ''), content.toString()]).sort(),
    [
      ['a.txt', 'alpha'],
      ['empty.txt', ''],
      [longName, 'deep'],
    ],
  );
  assert.throws(() => readTarEntries(gunzip(tarball({ 'a.txt': 'alpha' })).subarray(0, 600)));
  assert.throws(() => gunzip(Buffer.from('not gzip')));
});

test('a secret inside a compressed archive fails the scan', {
  skip: !existsSync(secretlint) && 'secretlint is not installed (npm ci --prefix js)',
}, () => {
  const dir = mkdtempSync(join(tmpdir(), 'check-secrets-test-'));
  try {
    const scan = (name, content) => {
      writeFileSync(join(dir, name), content);
      return spawnSync(process.execPath, [script, join(dir, name)], { encoding: 'utf8' });
    };
    assert.equal(scan('clean.tar.gz', tarball({ 'a.txt': 'nothing here' })).status, 0);
    assert.notEqual(scan('leak.tar.gz', tarball({ 'docs/key.pem': leakedToken })).status, 0);
    assert.notEqual(scan('leak.json.gz', gzipSync(leakedToken)).status, 0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
