import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const repoRoot = dirname(dirname(dirname(fileURLToPath(import.meta.url))));

test('language code and automation stay out of root-level src tests and scripts folders', () => {
  for (const entry of ['Cargo.toml', 'Cargo.lock', 'scripts', 'src', 'tests']) {
    assert.equal(
      existsSync(join(repoRoot, entry)),
      false,
      `${entry} should live under the language-specific project tree`
    );
  }

  for (const entry of [
    'csharp/scripts',
    'rust/scripts',
    'rust/wasm/Cargo.toml',
    'rust/wasm/src',
    'rust/wasm/tests',
  ]) {
    assert.equal(existsSync(join(repoRoot, entry)), true, `${entry} should exist`);
  }
});

test('web package scripts target the relocated WebAssembly crate and split script trees', () => {
  const packageJson = JSON.parse(
    readFileSync(join(repoRoot, 'package.json'), 'utf8')
  );

  assert.match(
    packageJson.scripts['build:wasm'],
    /^wasm-pack build --target web --out-dir \.\.\/\.\.\/web\/pkg rust\/wasm$/
  );
  assert.match(packageJson.scripts['test:wasm'], /^wasm-pack test --node rust\/wasm$/);
  assert.doesNotMatch(packageJson.scripts['test:wasm'], /test rust\/wasm --node/);
  assert.match(packageJson.scripts['test:js'], /csharp\/scripts\/\*\.test\.mjs/);
  assert.doesNotMatch(
    packageJson.scripts['test:js'],
    /(^|\s)scripts\/\*\.test\.mjs/
  );
});
