import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { promisify } from 'node:util';
import { decide } from './check-release-needed.mjs';
import { repoRoot } from './verify-packages.mjs';

for (const missing of ['clink', 'foundation.data.doublets.cli']) {
  test(`release resumes when only ${missing} is missing`, { timeout: 10000 }, async (t) => {
    const dir = mkdtempSync(join(tmpdir(), 'link-cli-release-pair-'));
    t.after(() => rmSync(dir, { recursive: true, force: true }));
    const server = createServer((req, res) => {
      res.setHeader('connection', 'close');
      const versions = req.url.includes(`/${missing}/`) ? ['4.0.6'] : ['5.1.0'];
      res.writeHead(200, { 'content-type': 'application/json' });
      res.end(JSON.stringify({ versions }));
    });
    await new Promise((done) => server.listen(0, '127.0.0.1', done));
    t.after(() => new Promise((done) => server.close(done)));
    for (const id of ['clink', 'Foundation.Data.Doublets.Cli']) {
      writeFileSync(
        join(dir, `${id}.csproj`),
        `<Project><PropertyGroup><PackageId>${id}</PackageId><Version>5.1.0</Version></PropertyGroup></Project>`,
      );
    }
    const url = `http://127.0.0.1:${server.address().port}`;
    const output = join(dir, 'output');
    await promisify(execFile)(
      process.execPath,
      [
        join(repoRoot, 'csharp/scripts/check-release-needed.mjs'),
        '--csproj',
        join(dir, 'clink.csproj'),
        '--library-csproj',
        join(dir, 'Foundation.Data.Doublets.Cli.csproj'),
        '--repository',
        'link-foundation/link-cli',
      ],
      {
        env: {
          ...process.env,
          HAS_CHANGESETS: 'false',
          GITHUB_OUTPUT: output,
          NUGET_INDEX_URL: url,
          GITHUB_API_URL: url,
        },
      },
    );
    const result = readFileSync(output, 'utf8');
    assert.match(result, /^should_release=true$/m);
    assert.match(result, /^skip_bump=true$/m);
    assert.match(result, /^nuget_published=false$/m);
    assert.match(result, /^current_version=5\.1\.0$/m);
  });
}

// Pending release metadata must not skip the explicitly requested 5.1.0.
test('unpublished current version is recovered before consuming new changesets', () => {
  const result = decide({
    hasChangesets: true,
    currentVersion: '5.1.0',
    publishedVersions: ['4.0.6'],
    githubReleaseExists: false,
  });
  assert.equal(result.shouldRelease, true);
  assert.equal(result.skipBump, true);
});
