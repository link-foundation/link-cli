// The two entry points of the package: `src/index.js` (the `exports` of
// package.json) and the `clink` executable (`bin` of package.json).
import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import * as api from '../src/index.js';
import { Cli } from '../src/cli.js';

test('the package entry point exports every name it lists', () => {
  const listed = [
    ...readFileSync(new URL('../src/index.js', import.meta.url), 'utf8').matchAll(
      /export \{([^}]*)\}/g,
    ),
  ].flatMap((match) =>
    match[1]
      .split(',')
      .map((name) =>
        name
          .trim()
          .split(/\s+as\s+/)
          .at(-1),
      )
      .filter(Boolean),
  );
  assert.ok(listed.length > 40, `only ${listed.length} exports`);
  for (const name of listed) assert.notEqual(api[name], undefined, name);
  assert.deepEqual(Object.keys(api).sort(), [...new Set(listed)].sort());
});

test('the package entry point opens an embedded store and queries it', () => {
  const dir = mkdtempSync(join(tmpdir(), 'clink-entry-'));
  try {
    const path = join(dir, 'db.links');
    const storage = api.NamedTypesDecorator.open(path);
    const changes = new api.QueryProcessor(false).processQuery(storage, '() ((1 1) (1 2))');
    assert.equal(changes.length, 2);
    storage.save();
    const reopened = api.NamedTypesDecorator.open(path);
    assert.deepEqual(
      reopened.allLinks().map((link) => [link.index, link.source, link.target]),
      [
        [1, 1, 1],
        [2, 1, 2],
      ],
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('bin/clink.js runs main with the command line and sets the exit status', async () => {
  const argv = process.argv;
  const write = process.stdout.write;
  const exitCode = process.exitCode;
  const written = [];
  process.argv = [argv[0], 'clink', '--version'];
  process.stdout.write = (text) => written.push(String(text)) > 0;
  try {
    await import('../bin/clink.js');
    assert.equal(process.exitCode, 0);
  } finally {
    process.argv = argv;
    process.stdout.write = write;
    process.exitCode = exitCode;
  }
  assert.equal(written.join(''), `${Cli.versionText()}\n`);
});
