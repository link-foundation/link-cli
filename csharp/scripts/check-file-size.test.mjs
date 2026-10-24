import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import test from 'node:test';

import {
  LANGUAGE_CONFIG,
  MAX_LINES,
  checkLanguage,
  countLines,
} from './check-file-size.mjs';

const script = new URL('./check-file-size.mjs', import.meta.url).pathname;

function writeTree(root, files) {
  for (const [path, content] of Object.entries(files)) {
    mkdirSync(join(root, path, '..'), { recursive: true });
    writeFileSync(join(root, path), content);
  }
}

test('exclusions match whole path components, not substrings', () => {
  const root = mkdtempSync(join(tmpdir(), 'check-file-size-'));
  const tooLong = 'x\n'.repeat(MAX_LINES + 1);
  writeTree(root, {
    // Substring matches of `bin` and `target` that used to be skipped.
    'rust/examples/lino_binary_dump.rs': tooLong,
    'rust/src/sequences/target_matcher.rs': tooLong,
    // Real build output and evidence directories stay excluded.
    'rust/target/debug/build.rs': tooLong,
    'rust/wasm/node_modules/pkg/index.rs': tooLong,
    'csharp/App/bin/Debug/Generated.cs': tooLong,
    'csharp/App/obj/Generated.cs': tooLong,
    'csharp/App/Robin.cs': tooLong,
    'dev/log/rust/copy.rs': tooLong,
  });

  const rust = checkLanguage(root, LANGUAGE_CONFIG.rust);
  assert.deepEqual(rust.violations.map((v) => v.file).sort(), [
    join('rust', 'examples', 'lino_binary_dump.rs'),
    join('rust', 'src', 'sequences', 'target_matcher.rs'),
  ]);
  const csharp = checkLanguage(root, LANGUAGE_CONFIG.csharp);
  assert.deepEqual(
    csharp.violations.map((v) => v.file),
    [join('csharp', 'App', 'Robin.cs')]
  );
});

test('a trailing newline does not count as an extra line', () => {
  const root = mkdtempSync(join(tmpdir(), 'check-file-size-'));
  writeTree(root, {
    'exact.rs': 'x\n'.repeat(MAX_LINES),
    'unterminated.rs': 'a\nb',
    'empty.rs': '',
  });
  // Agrees with `str::lines().count()` of rust/scripts/check-file-size.rs.
  assert.equal(countLines(join(root, 'exact.rs')), MAX_LINES);
  assert.equal(countLines(join(root, 'unterminated.rs')), 2);
  assert.equal(countLines(join(root, 'empty.rs')), 0);
});

test('CLI fails when a file exceeds the limit and passes otherwise', () => {
  const root = mkdtempSync(join(tmpdir(), 'check-file-size-'));
  writeTree(root, { 'rust/src/ok.rs': 'x\n'.repeat(MAX_LINES) });
  const output = execFileSync('node', [script, '--dir', root], {
    encoding: 'utf8',
  });
  assert.match(output, /Checked 1 file\(s\) - all within the line limit/);

  writeTree(root, { 'rust/src/sequences/target_matcher.rs': 'x\n'.repeat(MAX_LINES + 1) });
  assert.throws(
    () => execFileSync('node', [script, '--dir', root], { encoding: 'utf8' }),
    (error) => error.status === 1 && /target_matcher\.rs: 1001 lines/.test(error.stdout)
  );
});
