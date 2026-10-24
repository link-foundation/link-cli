import { describe, expect, test } from 'bun:test';
import { checkNugetAuthentication } from './nuget-auth.mjs';

function gallery(statuses = [200, 200]) {
  const calls = [];
  const fetchImpl = async (url, options) => {
    calls.push({ url, ...options });
    const status = statuses[calls.length - 1];
    return { status, ok: status === 200, json: async () => ({ Key: 'one-time-secret' }),
      body: { cancel: async () => {} } };
  };
  return { calls, fetchImpl };
}

describe('NuGet publishing authentication', () => {
  test('trusted publishing takes precedence over an expired API key without using it', async () => {
    const { calls, fetchImpl } = gallery([403]);
    const result = await checkNugetAuthentication({ nugetUser: 'publisher', apiKey: 'expired', fetchImpl });
    expect(result).toMatchObject({ status: 'ok', auth: 'trusted-publishing' });
    expect(result.detail).toContain('policy');
    expect(calls).toHaveLength(0);
  });

  test('neither publishing credential fails with setup instructions', async () => {
    const result = await checkNugetAuthentication({});
    expect(result).toMatchObject({ status: 'failed', auth: 'none' });
    expect(result.detail).toContain('NUGET_USER');
    expect(result.detail).toContain('NUGET_API_KEY');
  });

  test('verifies key validity then package owner/glob using the one-time key', async () => {
    const { calls, fetchImpl } = gallery();
    const result = await checkNugetAuthentication({
      apiKey: 'long-lived-secret', packageId: 'My.Package', publishedVersion: '1.0.0', fetchImpl,
    });
    expect(result).toMatchObject({ status: 'ok', auth: 'api-key' });
    expect(calls.map(({ url }) => url)).toEqual([
      'https://www.nuget.org/api/v2/package/create-verification-key/My.Package',
      'https://www.nuget.org/api/v2/verifykey/My.Package/1.0.0',
    ]);
    expect(calls[0].method).toBe('POST');
    expect(calls[0].headers).toMatchObject({ 'X-NuGet-ApiKey': 'long-lived-secret', 'Content-Length': '0' });
    expect(calls[0].redirect).toBe('error');
    expect(calls[1].method).toBe('GET');
    expect(calls[1].headers['X-NuGet-ApiKey']).toBe('one-time-secret');
    expect(JSON.stringify(result)).not.toContain('secret');
  });

  for (const status of [401, 403]) {
    test(`expired/revoked key (${status}) fails before publish`, async () => {
      const { calls, fetchImpl } = gallery([status]);
      const result = await checkNugetAuthentication({ apiKey: 'expired', packageId: 'MyPackage', fetchImpl });
      expect(result.status).toBe('failed');
      expect(result.detail).toContain('NUGET_USER');
      expect(calls).toHaveLength(1);
    });
  }

  test('a valid key with the wrong package scope fails the second check', async () => {
    const { fetchImpl } = gallery([200, 403]);
    const result = await checkNugetAuthentication({
      apiKey: 'wrong-scope', packageId: 'MyPackage', publishedVersion: '1.0.0', fetchImpl,
    });
    expect(result.status).toBe('failed');
    expect(result.detail).toMatch(/owner|scope|glob/);
  });

  test('first publication verifies validity and explicitly reports the scope limitation', async () => {
    const { calls, fetchImpl } = gallery();
    const result = await checkNugetAuthentication({ apiKey: 'valid', packageId: 'NewPackage', fetchImpl });
    expect(result.status).toBe('ok');
    expect(result.detail).toContain('first publish');
    expect(calls).toHaveLength(1);
  });

  test('an unreachable gallery never verifies a key or exposes an exception containing it', async () => {
    const result = await checkNugetAuthentication({
      apiKey: 'private-secret', packageId: 'MyPackage',
      fetchImpl: async () => { throw new Error('private-secret'); },
    });
    expect(result.status).toBe('unknown');
    expect(result.detail).not.toContain('private-secret');
  });

  test('malformed verification response never passes', async () => {
    const result = await checkNugetAuthentication({
      apiKey: 'valid', packageId: 'MyPackage',
      fetchImpl: async () => ({ status: 200, ok: true, json: async () => ({}) }),
    });
    expect(result.status).toBe('unknown');
  });
});
