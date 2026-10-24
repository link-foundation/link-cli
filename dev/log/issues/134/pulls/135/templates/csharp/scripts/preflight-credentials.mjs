#!/usr/bin/env bun

/**
 * Prove the release can be published before any expensive job runs.
 *
 * The release jobs push to nuget.org with trusted publishing or NUGET_API_KEY and to this
 * repository with GITHUB_TOKEN. Before this preflight existed, the API key
 * was first exercised by `dotnet nuget push`, 55 capped minutes into the run,
 * and an absent key produced a green release that published nothing (a
 * version bump commit, a tag, a GitHub Release, and no package). See issues
 * #51 and #57.
 *
 * What each probe can honestly prove:
 *   - GITHUB_TOKEN push permission: GET /repos/{owner}/{repo} reports the
 *     token's permission block. push:true is a verified positive.
 *   - NUGET_USER selects trusted publishing; the OIDC policy is validated by
 *     NuGet/login immediately before publishing. Otherwise, the API key is
 *     checked via the symbol verification endpoints, including owner/glob
 *     coverage for an existing published version (issue #66).
 *   - Package visibility on nuget.org (advisory, never blocks): the
 *     flat-container index answers 200 for a published package and 404 for
 *     one whose first publish has not happened yet.
 *
 * Modes:
 *   release — push to main and workflow_dispatch: any failed probe, or a
 *             required probe that could not be verified, fails the run
 *             before the build matrix starts. Every problem is reported,
 *             not just the first.
 *   report  — pull requests (a fork legitimately has no publishing
 *             secrets): the same probes run and report, and never block.
 *
 * Usage:
 *   bun run scripts/preflight-credentials.mjs
 *
 * Environment variables:
 *   - PREFLIGHT_MODE:     'release' or 'report' (default 'report')
 *   - GITHUB_REPOSITORY:  owner/repo
 *   - GITHUB_TOKEN:       the workflow token whose push permission is probed
 *   - NUGET_USER:         repository variable selecting trusted publishing
 *   - NUGET_API_KEY:      legacy nuget.org API key to verify
 *   - CSHARP_ROOT:        optional C# root (auto-detected when unset)
 *   - GITHUB_API_URL:     override GitHub API endpoint (for tests)
 *   - NUGET_INDEX_URL:    override NuGet flat-container endpoint (for tests)
 *
 * Outputs (written to GITHUB_OUTPUT):
 *   - preflight_result: 'passed' or 'failed'
 *   - auth: 'trusted-publishing', 'api-key', or 'none'
 */

import {
  detectCsharpLayout,
  findCsharpProjectFile,
} from './release-naming.mjs';
import { readCsprojInfo } from './check-release-needed.mjs';
import { checkNugetAuthentication } from './nuget-auth.mjs';
import { appendFileSync } from 'node:fs';
import path from 'node:path';

const GITHUB_API = 'https://api.github.com';

/**
 * Append a key/value pair to GITHUB_OUTPUT (when defined) and echo to stdout.
 * @param {string} key
 * @param {string} value
 */
export function setOutput(key, value) {
  const outputFile = process.env.GITHUB_OUTPUT;
  if (outputFile) {
    appendFileSync(outputFile, `${key}=${value}\n`);
  }
  console.log(`Output: ${key}=${value}`);
}

/**
 * Probe GET /repos/{owner}/{repo} for the token's effective permissions.
 *
 * @param {object} input
 * @param {string} input.repository  owner/repo
 * @param {string} input.token       GITHUB_TOKEN value ('' probes anonymously)
 * @param {typeof fetch} [input.fetchImpl]
 * @returns {Promise<{status: 'ok'|'failed'|'unknown', detail: string}>}
 */
