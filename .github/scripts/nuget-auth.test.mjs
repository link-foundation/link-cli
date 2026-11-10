import assert from 'node:assert/strict';
import test from 'node:test';
import {
  checkNugetAuthentication,
  describeVerificationDenial,
  selectNugetCredential,
} from './nuget-auth.mjs';

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
    assert.equal(result.status, status === 500 ? 'unknown' : 'failed');
    assert.ok(!JSON.stringify(result).includes('provided'));
    assert.ok(!result.detail.includes('PackageVerify'), 'push-scoped endpoint, not verify scope');
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

test('a 403 is a key without push scope and a 401 is an unaccepted key (issue #172)', async () => {
  const probe = (status, headers = new Headers()) =>
    checkNugetAuthentication({
      apiKey: 'provided',
      packageId: 'clink',
      fetchImpl: async () => ({ ok: false, status, headers }),
    });
  const denied = await probe(403);
  assert.equal(denied.status, 'failed');
  assert.equal(denied.authenticated, true);
  assert.equal(denied.authorized, false);
  assert.match(denied.detail, /HTTP 403: .*no push scope/);
  const rejected = await probe(401, new Headers({ 'X-NuGet-Warning': 'The API key has expired.' }));
  assert.equal(rejected.authenticated, false);
  assert.match(
    rejected.detail,
    /invalid, expired or revoked.*NuGet warning: The API key has expired\./,
  );
  assert.equal(describeVerificationDenial(502).result.status, 'unknown');
});
