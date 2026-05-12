#!/usr/bin/env node

/**
 * Check if a C# release is needed based on changesets and NuGet registry state.
 *
 * This script checks:
 *   1. If there are changeset files to process.
 *   2. If the current csproj <Version> has already been published to NuGet.
 *   3. If the matching GitHub Release already exists.
 *
 * IMPORTANT: This script checks NuGet (the source of truth for the package)
 * and the GitHub Releases API, NOT git tags. Git tags can exist without the
 * package being on NuGet — that is exactly the failure mode that motivates
 * issue #11: a version-bump commit and a tag were pushed, but
 * `dotnet nuget push` returned HTTP 403, so the public package was never
 * created. Re-runs then short-circuited on the pre-existing tag and never
 * retried the publish.
 *
 * This provides a self-healing mechanism: if a previous release attempt failed
 * after the version commit and tag were pushed, the next push to main detects
 * the unpublished version on NuGet and resumes the publish + GitHub release
 * steps without requiring a new changeset.
 *
 * Analogous to `check-release-needed.mjs` in the JavaScript template.
 *
 * Usage:
 *   bun run scripts/check-release-needed.mjs
 *     [--csproj <path>] [--repository <owner/repo>] [--tag-prefix <prefix>]
 *     [--package-id <id>]
 *
 * Environment variables:
 *   - HAS_CHANGESETS:    'true' if changeset files exist (from check_changesets step)
 *   - NUGET_INDEX_URL:   override NuGet flat-container endpoint (for tests)
 *   - GITHUB_API_URL:    override GitHub API endpoint (for tests)
 *   - GITHUB_REPOSITORY: owner/repo, used when --repository is omitted
 *   - GH_TOKEN / GITHUB_TOKEN: optional auth for the GitHub release probe
 *
 * Outputs (written to GITHUB_OUTPUT):
 *   - should_release:        'true' if a release should be created
 *   - skip_bump:             'true' if the version bump should be skipped
 *                            (csproj version is not yet on NuGet)
 *   - current_version:       the current csproj <Version> value
 *   - nuget_published:       'true' if the current version is on NuGet
 *   - github_release_exists: 'true' if the matching GitHub release exists
 *   - reason:                short human-readable reason
 */

import { appendFileSync, readFileSync } from 'node:fs';

const NUGET_FLAT_CONTAINER = 'https://api.nuget.org/v3-flatcontainer';
const GITHUB_API = 'https://api.github.com';

const args = process.argv.slice(2);

/**
 * Read a CLI flag value.
 * @param {string} name
 * @returns {string | null}
 */
function getArg(name) {
  const equalsIndex = args.findIndex((arg) => arg.startsWith(`--${name}=`));
  if (equalsIndex !== -1) {
    return args[equalsIndex].slice(`--${name}=`.length);
  }
  const index = args.indexOf(`--${name}`);
  if (index === -1) {
    return null;
  }
  const value = args[index + 1];
  if (value === undefined || value.startsWith('--')) {
    return '';
  }
  return value;
}

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
 * Append a markdown block to GITHUB_STEP_SUMMARY (when defined).
 * @param {string} markdown
 */
function appendStepSummary(markdown) {
  const summaryFile = process.env.GITHUB_STEP_SUMMARY;
  if (summaryFile) {
    appendFileSync(summaryFile, `${markdown}\n`);
  }
}

/**
 * Extract <Version> and <PackageId> from a csproj file.
 * @param {string} csprojPath
 * @returns {{ version: string, packageId: string }}
 */
export function readCsprojInfo(csprojPath) {
  const content = readFileSync(csprojPath, 'utf-8');

  const versionMatch = content.match(/<Version>([^<]+)<\/Version>/);
  if (!versionMatch) {
    throw new Error(`Could not parse <Version> from ${csprojPath}`);
  }

  const packageIdMatch = content.match(/<PackageId>([^<]+)<\/PackageId>/);
  // PackageId falls back to the AssemblyName/csproj file name when omitted —
  // expose the explicit value to the caller, otherwise the workflow will
  // resolve via msbuild like it already does for the publish step.
  return {
    version: versionMatch[1].trim(),
    packageId: packageIdMatch ? packageIdMatch[1].trim() : '',
  };
}

