#!/usr/bin/env node
// Generates the first parity/golden-fixtures.json (issue #111): every shared
// docs/protocol fixture with the C# and Rust tests that already read it.
// Afterwards `node parity/check-parity.mjs --update` refreshes the hashes.
import { execFileSync } from 'node:child_process';
import { writeFileSync } from 'node:fs';

const git = (...args) =>
  execFileSync('git', args, { encoding: 'utf8' }).split('\n').filter(Boolean);
const fixtures = git('ls-files', 'docs/protocol').filter((file) => !file.endsWith('.md'));
const consumersOf = (fixture, paths, exclude = /\.csproj$/) => {
  const name = fixture.split('/').pop();
  try {
    return git('grep', '-l', '-F', name, '--', ...paths).filter((file) => !exclude.test(file));
  } catch {
    return [];
  }
};
const manifest = {
  $comment:
    'Golden fixtures shared by the C# and Rust suites (issue #111). Both suites load every entry in their GoldenFixtures test and write a receipt that CI checks with `node parity/check-parity.mjs --receipt <suite> <file>`.',
  schema: 1,
  suites: ['csharp', 'rust'],
  include: ['docs/protocol/**/*.{json,txt,gz}'],
  fixtures: fixtures.map((path) => {
    const consumers = {
      csharp: consumersOf(path, ['csharp/Foundation.Data.Doublets.Cli.Tests']),
      rust: consumersOf(path, ['rust/tests', 'rust/src']),
    };
    const examples = consumersOf(path, ['examples/*.sh', 'examples/**/*.sh']);
    if (examples.length) consumers.examples = examples;
    return { path, sha256: '', consumers };
  }),
};
writeFileSync('parity/golden-fixtures.json', `${JSON.stringify(manifest, null, 2)}\n`);
