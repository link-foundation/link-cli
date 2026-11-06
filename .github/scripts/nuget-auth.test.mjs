import assert from 'node:assert/strict';
import test from 'node:test';
import { checkNugetAuthentication, selectNugetCredential } from './nuget-auth.mjs';

test('NuGet username is configuration, OIDC is authentication and scope remains unverified', async () => {
  assert.equal((await checkNugetAuthentication({ nugetUser: 'publisher' })).status, 'failed');
  const result = await checkNugetAuthentication({
    oidcOutcome: 'success',
    oidcKey: 'temporary',
    apiKey: 'invalid',
    fetchImpl: async () => {
      throw Error('must not probe fallback');
    },
  });
  assert.equal(result.status, 'ok');
  assert.equal(result.authenticated, true);
  assert.equal(result.authorized, 'unknown');
  assert.ok(!JSON.stringify(result).includes('temporary'));
  assert.equal(
    selectNugetCredential({ oidcOutcome: 'failure', apiKey: 'provided' }).method,
    'token-fallback',
  );
});

test('fallback rejects expired, unavailable or malformed verification without exposing keys', async () => {
  for (const status of [401, 403, 500]) {
    const result = await checkNugetAuthentication({
      apiKey: 'provided',
      packageId: 'Example',
      fetchImpl: async () => ({ ok: false, status }),
    });
    assert.equal(result.status, status === 401 ? 'failed' : 'unknown');
    assert.ok(!JSON.stringify(result).includes('provided'));
  }
  const options = { apiKey: 'provided', packageId: 'Example' };
  assert.equal(
    (
      await checkNugetAuthentication({
        ...options,
        fetchImpl: async () => {
          throw Error('offline');
        },
      })
    ).status,
    'unknown',
  );
  assert.equal(
    (
      await checkNugetAuthentication({
        ...options,
        fetchImpl: async () => ({ ok: true, json: async () => ({}) }),
      })
    ).status,
    'unknown',
  );
  const valid = await checkNugetAuthentication({
    ...options,
    fetchImpl: async () => ({ ok: true, json: async () => ({ Key: 'derived-secret' }) }),
  });
  assert.equal(valid.status, 'ok');
  assert.equal(valid.authorized, 'unknown');
  assert.ok(!JSON.stringify(valid).includes('derived-secret'));
});