export async function checkGithubPushPermission({
  repository,
  token,
  fetchImpl = fetch,
}) {
  if (!repository) {
    return { status: 'unknown', detail: 'GITHUB_REPOSITORY is not set; the push permission cannot be probed' };
  }

  const baseUrl = process.env.GITHUB_API_URL ?? GITHUB_API;
  const url = `${baseUrl}/repos/${repository}`;
  console.log(`Fetching ${url}`);

  const headers = {
    Accept: 'application/vnd.github+json',
    'X-GitHub-Api-Version': '2022-11-28',
  };
  if (token) {
    headers.Authorization = `Bearer ${token}`;
  }

  let response;
  try {
    response = await fetchImpl(url, { headers });
  } catch (error) {
    return {
      status: 'unknown',
      detail: `GitHub API unreachable (${error.message}); the push permission could not be probed`,
    };
  }

  if (response.status === 403 || response.status === 429) {
    return {
      status: 'unknown',
      detail: `GitHub API answered ${response.status}; the push permission could not be probed`,
    };
  }
  if (response.status === 404) {
    return {
      status: 'failed',
      detail: `repository ${repository} is not visible to the token (HTTP 404)`,
    };
  }
  if (!response.ok) {
    return {
      status: 'unknown',
      detail: `GitHub API answered ${response.status}; the push permission could not be probed`,
    };
  }

  const payload = await response.json();
  const permissions = payload.permissions ?? {};
  if (permissions.push === true) {
    return {
      status: 'ok',
      detail: 'the workflow token can push to this repository',
    };
  }
  return {
    status: 'failed',
    detail:
      'the workflow token cannot push to this repository. Check Settings → Actions → General → Workflow permissions (needs "Read and write permissions").',
  };
}

/**
 * Advisory probe of the package's visibility on nuget.org. Never blocks: a
 * 404 is the expected state before the first publish, and NuGet creates the
 * registration on first push (unlike Packagist, which requires a submit).
 *
 * @param {object} input
 * @param {string} input.packageId
 * @param {typeof fetch} [input.fetchImpl]
 * @returns {Promise<{status: 'ok'|'failed'|'unknown', detail: string}>}
 */
export async function checkNugetPackageVisibility({
  packageId,
  fetchImpl = fetch,
}) {
  if (!packageId) {
    return {
      status: 'unknown',
      detail: 'package id could not be resolved from the csproj; visibility not probed',
    };
  }

  const baseUrl = process.env.NUGET_INDEX_URL ?? 'https://api.nuget.org/v3-flatcontainer';
  const url = `${baseUrl}/${packageId.toLowerCase()}/index.json`;
  console.log(`Fetching ${url}`);

  let response;
  try {
    response = await fetchImpl(url, { signal: AbortSignal.timeout(20000) });
  } catch (error) {
    return {
      status: 'unknown',
      detail: `NuGet flat-container index unreachable (${error.message}); visibility not probed`,
    };
  }

  if (response.status === 404) {
    return {
      status: 'ok',
      latestVersion: '',
      detail: `"${packageId}" is not on nuget.org yet — the first publish creates the registration`,
    };
  }
  if (!response.ok) {
    return {
      status: 'unknown',
      detail: `NuGet flat-container index answered ${response.status}; visibility not probed`,
    };
  }

  let payload;
  try {
    payload = await response.json();
  } catch {
    return { status: 'unknown', detail: 'NuGet flat-container index returned invalid JSON.' };
  }
  if (!Array.isArray(payload?.versions) || !payload.versions.every((version) => typeof version === 'string')) {
    return { status: 'unknown', detail: 'NuGet flat-container index returned an invalid versions list.' };
  }
  const versions = payload.versions;
  return {
    status: 'ok',
    latestVersion: versions.at(-1) || '',
    detail: `"${packageId}" is public on nuget.org with ${versions.length} published version(s)`,
  };
}

/**
 * Pure decision — exported for unit tests.
 *
 * Release mode fails when any probe failed or when a required probe did not
 * come back verified; report mode never fails. All results are returned so
 * one run names every problem, not just the first.
 *
 * @param {object} input
 * @param {'release'|'report'} input.mode
 * @param {Array<{name: string, required: boolean, status: 'ok'|'failed'|'unknown'|'skipped', detail: string}>} input.checks
 * @returns {{passed: boolean, failures: string[]}}
 */
export function decidePreflight({ mode, checks }) {
  const failures = [];
  for (const check of checks) {
    if (check.status === 'failed') {
      failures.push(`${check.name}: ${check.detail}`);
    } else if (check.required && mode === 'release' && check.status !== 'ok') {
      failures.push(
        `${check.name}: could not be verified (${check.status}) — ${check.detail}`
      );
    }
  }
  // Report mode still names every failure, but never blocks: a fork PR
  // legitimately has no publishing secrets.
  return { passed: mode === 'release' ? failures.length === 0 : true, failures };
}

