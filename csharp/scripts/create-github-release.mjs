#!/usr/bin/env node

/**
 * Create GitHub Release from CHANGELOG.md
 * Usage:
 *   node csharp/scripts/create-github-release.mjs --release-version <version> --repository <owner/repo> [--tag-prefix v] [--changelog-path CHANGELOG.md] [--assets-glob csharp/artifacts/*.nupkg]
 *
 * When --package-id is provided the release notes are prefixed with NuGet
 * version and downloads shields.io badges that link to the specific package
 * version. The Rust release script does the same with Crates.io and Docs.rs
 * badges; the templates' csharp pipeline appends a NuGet badge as well.
 */

import { readFileSync, existsSync, readdirSync } from 'fs';
import { execFileSync } from 'child_process';
import { dirname, basename, join, isAbsolute, resolve } from 'path';
import { fileURLToPath } from 'url';

/**
 * Parse `--flag value` and `--flag=value` style arguments. Returns the value
 * or the provided fallback when the flag is absent.
 * @param {string[]} argv
 * @param {string} name
 * @param {string|null} fallback
 * @returns {string|null}
 */
export function getArg(argv, name, fallback = null) {
  const flag = `--${name}`;
  const eqPrefix = `${flag}=`;
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === flag) {
      return argv[i + 1] ?? fallback;
    }
    if (a.startsWith(eqPrefix)) {
      return a.slice(eqPrefix.length);
    }
  }
  return fallback;
}

/**
 * Build NuGet version + downloads badges that link to a specific package version.
 * Mirrors the Rust release script that emits Crates.io + Docs.rs badges.
 * @param {string} packageId NuGet package id (e.g. `clink`).
 * @param {string} version Bare semver string (e.g. `2.4.0`).
 * @returns {string} A single line of two markdown badges separated by a space.
 */
export function buildNugetBadges(packageId, version) {
  const id = encodeURIComponent(packageId);
  const versionPath = encodeURIComponent(version);
  const versionUrl = `https://www.nuget.org/packages/${id}/${versionPath}`;
  const versionBadge = `[![NuGet](https://img.shields.io/nuget/v/${id}?logo=nuget&label=NuGet)](${versionUrl})`;
  const downloadsBadge = `[![NuGet Downloads](https://img.shields.io/nuget/dt/${id}?logo=nuget&label=downloads)](${versionUrl})`;
  return `${versionBadge} ${downloadsBadge}`;
}

/**
 * Prepend NuGet badges to release notes when a package id is known and badges
 * are not already present. Returns the notes unchanged otherwise.
 * @param {string} releaseNotes
 * @param {string} packageId
 * @param {string} version
 * @returns {string}
 */
export function prependNugetBadges(releaseNotes, packageId, version) {
  if (!packageId || !version) {
    return releaseNotes;
  }
  if (/img\.shields\.io\/nuget\//i.test(releaseNotes)) {
    return releaseNotes;
  }
  return `${buildNugetBadges(packageId, version)}\n\n${releaseNotes}`;
}

/**
 * Extract changelog content for a specific version
 * @param {string} version
 * @param {string} changelogPath
 * @returns {string}
 */
export function getChangelogForVersion(version, changelogPath) {
  if (!existsSync(changelogPath)) {
    return `Release v${version}`;
  }

  const content = readFileSync(changelogPath, 'utf-8');

  const escapedVersion = version.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const pattern = new RegExp(
    `## \\[${escapedVersion}\\].*?\\n([\\s\\S]*?)(?=\\n## \\[|$)`
  );
  const match = content.match(pattern);

  if (match) {
    return match[1].trim();
  }

  return `Release v${version}`;
}

/**
 * Build the GitHub release API payload.
 * @param {{changelogPath: string, language: string, packageId: string, releaseVersion: string, tagPrefix: string}} options
 * @returns {{tag_name: string, name: string, body: string}}
 */
