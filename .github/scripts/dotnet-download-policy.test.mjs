// The 10.0.x aka.ms redirect selected a private dotnetorigin storage account
// (HTTP 409) on 2026-10-25. Exact SDK versions use the public builds feed.
import assert from 'node:assert/strict';
import { readFileSync, readdirSync } from 'node:fs';
import test from 'node:test';

const directory = new URL('../workflows/', import.meta.url);
for (const name of readdirSync(directory).filter((name) => /\.ya?ml$/.test(name))) {
  const text = readFileSync(new URL(name, directory), 'utf8');
  if (!text.includes('actions/setup-dotnet@')) continue;
  test(`${name}: use an exact SDK version to avoid the broken channel redirect`, () => {
    const versions = [...text.matchAll(/^\s*dotnet-version:\s*['"]?([\w.]+)/gm)];
    assert.ok(versions.length > 0, 'setup-dotnet must select an explicit SDK');
    for (const [, version] of versions) {
      assert.match(version, /^\d+\.\d+\.\d+$/, 'channel installs select the inaccessible aka.ms origin');
    }
  });
}
