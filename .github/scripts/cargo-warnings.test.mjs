import assert from 'node:assert/strict';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import test from 'node:test';

test('Cargo manifest warnings fail even when cargo exits zero; compiler errors retain their exit status', { timeout: 10000 }, () => {
  const dir = mkdtempSync(join(tmpdir(), 'cargo-warning-test-'));
  try {
    writeFileSync(join(dir, 'cargo'), '#!/usr/bin/env bash\nprintf "%s\\n" "$FAKE_CARGO_OUTPUT" >&2\nexit "$FAKE_CARGO_STATUS"\n', { mode: 0o755 });
    for (const [output, status, expected] of [
      ['Finished dev profile', 0, 0],
      ['warning: unused manifest key: package.foo', 0, 1],
      ['warning[unused]: variable', 0, 1],
      ['error: compilation failed', 7, 7],
    ]) {
      const result = spawnSync('bash', [new URL('./check-cargo-warnings.sh', import.meta.url).pathname], {
        encoding: 'utf8', env: { ...process.env, PATH: `${dir}:${process.env.PATH}`,
          CARGO_WARNING_LOG: join(dir, 'output.log'), FAKE_CARGO_OUTPUT: output, FAKE_CARGO_STATUS: String(status) },
      });
      assert.equal(result.status, expected, result.stdout + result.stderr);
      assert.match(result.stdout, new RegExp(output.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
