import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

test('release helper check compiles every executable, denies warnings, and propagates failures', () => {
  const root = mkdtempSync(join(tmpdir(), 'release-scripts-136-'));
  try {
    copyFileSync(new URL('../../rust/scripts/check-scripts.sh', import.meta.url), join(root, 'check-scripts.sh'));
    writeFileSync(join(root, 'first.rs'), 'fn main() {}\n');
    writeFileSync(join(root, 'second.rs'), 'fn main() {}\n');
    writeFileSync(join(root, 'module.rs'), 'pub fn helper() {}\n');
    writeFileSync(join(root, 'rust-script'), '#!/usr/bin/env bash\n[[ "$1" == --package ]] || exit 1\nprintf "%s\\n" "$2"\n', { mode: 0o755 });
    writeFileSync(join(root, 'cargo'), '#!/usr/bin/env bash\nprintf "%s|%s\\n" "$*" "$RUSTFLAGS" >> "$INVOCATIONS"\n[[ "$*" != *"${FAIL_SCRIPT:-never-match}"* ]]\n', { mode: 0o755 });
    for (const fail of ['', 'first.rs']) {
      const invocations = join(root, `invocations-${fail}.txt`);
      const result = spawnSync('bash', [join(root, 'check-scripts.sh')], {
        env: { ...process.env, PATH: `${root}:${process.env.PATH}`, RUSTFLAGS: '-C debuginfo=0', INVOCATIONS: invocations, FAIL_SCRIPT: fail },
        encoding: 'utf8', timeout: 10_000,
      });
      assert.equal(result.status, fail ? 1 : 0, result.stdout + result.stderr);
      assert.deepEqual(readFileSync(invocations, 'utf8').trim().split('\n'), [
        'build --release --manifest-path first.rs/Cargo.toml|-C debuginfo=0 -D warnings',
        'build --release --manifest-path second.rs/Cargo.toml|-C debuginfo=0 -D warnings',
      ]);
      if (fail) assert.match(result.stdout, /::error file=rust\/scripts\/first.rs/);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
