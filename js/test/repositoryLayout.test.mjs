import assert from 'node:assert/strict';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';

const repoRoot = dirname(dirname(dirname(fileURLToPath(import.meta.url))));

function getIndentedBlock(text, header) {
  const lines = text.split('\n');
  const start = lines.findIndex((line) => line === header);
  assert.notEqual(start, -1, `${header} should exist`);

  const indent = header.search(/\S/);
  const block = [];
  for (let index = start + 1; index < lines.length; index += 1) {
    const line = lines[index];
    if (line.trim() && line.search(/\S/) <= indent) {
      break;
    }
    block.push(line);
  }

  return block.join('\n');
}

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

test('WebAssembly workflow deploys GitHub Pages automatically on push to main', () => {
  const workflow = readFileSync(join(repoRoot, '.github/workflows/wasm.yml'), 'utf8');

  assert.match(workflow, /name: Deploy GitHub Pages/);
  assert.match(workflow, /uses: actions\/deploy-pages@/);
  assert.match(workflow, /uses: actions\/upload-pages-artifact@/);
  assert.match(workflow, /uses: actions\/configure-pages@/);
  assert.match(workflow, /pages: write/);
  assert.match(workflow, /id-token: write/);
  assert.match(
    workflow,
    /github\.event_name == 'push'[\s\S]*?github\.ref == 'refs\/heads\/main'/
  );
  assert.doesNotMatch(workflow, /inputs\.deploy_pages/);
});

test('CSharp release workflow attaches NuGet packages to GitHub Releases', () => {
  const workflow = readFileSync(join(repoRoot, '.github/workflows/csharp.yml'), 'utf8');

  const matches = workflow.match(/--assets-glob "csharp\/artifacts\/\*\.nupkg"/g) ?? [];
  assert.ok(
    matches.length >= 2,
    `expected --assets-glob in both release jobs, found ${matches.length}`
  );
});

test('CSharp and Rust release workflows are scheduled on every push to main', () => {
  for (const [name, workflowPath] of [
    ['CSharp', '.github/workflows/csharp.yml'],
    ['Rust', '.github/workflows/rust.yml'],
  ]) {
    const workflow = readFileSync(join(repoRoot, workflowPath), 'utf8');
    const pushBlock = getIndentedBlock(workflow, '  push:');

    assert.match(pushBlock, /branches:\n\s+- main/);
    assert.doesNotMatch(
      pushBlock,
      /^\s+paths:/m,
      `${name} release workflow push trigger must not be path-filtered`
    );
  }
});

test('GitHub workflows avoid Node 20 action major versions that are being retired', () => {
  for (const workflowPath of [
    '.github/workflows/csharp.yml',
    '.github/workflows/rust.yml',
    '.github/workflows/wasm.yml',
  ]) {
    const workflow = readFileSync(join(repoRoot, workflowPath), 'utf8');

    assert.doesNotMatch(workflow, /actions\/[^@\s]+@v4/);
    assert.doesNotMatch(workflow, /codecov\/codecov-action@v4/);
  }
});