export function buildReleasePayload({
  changelogPath,
  language,
  packageId,
  releaseVersion,
  tagPrefix,
}) {
  const changelogNotes = getChangelogForVersion(releaseVersion, changelogPath);
  const notesWithPackage = packageId
    ? `${changelogNotes}\n\nPackage: \`${packageId}\``
    : changelogNotes;
  const body = prependNugetBadges(notesWithPackage, packageId, releaseVersion);

  return {
    tag_name: `${tagPrefix}${releaseVersion}`,
    name: language ? `${language} v${releaseVersion}` : `v${releaseVersion}`,
    body,
  };
}

/**
 * Resolve a simple `directory/*.ext` glob to a list of file paths.
 * Only `*` in the file name part is supported; matches are returned in name order.
 * @param {string} pattern
 * @returns {string[]}
 */
export function resolveAssets(pattern) {
  if (!pattern) return [];
  const dir = dirname(pattern) || '.';
  const filePattern = basename(pattern);
  if (!existsSync(dir)) return [];

  if (!filePattern.includes('*')) {
    const candidate = isAbsolute(pattern) ? pattern : join(dir, filePattern);
    return existsSync(candidate) ? [candidate] : [];
  }

  const escaped = filePattern.replace(/[.+?^${}()|[\]\\]/g, '\\$&').replace(/\*/g, '.*');
  const regex = new RegExp(`^${escaped}$`);
  return readdirSync(dir)
    .filter((name) => regex.test(name))
    .sort()
    .map((name) => join(dir, name));
}

function main(argv) {
  const version = getArg(argv, 'release-version');
  const repository = getArg(argv, 'repository');
  const tagPrefix = getArg(argv, 'tag-prefix', 'v');
  const changelogPath = getArg(argv, 'changelog-path', 'CHANGELOG.md');
  const language = getArg(argv, 'language', '');
  const packageId = getArg(argv, 'package-id', '');
  const assetsGlob = getArg(argv, 'assets-glob', '');
  const dryRun = argv.includes('--dry-run');

  if (!version || !repository) {
    console.error('Error: Missing required arguments');
    console.error(
      'Usage: node csharp/scripts/create-github-release.mjs --release-version <version> --repository <repository>'
    );
    process.exit(1);
  }

  const payload = buildReleasePayload({
    changelogPath,
    language,
    packageId,
    releaseVersion: version,
    tagPrefix,
  });
  const tag = payload.tag_name;

  console.log(`Creating GitHub release for ${tag}...`);

  if (dryRun) {
    console.log(JSON.stringify(payload, null, 2));
    process.exit(0);
  }

  try {
    const assetPaths = resolveAssets(assetsGlob);

    let releaseExists = false;
    try {
      execFileSync('gh', ['release', 'view', tag, '--repo', repository], {
        stdio: 'ignore',
      });
      releaseExists = true;
      console.log(`Release ${tag} already exists, will reconcile assets`);
    } catch {
      // Release does not exist yet.
    }

    if (!releaseExists) {
      try {
        execFileSync('gh', ['api', `repos/${repository}/releases`, '-X', 'POST', '--input', '-'], {
          input: JSON.stringify(payload),
          encoding: 'utf-8',
          stdio: ['pipe', 'inherit', 'inherit'],
        });
        console.log(`Created GitHub release: ${tag}`);
      } catch (error) {
        if (error.message && error.message.includes('already exists')) {
          console.log(`Release ${tag} already exists, will reconcile assets`);
        } else {
          throw error;
        }
      }
    }

    if (assetPaths.length === 0) {
      if (assetsGlob) {
        console.log(`No assets matched ${assetsGlob}, skipping asset upload`);
      }
    } else {
      console.log(`Uploading ${assetPaths.length} asset(s) to ${tag}`);
      execFileSync(
        'gh',
        ['release', 'upload', tag, ...assetPaths, '--clobber', '--repo', repository],
        { stdio: 'inherit' }
      );
    }
  } catch (error) {
    console.error('Error creating release:', error.message);
    process.exit(1);
  }
}

function isCliEntryPoint() {
  return (
    typeof process !== 'undefined' &&
    process.argv?.[1] &&
    fileURLToPath(import.meta.url) === resolve(process.argv[1])
  );
}

if (isCliEntryPoint()) {
  main(process.argv.slice(2));
}
