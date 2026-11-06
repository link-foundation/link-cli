import assert from 'node:assert/strict';
import test from 'node:test';
import { preflight } from './release-preflight.mjs';

test('PRs and metadata dispatches are advisory without touching credentials or registries', async () => {
  const forbidden = () => {
    throw Error('unexpected authentication');
  };
  for (const registry of ['crates', 'nuget', 'npm']) {
    assert.match(
      await preflight(registry, {
        env: { GITHUB_EVENT_NAME: 'pull_request' },
        crates: forbidden,
        nuget: forbidden,
        npm: forbidden,
      }),
      /PR advisory/,
    );
    assert.match(
      await preflight(registry, {
        env: { RELEASE_MODE: 'changeset-pr' },
        crates: forbidden,
        nuget: forbidden,
        npm: forbidden,
      }),
      /metadata/,
    );
  }
});

test('main fails on denied or unknown authentication and validates both NuGet packages', async () => {
  for (const verdict of ['unknown', 'denied'])
    await assert.rejects(
      preflight('crates', {
        env: {},
        crates: async () => ({ verdict, message: 'unverified' }),
      }),
      /CARGO_TOKEN/,
    );
  for (const status of ['failed', 'unknown'])
    await assert.rejects(
      preflight('nuget', {
        env: {},
        nuget: async () => ({ status, detail: 'unverified' }),
      }),
      /NUGET_USER/,
    );
  const packages = [];
  await preflight('nuget', {
    env: {},
    nuget: async ({ packageId }) => {
      packages.push(packageId);
      return { status: 'ok' };
    },
  });
  assert.deepEqual(packages, ['clink', 'Foundation.Data.Doublets.Cli']);
  await assert.rejects(
    preflight('npm', {
      env: {},
      npm: async () => {
        throw Error('policy missing');
      },
    }),
    /policy missing/,
  );
});