/**
 * Probe `https://api.nuget.org/v3-flatcontainer/{id-lower}/index.json` for the
 * package's published versions.
 *
 * Returns null when the package id is not registered on NuGet at all (HTTP 404
 * for the index endpoint). Returns an empty array if the registration exists
 * but the index has no versions (extremely rare). Otherwise returns the
 * declared version list.
 *
 * @param {string} packageId
 * @param {typeof fetch} fetchImpl
 * @returns {Promise<string[] | null>}
 */
export async function fetchNugetVersions(packageId, fetchImpl = fetch) {
  const baseUrl = process.env.NUGET_INDEX_URL ?? NUGET_FLAT_CONTAINER;
  const url = `${baseUrl}/${packageId.toLowerCase()}/index.json`;
  console.log(`Fetching ${url}`);

  const response = await fetchImpl(url);
  if (response.status === 404) {
    return null;
  }
  if (!response.ok) {
    throw new Error(
      `NuGet flat-container index returned HTTP ${response.status} for ${packageId}`
    );
  }
  const payload = await response.json();
  return Array.isArray(payload.versions) ? payload.versions : [];
}

/**
 * Probe `GET /repos/{owner}/{repo}/releases/tags/{tag}` to see if a matching
 * GitHub release already exists.
 *
 * @param {string} repository  owner/repo
 * @param {string} tag         full tag, e.g. csharp_v2.4.0
 * @param {typeof fetch} fetchImpl
 * @returns {Promise<boolean>}
 */
