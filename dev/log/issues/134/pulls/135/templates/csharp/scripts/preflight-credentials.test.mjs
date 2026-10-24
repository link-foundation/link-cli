import { describe, expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

import {
  checkGithubPushPermission,
  checkNugetPackageVisibility,
  decidePreflight,
  main,
} from './preflight-credentials.mjs';

/**
 * Build a fetch stub from a status/body table keyed by "METHOD url".
 * Calls are recorded so tests can assert the probe went to the right
 * endpoint with the right headers — the whole point of the preflight is
 * that the probe is real, so the test pins what "real" means.
 * @param {Record<string, { status: number, body?: unknown }>} routes
 * @returns {{ fetchImpl: typeof fetch, calls: Array<{ method: string, url: string, headers: Record<string, string> }> }}
 */
function stubFetch(routes) {
  const calls = [];
  const fetchImpl = async (url, init = {}) => {
    const method = init.method ?? 'GET';
    calls.push({ method, url, headers: init.headers ?? {} });
    const route = routes[`${method} ${url}`];
    if (!route) {
      return { ok: false, status: 500, json: async () => ({}) };
    }
    return {
      ok: route.status >= 200 && route.status < 300,
      status: route.status,
      json: async () => route.body,
    };
  };
  return { fetchImpl, calls };
}

describe('GitHub push permission probe', () => {
  test('verifies push permission from the repository API', async () => {
    const { fetchImpl, calls } = stubFetch({
      'GET https://api.github.com/repos/owner/repo': {
        status: 200,
        body: { permissions: { push: true } },
      },
    });

    const result = await checkGithubPushPermission({
      repository: 'owner/repo',
      token: 'secret-token',
      fetchImpl,
    });

    expect(result.status).toBe('ok');
    expect(result.detail).toMatch(/can push/);
    expect(calls).toHaveLength(1);
    expect(calls[0].method).toBe('GET');
    expect(calls[0].url).toBe('https://api.github.com/repos/owner/repo');
    expect(calls[0].headers.Authorization).toBe('Bearer secret-token');
    expect(calls[0].headers.Accept).toBe('application/vnd.github+json');
  });

  test('fails loud when the token may not push', async () => {
    const { fetchImpl } = stubFetch({
      'GET https://api.github.com/repos/owner/repo': {
        status: 200,
        body: { permissions: { push: false } },
      },
    });

    const result = await checkGithubPushPermission({
      repository: 'owner/repo',
      token: 'secret-token',
      fetchImpl,
    });

    expect(result.status).toBe('failed');
    expect(result.detail).toMatch(/Workflow permissions/);
  });

  test('treats an anonymous probe honestly: no token, no push', async () => {
    const { fetchImpl } = stubFetch({
      'GET https://api.github.com/repos/owner/repo': {
        status: 200,
        body: { permissions: {} },
      },
    });

    const result = await checkGithubPushPermission({
      repository: 'owner/repo',
      token: '',
      fetchImpl,
    });

    expect(result.status).toBe('failed');
  });

  test('reports rate limiting as unknown, never as a pass', async () => {
    const { fetchImpl } = stubFetch({
      'GET https://api.github.com/repos/owner/repo': { status: 403 },
    });

    const result = await checkGithubPushPermission({
      repository: 'owner/repo',
      token: 'secret-token',
      fetchImpl,
    });

    expect(result.status).toBe('unknown');
  });

  test('network failure is unknown, never a pass', async () => {
    const fetchImpl = async () => {
      throw new Error('connection reset');
    };

    const result = await checkGithubPushPermission({
      repository: 'owner/repo',
      token: 'secret-token',
      fetchImpl,
    });

    expect(result.status).toBe('unknown');
    expect(result.detail).toMatch(/connection reset/);
  });

  test('a missing repository cannot be probed', async () => {
    const { fetchImpl, calls } = stubFetch({});

    const result = await checkGithubPushPermission({
      repository: '',
      token: 'secret-token',
      fetchImpl,
    });

    expect(result.status).toBe('unknown');
    expect(calls).toHaveLength(0);
  });
});

describe('NuGet package visibility probe', () => {
  test('a published package is reported with its version count', async () => {
    const { fetchImpl, calls } = stubFetch({
      'GET https://api.nuget.org/v3-flatcontainer/mypackage/index.json': {
        status: 200,
        body: { versions: ['1.0.0', '1.1.0'] },
      },
    });

    const result = await checkNugetPackageVisibility({
      packageId: 'MyPackage',
      fetchImpl,
    });

    expect(result.status).toBe('ok');
    expect(result.detail).toMatch(/2 published version/);
    expect(result.latestVersion).toBe('1.1.0');
    expect(calls[0].url).toContain('/mypackage/index.json');
  });

  test('a 404 is the expected pre-first-publish state, not a failure', async () => {
    const { fetchImpl } = stubFetch({
      'GET https://api.nuget.org/v3-flatcontainer/mypackage/index.json': {
        status: 404,
      },
    });

    const result = await checkNugetPackageVisibility({
      packageId: 'MyPackage',
      fetchImpl,
    });

    expect(result.status).toBe('ok');
    expect(result.detail).toMatch(/first publish creates/);
  });

  test('an unreachable registry is unknown; the probe is advisory', async () => {
    const fetchImpl = async () => {
      throw new Error('ECONNREFUSED');
    };

    const result = await checkNugetPackageVisibility({
      packageId: 'MyPackage',
      fetchImpl,
    });

    expect(result.status).toBe('unknown');
  });
});

describe('preflight authentication integration', () => {
  for (const [nugetUser, apiKey, mode, expected] of [
    ['publisher', '', 'release', true],
    ['', 'expired', 'release', false],
    ['', '', 'report', true],
  ]) {
    test(`${mode} mode with auth user=${nugetUser || '(none)'} key=${apiKey || '(none)'}`, async () => {
      const directory = mkdtempSync(join(tmpdir(), 'preflight-test-'));
      const outputFile = join(directory, 'outputs');
      const overrides = {
        NUGET_USER: nugetUser, NUGET_API_KEY: apiKey, PREFLIGHT_MODE: mode,
        GITHUB_REPOSITORY: 'owner/repo', GITHUB_TOKEN: 'token',
        GITHUB_OUTPUT: outputFile, GITHUB_STEP_SUMMARY: join(directory, 'summary'),
      };
      const saved = Object.fromEntries(Object.keys(overrides).map((key) => [key, process.env[key]]));
      try {
        Object.assign(process.env, overrides);
        const { fetchImpl, calls } = stubFetch({
          'GET https://api.github.com/repos/owner/repo': { status: 200, body: { permissions: { push: true } } },
          'GET https://api.nuget.org/v3-flatcontainer/mypackage/index.json': { status: 200, body: { versions: ['0.3.16'] } },
          'POST https://www.nuget.org/api/v2/package/create-verification-key/MyPackage': { status: 403 },
        });
        const decision = await main({ fetchImpl });
        expect(decision.passed).toBe(expected);
        expect(readFileSync(outputFile, 'utf-8')).toContain(`preflight_result=${expected ? 'passed' : 'failed'}`);
        expect(readFileSync(outputFile, 'utf-8')).toContain(`auth=${nugetUser ? 'trusted-publishing' : apiKey ? 'api-key' : 'none'}`);
        if (nugetUser) expect(calls.some((call) => call.method === 'POST')).toBe(false);
      } finally {
        for (const [key, value] of Object.entries(saved)) {
          if (value === undefined) delete process.env[key];
          else process.env[key] = value;
        }
        rmSync(directory, { recursive: true, force: true });
      }
    });
  }
});

describe('preflight decision', () => {
  const passingChecks = [
    { name: 'push', required: true, status: 'ok', detail: '' },
    { name: 'key', required: true, status: 'ok', detail: '' },
    { name: 'visibility', required: false, status: 'ok', detail: '' },
  ];

  test('release mode passes only when every required probe verified', () => {
    expect(decidePreflight({ mode: 'release', checks: passingChecks }).passed).toBe(true);
  });

  test('release mode fails on any failed probe and names it', () => {
    const checks = [
      passingChecks[0],
      { name: 'key', required: true, status: 'failed', detail: 'NUGET_API_KEY is not configured.' },
      passingChecks[2],
    ];
    const decision = decidePreflight({ mode: 'release', checks });

    expect(decision.passed).toBe(false);
    expect(decision.failures).toHaveLength(1);
    expect(decision.failures[0]).toContain('NUGET_API_KEY is not configured');
  });

  test('release mode treats an unverified required probe as a failure', () => {
    const checks = [
      { name: 'push', required: true, status: 'unknown', detail: 'rate limited' },
      passingChecks[1],
      passingChecks[2],
    ];
    const decision = decidePreflight({ mode: 'release', checks });

    expect(decision.passed).toBe(false);
    expect(decision.failures[0]).toMatch(/could not be verified/);
  });

  test('an unknown advisory probe never blocks', () => {
    const checks = [
      passingChecks[0],
      passingChecks[1],
      { name: 'visibility', required: false, status: 'unknown', detail: 'unreachable' },
    ];
    expect(decidePreflight({ mode: 'release', checks }).passed).toBe(true);
  });

  test('report mode reports without blocking', () => {
    const checks = [
      { name: 'push', required: true, status: 'failed', detail: 'no push for a fork token' },
      { name: 'key', required: true, status: 'failed', detail: 'absent' },
    ];
    const decision = decidePreflight({ mode: 'report', checks });

    expect(decision.passed).toBe(true);
    expect(decision.failures).toHaveLength(2);
  });
});
