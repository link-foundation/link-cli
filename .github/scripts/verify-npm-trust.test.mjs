import assert from 'node:assert/strict';
import test from 'node:test';
import { verifyNpmTrust } from './verify-npm-trust.mjs';

test('npm trust verification exchanges OIDC credentials without publishing or using a secret', async () => {
  const requests = [];
  await verifyNpmTrust('@link-foundation/link-cli-client', {
    env: { ACTIONS_ID_TOKEN_REQUEST_URL: 'https://example.test/oidc?existing=value', ACTIONS_ID_TOKEN_REQUEST_TOKEN: 'runner-token' },
    fetch: async (url, options) => {
      requests.push({ url: String(url), ...options });
      return Response.json(requests.length === 1 ? { value: 'oidc-token' } : { token: 'temporary-npm-token' });
    },
  });
  assert.equal(requests.length, 2);
  const oidc = new URL(requests[0].url);
  assert.equal(oidc.searchParams.get('audience'), 'npm:registry.npmjs.org');
  assert.equal(oidc.searchParams.get('existing'), 'value');
  assert.equal(requests[0].headers.Authorization, 'Bearer runner-token');
  assert.equal(requests[1].url, 'https://registry.npmjs.org/-/npm/v1/oidc/token/exchange/package/%40link-foundation%2Flink-cli-client');
  assert.equal(requests[1].method, 'POST');
  assert.equal(requests[1].headers.Authorization, 'Bearer oidc-token');
});

test('npm trust verification fails clearly when OIDC or the registry policy is missing', async () => {
  await assert.rejects(verifyNpmTrust('package', { env: {} }), /id-token: write/);
  const env = { ACTIONS_ID_TOKEN_REQUEST_URL: 'https://example.test/oidc', ACTIONS_ID_TOKEN_REQUEST_TOKEN: 'runner' };
  for (const [response, error] of [
    [new Response('', { status: 403 }), /OIDC.*403/],
    [Response.json({}), /OIDC.*token/],
  ]) {
    await assert.rejects(verifyNpmTrust('package', { env, fetch: async () => response }), error);
  }
  for (const response of [new Response('', { status: 404 }), Response.json({})]) {
    let calls = 0;
    await assert.rejects(verifyNpmTrust('package', { env, fetch: async () => ++calls === 1 ? Response.json({ value: 'oidc' }) : response }), /npm.*trusted publisher/);
  }
});