export async function fetchGithubReleaseExists(
  repository,
  tag,
  fetchImpl = fetch
) {
  if (!repository) {
    return false;
  }
  const baseUrl = process.env.GITHUB_API_URL ?? GITHUB_API;
  const url = `${baseUrl}/repos/${repository}/releases/tags/${encodeURIComponent(tag)}`;
  console.log(`Fetching ${url}`);

  const headers = { Accept: 'application/vnd.github+json' };
  if (process.env.GH_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GH_TOKEN}`;
  } else if (process.env.GITHUB_TOKEN) {
    headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  }

  const response = await fetchImpl(url, { headers });
  if (response.status === 404) {
    return false;
  }
  if (!response.ok) {
    throw new Error(`GitHub releases endpoint returned HTTP ${response.status}`);
  }
  return true;
}

/**
 * @typedef {object} CheckResult
 * @property {boolean} shouldRelease
 * @property {boolean} skipBump
 * @property {string}  currentVersion
 * @property {boolean} nugetPublished
 * @property {boolean} githubReleaseExists
 * @property {string}  reason
 */

/**
 * Pure decision function — exported for unit tests.
 * @param {object} input
 * @param {boolean} input.hasChangesets
 * @param {string}  input.currentVersion
 * @param {string[] | null} input.publishedVersions
 *   `null` when the package id is not registered on NuGet at all.
 * @param {boolean} input.githubReleaseExists
 * @returns {CheckResult}
 */
export function decide({
  hasChangesets,
  currentVersion,
  publishedVersions,
  githubReleaseExists,
}) {
  const nugetPublished =
    Array.isArray(publishedVersions) && publishedVersions.includes(currentVersion);

  if (hasChangesets) {
    return {
      shouldRelease: true,
      skipBump: false,
      currentVersion,
      nugetPublished,
      githubReleaseExists,
      reason: 'changesets present — normal release path',
    };
  }

  if (!nugetPublished) {
    return {
      shouldRelease: true,
      skipBump: true,
      currentVersion,
      nugetPublished,
      githubReleaseExists,
      reason:
        publishedVersions === null
          ? `package not yet registered on NuGet — self-healing resume for v${currentVersion}`
          : `v${currentVersion} not yet published on NuGet — self-healing resume`,
    };
  }

  if (!githubReleaseExists) {
    return {
      shouldRelease: true,
      skipBump: true,
      currentVersion,
      nugetPublished,
      githubReleaseExists,
      reason: `v${currentVersion} on NuGet but no GitHub release — self-healing release creation`,
    };
  }

  return {
    shouldRelease: false,
    skipBump: false,
    currentVersion,
    nugetPublished,
    githubReleaseExists,
    reason: `v${currentVersion} already on NuGet and GitHub — no release needed`,
  };
}

async function main() {
  const csprojPath = getArg('csproj') || 'src/MyPackage/MyPackage.csproj';
  const repository = getArg('repository') || process.env.GITHUB_REPOSITORY || '';
  const tagPrefix = getArg('tag-prefix') || 'csharp_v';
  const packageIdOverride = getArg('package-id') || '';

  const csproj = readCsprojInfo(csprojPath);
  const packageId = packageIdOverride || csproj.packageId || 'MyPackage';
  const currentVersion = csproj.version;

  console.log(`csproj path:     ${csprojPath}`);
  console.log(`Package id:      ${packageId}`);
  console.log(`Current version: ${currentVersion}`);
  console.log(`Repository:      ${repository || '(not set)'}`);
  console.log(`Has changesets:  ${process.env.HAS_CHANGESETS === 'true'}`);

  const publishedVersions = await fetchNugetVersions(packageId);
  if (publishedVersions === null) {
    console.log(`NuGet: package "${packageId}" not registered yet`);
  } else {
    console.log(`NuGet: ${publishedVersions.length} version(s) registered`);
    console.log(`NuGet versions: ${publishedVersions.join(', ')}`);
  }

  const tag = `${tagPrefix}${currentVersion}`;
  const githubReleaseExists = await fetchGithubReleaseExists(repository, tag);
  console.log(`GitHub release ${tag}: ${githubReleaseExists ? 'exists' : 'missing'}`);

  const decision = decide({
    hasChangesets: process.env.HAS_CHANGESETS === 'true',
    currentVersion,
    publishedVersions,
    githubReleaseExists,
  });

  console.log(`Decision: ${decision.reason}`);

  setOutput('should_release', decision.shouldRelease ? 'true' : 'false');
  setOutput('skip_bump', decision.skipBump ? 'true' : 'false');
  setOutput('current_version', decision.currentVersion);
  setOutput('nuget_published', decision.nugetPublished ? 'true' : 'false');
  setOutput(
    'github_release_exists',
    decision.githubReleaseExists ? 'true' : 'false'
  );
  setOutput('reason', decision.reason);

  appendStepSummary(
    `### C# release decision\n\n` +
      `- Package: \`${packageId}\`\n` +
      `- csproj \`<Version>\`: \`${currentVersion}\`\n` +
      `- On NuGet: ${decision.nugetPublished ? 'yes' : 'no'}\n` +
      `- GitHub release \`${tag}\`: ${decision.githubReleaseExists ? 'exists' : 'missing'}\n` +
      `- \`should_release\`: \`${decision.shouldRelease}\`\n` +
      `- \`skip_bump\`: \`${decision.skipBump}\`\n` +
      `- Reason: ${decision.reason}\n`
  );
}

// Allow `import { decide, readCsprojInfo, ... }` without running main().
const entryPath = process.argv[1];
const invokedDirectly =
  typeof entryPath === 'string' &&
  entryPath.length > 0 &&
  (import.meta.url === `file://${entryPath}` ||
    import.meta.url.endsWith(entryPath));
if (invokedDirectly) {
  main().catch((error) => {
    console.error('Error:', error.message);
    process.exit(1);
  });
}