/**
 * Resolve the package id from the csproj without invoking dotnet. Returns ''
 * when the layout or the csproj cannot be read — the visibility probe is
 * advisory, so a resolution failure must not crash the preflight.
 * @returns {string}
 */
export function resolvePackageId() {
  try {
    const layout = detectCsharpLayout({
      csharpRoot: process.env.CSHARP_ROOT || '',
    });
    const csharpRootPath =
      layout.csharpRoot === '.'
        ? process.cwd()
        : path.join(process.cwd(), layout.csharpRoot);
    const csprojPath =
      findCsharpProjectFile(csharpRootPath) ||
      path.join(csharpRootPath, 'src/MyPackage/MyPackage.csproj');
    return readCsprojInfo(csprojPath).packageId || '';
  } catch (error) {
    console.log(`Package id resolution skipped: ${error.message}`);
    return '';
  }
}

export async function main({ fetchImpl = fetch } = {}) {
  const mode =
    process.env.PREFLIGHT_MODE === 'release' ? 'release' : 'report';
  const repository = process.env.GITHUB_REPOSITORY || '';
  const token = process.env.GITHUB_TOKEN || '';
  const apiKey = process.env.NUGET_API_KEY || '';
  const nugetUser = process.env.NUGET_USER || '';
  const packageId = resolvePackageId();

  console.log(`Preflight mode: ${mode}`);
  console.log(`Repository:     ${repository || '(not set)'}`);
  console.log(`Package id:     ${packageId || '(unresolved)'}`);

  const pushCheck = await checkGithubPushPermission({ repository, token, fetchImpl });
  const visibilityCheck = await checkNugetPackageVisibility({ packageId, fetchImpl });
  const keyCheck = await checkNugetAuthentication({
    apiKey, nugetUser, packageId, publishedVersion: visibilityCheck.latestVersion,
    visibilityKnown: visibilityCheck.status === 'ok', fetchImpl,
  });
  setOutput('auth', keyCheck.auth);

  const checks = [
    { name: 'GITHUB_TOKEN push permission', required: true, ...pushCheck },
    { name: 'NuGet authentication', required: true, ...keyCheck },
    { name: 'NuGet package visibility', required: false, ...visibilityCheck },
  ];

  for (const check of checks) {
    const mark =
      check.status === 'ok' ? 'PASS' : check.status === 'failed' ? 'FAIL' : 'WARN';
    console.log(`[${mark}] ${check.name}: ${check.detail}`);
    if (check.status === 'failed') {
      console.error(`::error::${check.name}: ${check.detail}`);
    }
  }

  const decision = decidePreflight({ mode, checks });
  for (const failure of decision.failures) {
    console.error(
      mode === 'release'
        ? `::error::release preflight failed — ${failure}`
        : `[REPORT] ${failure}`
    );
  }

  setOutput('preflight_result', decision.passed ? 'passed' : 'failed');

  const summaryFile = process.env.GITHUB_STEP_SUMMARY;
  if (summaryFile) {
    const rows = checks
      .map(
        (check) =>
          `| ${check.name} | ${check.status} | ${check.required ? 'required' : 'advisory'} | ${check.detail} |`
      )
      .join('\n');
    appendFileSync(
      summaryFile,
      `### Release preflight (${mode} mode)\n\n` +
        `| Check | Result | Kind | Detail |\n|---|---|---|---|\n${rows}\n`
    );
  }

  if (!decision.passed) {
    console.error(
      'Release preflight failed; no build or publish job will run. Fix the probes above and push again.'
    );
  }
  return decision;
}

// Allow `import { ... } from './preflight-credentials.mjs'` without running main().
const entryPath = process.argv[1];
const invokedDirectly =
  typeof entryPath === 'string' &&
  entryPath.length > 0 &&
  (import.meta.url === `file://${entryPath}` ||
    import.meta.url.endsWith(entryPath));
if (invokedDirectly) {
  main().then((decision) => {
    if (!decision.passed) process.exitCode = 1;
  }).catch((error) => {
    console.error('Error:', error.message);
    process.exit(1);
  });
}
