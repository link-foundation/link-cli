import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const repoRoot = dirname(dirname(dirname(fileURLToPath(import.meta.url))));

test('language code package manifests and generated evidence stay out of the root folder', () => {
  for (const entry of [
    '.gitkeep',
    'Cargo.toml',
    'Cargo.lock',
    'README-WASM.md',
    'WEBASSEMBLY_IMPLEMENTATION.md',
    'ci-logs',
    'package-lock.json',
    'package.json',
    'scripts',
    'src',
    'tests',
    'web',
  ]) {
    assert.equal(
      existsSync(join(repoRoot, entry)),
      false,
      `${entry} should live under the package-specific project tree or case-study evidence`
    );
  }

  for (const entry of [
    'csharp/scripts',
    'docs/case-studies/issue-12/screenshots',
    'docs/case-studies/issue-79/evidence',
    'js/package-lock.json',
    'js/package.json',
    'js/README.md',
    'js/src',
    'js/test',
    'rust/scripts',
    'rust/wasm/Cargo.toml',
    'rust/wasm/src',
    'rust/wasm/tests',
  ]) {
    assert.equal(existsSync(join(repoRoot, entry)), true, `${entry} should exist`);
  }
});

test('JavaScript package scripts target the relocated WebAssembly crate and split script trees', () => {
  const packageJson = JSON.parse(
    readFileSync(join(repoRoot, 'js/package.json'), 'utf8')
  );

  assert.match(
    packageJson.scripts['build:wasm'],
    /^wasm-pack build --target web --out-dir \.\.\/\.\.\/js\/pkg \.\.\/rust\/wasm$/
  );
  assert.match(
    packageJson.scripts['test:wasm'],
    /^wasm-pack test --node \.\.\/rust\/wasm$/
  );
  assert.doesNotMatch(packageJson.scripts['test:wasm'], /test rust\/wasm --node/);
  assert.match(packageJson.scripts['test:js'], /test\/\*\.test\.mjs/);
  assert.match(packageJson.scripts['test:js'], /\.\.\/csharp\/scripts\/\*\.test\.mjs/);
  assert.doesNotMatch(
    packageJson.scripts['test:js'],
    /(^|\s)(web|scripts)\/\*\.test\.mjs/
  );
});

test('WebAssembly workflow uses the JavaScript package lockfile from js', () => {
  const workflow = readFileSync(join(repoRoot, '.github/workflows/wasm.yml'), 'utf8');

  assert.match(workflow, /js\/package-lock\.json/);
  assert.match(workflow, /cache-dependency-path:\s+js\/package-lock\.json/);
  assert.match(workflow, /working-directory:\s+js/);
  assert.doesNotMatch(workflow, /(^|\s)- 'package(-lock)?\.json'/);
  assert.doesNotMatch(workflow, /(^|\s)- 'web\/\*\*'/);
});
