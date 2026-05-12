#!/usr/bin/env bun

/**
 * Create GitHub Release from CHANGELOG.md
 * Usage: bun run scripts/create-github-release.mjs --release-version <version> --repository <repository> [--tag-prefix <prefix>] [--language <language>] [--package-id <id>] [--assets-glob <glob>]
 */

import { spawnSync } from 'node:child_process';
import {
  existsSync,
  readFileSync,
  readdirSync,
  statSync,
} from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const USAGE =
  'Usage: bun run scripts/create-github-release.mjs --release-version <version> --repository <repository> [--tag-prefix <prefix>] [--language <language>] [--package-id <id>] [--assets-glob <glob>]';

/**
 * Parse CLI arguments.
 * @param {string[]} argv
 * @param {NodeJS.ProcessEnv} env
 * @returns {{assetsGlob: string, releaseVersion: string, repository: string, tagPrefix: string, language: string, packageId: string}}
 */
export function parseArgs(argv, env = process.env) {
  const config = {
    assetsGlob: env.ASSETS_GLOB ?? '',
    language: env.LANGUAGE ?? 'C#',
    packageId: env.PACKAGE_ID ?? '',
    releaseVersion: env.VERSION ?? '',
    repository: env.REPOSITORY ?? '',
    tagPrefix: env.TAG_PREFIX ?? 'csharp_v',
  };

  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];

    if (arg === '--release-version' || arg === '--version') {
      config.releaseVersion = readOptionValue(argv, index, arg);
      index++;
    } else if (arg.startsWith('--release-version=')) {
      config.releaseVersion = arg.slice('--release-version='.length);
    } else if (arg.startsWith('--version=')) {
      config.releaseVersion = arg.slice('--version='.length);
    } else if (arg === '--repository') {
      config.repository = readOptionValue(argv, index, arg);
      index++;
    } else if (arg.startsWith('--repository=')) {
      config.repository = arg.slice('--repository='.length);
    } else if (arg === '--tag-prefix') {
      config.tagPrefix = readOptionValue(argv, index, arg);
      index++;
    } else if (arg.startsWith('--tag-prefix=')) {
      config.tagPrefix = arg.slice('--tag-prefix='.length);
    } else if (arg === '--language') {
      config.language = readOptionValue(argv, index, arg);
      index++;
    } else if (arg.startsWith('--language=')) {
      config.language = arg.slice('--language='.length);
    } else if (arg === '--package-id') {
      config.packageId = readOptionValue(argv, index, arg);
      index++;
    } else if (arg.startsWith('--package-id=')) {
      config.packageId = arg.slice('--package-id='.length);
    } else if (arg === '--assets-glob') {
      config.assetsGlob = readOptionValue(argv, index, arg);
      index++;
    } else if (arg.startsWith('--assets-glob=')) {
      config.assetsGlob = arg.slice('--assets-glob='.length);
    }
  }

  return config;
}

/**
 * Read a CLI option value.
 * @param {string[]} argv
 * @param {number} index
 * @param {string} optionName
 * @returns {string}
 */
function readOptionValue(argv, index, optionName) {
  const value = argv[index + 1];

  if (value === undefined || value.startsWith('--')) {
    throw new Error(`Missing value for ${optionName}`);
  }

  return value;
}

/**
 * Escape text for a regular expression.
 * @param {string} value
 * @returns {string}
 */
function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * Normalize release versions to bare semver.
 * @param {string} releaseVersion
 * @returns {string}
 */
export function normalizeReleaseVersion(releaseVersion) {
  const trimmedVersion = String(releaseVersion ?? '').trim();
  const semverTagMatch = trimmedVersion.match(
    /(?:^|[-_])v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?)$/i
  );

  if (semverTagMatch) {
    return semverTagMatch[1];
  }

  return trimmedVersion
    .replace(/^[A-Za-z][A-Za-z0-9]*[-_]/, '')
    .replace(/^v/i, '');
}

/**
 * Build a release tag.
 * @param {string} tagPrefix
 * @param {string} releaseVersion
 * @returns {string}
 */
export function buildReleaseTag(tagPrefix, releaseVersion) {
  return `${tagPrefix}${normalizeReleaseVersion(releaseVersion)}`;
}

/**
 * Build a release title.
 * @param {string} language
 * @param {string} releaseVersion
 * @returns {string}
 */
export function buildReleaseTitle(language, releaseVersion) {
  const releaseLanguage = language.trim() || 'C#';
  return `[${releaseLanguage}] ${normalizeReleaseVersion(releaseVersion)}`;
}

/**
 * Build a NuGet badge markdown link.
 * @param {string} packageId
 * @returns {string}
 */
export function buildNuGetBadge(packageId) {
  const encodedPackageId = encodeURIComponent(packageId);
  return `[![NuGet](https://img.shields.io/nuget/v/${encodedPackageId}.svg)](https://www.nuget.org/packages/${encodedPackageId})`;
}

/**
 * Append a NuGet badge unless release notes already include a shields.io badge.
 * @param {string} releaseNotes
 * @param {string} packageId
 * @returns {string}
 */
export function appendNuGetBadgeIfMissing(releaseNotes, packageId) {
  if (!packageId || /img\.shields\.io/i.test(releaseNotes)) {
    return releaseNotes;
  }

  return `${releaseNotes}\n\n---\n\n${buildNuGetBadge(packageId)}`;
}

/**
 * Extract changelog content for a specific version
 * @param {string} changelog
 * @param {string} version
 * @returns {string}
 */
export function extractReleaseNotes(changelog, version) {
  const semver = normalizeReleaseVersion(version);

  // Find the section for this version
  const escapedVersion = escapeRegex(semver);
  const pattern = new RegExp(
    `(?:^|\\n)## \\[?${escapedVersion}\\]?[^\\n]*\\n([\\s\\S]*?)(?=\\n## \\[?\\d|$)`
  );
  const match = changelog.match(pattern);

  if (match) {
    const releaseNotes = match[1].trim();
    return releaseNotes || `Release ${semver}`;
  }

  return `Release ${semver}`;
}

/**
 * Find a package id by scanning project files.
 * @param {string} rootDir
 * @returns {string}
 */
export function findPackageId(rootDir = '.') {
  const candidates = [];

  walkProjectFiles(rootDir, candidates);

  for (const csprojPath of candidates) {
    const csproj = readFileSync(csprojPath, 'utf-8');
    const packageIdMatch = csproj.match(/<PackageId>([^<]+)<\/PackageId>/);
    if (packageIdMatch) {
      return packageIdMatch[1].trim();
    }

    const assemblyNameMatch = csproj.match(
      /<AssemblyName>([^<]+)<\/AssemblyName>/
    );
    if (assemblyNameMatch) {
      return assemblyNameMatch[1].trim();
    }
  }

  if (candidates.length > 0) {
    return path.basename(candidates[0], '.csproj');
  }

  return '';
}

/**
 * Walk project files under a root directory.
 * @param {string} dir
 * @param {string[]} candidates
 * @param {number} depth
 */
function walkProjectFiles(dir, candidates, depth = 0) {
  if (depth > 4) {
    return;
  }

  let entries;
  try {
    entries = readdirSync(dir);
  } catch {
    return;
  }

  for (const entry of entries) {
    if (
      entry === '.git' ||
      entry === 'bin' ||
      entry === 'obj' ||
      entry === 'node_modules'
    ) {
      continue;
    }

    const fullPath = path.join(dir, entry);
    let stat;
    try {
      stat = statSync(fullPath);
    } catch {
      continue;
    }

    if (stat.isDirectory()) {
      walkProjectFiles(fullPath, candidates, depth + 1);
    } else if (fullPath.endsWith('.csproj')) {
      candidates.push(fullPath);
    }
  }
}

/**
 * Build the GitHub release API payload.
 * @param {{changelog: string, language: string, packageId: string, releaseVersion: string, tagPrefix: string}} options
 * @returns {string}
 */
export function buildReleasePayload({
  changelog,
  language,
  packageId,
  releaseVersion,
  tagPrefix,
}) {
  const semver = normalizeReleaseVersion(releaseVersion);
  const releaseNotes = appendNuGetBadgeIfMissing(
    extractReleaseNotes(changelog, semver),
    packageId
  );

  return JSON.stringify({
    tag_name: buildReleaseTag(tagPrefix, semver),
    name: buildReleaseTitle(language, semver),
    body: releaseNotes,
  });
}

/**
 * Create a GitHub release using gh.
 * @param {{payload: string, repository: string, spawn?: typeof spawnSync}} options
 * @returns {{alreadyExists: boolean}}
 */
export function createRelease({ payload, repository, spawn = spawnSync }) {
  const result = spawn(
    'gh',
    ['api', `repos/${repository}/releases`, '-X', 'POST', '--input', '-'],
    {
      encoding: 'utf-8',
      input: payload,
    }
  );

  if (result.error) {
    throw new Error(`gh api failed to start: ${result.error.message}`);
  }

  if (result.status === 0) {
    return { alreadyExists: false };
  }

  const output = [result.stderr, result.stdout]
    .filter((value) => typeof value === 'string' && value.trim())
    .join('\n');

  if (/already_exists|already exists/i.test(output)) {
    return { alreadyExists: true };
  }

  throw new Error(`gh api failed with code ${result.status}: ${output}`);
}

/**
 * Resolve a simple release asset glob.
 *
 * Supports exact file paths or `*` in the file name portion, such as
 * `artifacts/*.nupkg`. Matches are returned in deterministic path order.
 *
 * @param {string} assetsGlob
 * @param {string} cwd
 * @returns {string[]}
 */
export function resolveReleaseAssets(assetsGlob, cwd = '.') {
  const pattern = String(assetsGlob ?? '').trim();
  if (!pattern) {
    return [];
  }

  const absolutePattern = path.isAbsolute(pattern)
    ? pattern
    : path.resolve(cwd, pattern);
  const assetDirectory = path.dirname(absolutePattern);
  const filePattern = path.basename(absolutePattern);

  if (!filePattern.includes('*')) {
    try {
      return existsSync(absolutePattern) && statSync(absolutePattern).isFile()
        ? [absolutePattern]
        : [];
    } catch {
      return [];
    }
  }

  let entries;
  try {
    entries = readdirSync(assetDirectory, { withFileTypes: true });
  } catch {
    return [];
  }

  const filePatternRegex = new RegExp(
    `^${escapeRegex(filePattern).replace(/\\\*/g, '.*')}$`
  );

  return entries
    .filter((entry) => entry.isFile() && filePatternRegex.test(entry.name))
    .map((entry) => path.join(assetDirectory, entry.name))
    .sort();
}

/**
 * Upload release assets using gh.
 * @param {{assetPaths: string[], repository: string, tag: string, spawn?: typeof spawnSync}} options
 * @returns {void}
 */
export function uploadReleaseAssets({
  assetPaths,
  repository,
  tag,
  spawn = spawnSync,
}) {
  if (assetPaths.length === 0) {
    return;
  }

  const result = spawn(
    'gh',
    [
      'release',
      'upload',
      tag,
      ...assetPaths,
      '--clobber',
      '--repo',
      repository,
    ],
    { encoding: 'utf-8' }
  );

  if (result.error) {
    throw new Error(`gh release upload failed to start: ${result.error.message}`);
  }

  if (result.status !== 0) {
    const output = [result.stderr, result.stdout]
      .filter((value) => typeof value === 'string' && value.trim())
      .join('\n');

    throw new Error(
      `gh release upload failed with code ${result.status}: ${output}`
    );
  }
}

/**
 * Run the CLI.
 * @param {{argv?: string[], cwd?: string, env?: NodeJS.ProcessEnv, spawn?: typeof spawnSync, stderr?: typeof console.error, stdout?: typeof console.log}} options
 * @returns {number}
 */
export function main({
  argv = process.argv.slice(2),
  cwd = process.cwd(),
  env = process.env,
  spawn = spawnSync,
  stderr = console.error,
  stdout = console.log,
} = {}) {
  try {
    const {
      assetsGlob,
      language,
      packageId,
      releaseVersion,
      repository,
      tagPrefix,
    } =
      parseArgs(argv, env);

    if (!releaseVersion || !repository) {
      stderr('Error: Missing required arguments');
      stderr(USAGE);
      return 1;
    }

    const changelogPath = path.join(cwd, 'CHANGELOG.md');
    const changelog = existsSync(changelogPath)
      ? readFileSync(changelogPath, 'utf-8')
      : '';
    const resolvedPackageId = packageId || findPackageId(cwd);
    const tag = buildReleaseTag(tagPrefix, releaseVersion);
    const payload = buildReleasePayload({
      changelog,
      language,
      packageId: resolvedPackageId,
      releaseVersion,
      tagPrefix,
    });

    stdout(`Creating GitHub release for ${tag}...`);

    const result = createRelease({ payload, repository, spawn });

    if (result.alreadyExists) {
      stdout(`GitHub release already exists: ${tag}, reconciling assets`);
    } else {
      stdout(`Created GitHub release: ${tag}`);
    }

    if (assetsGlob) {
      const assetPaths = resolveReleaseAssets(assetsGlob, cwd);

      if (assetPaths.length === 0) {
        throw new Error(`No release assets matched ${assetsGlob}`);
      }

      stdout(`Uploading ${assetPaths.length} release asset(s) to ${tag}...`);
      uploadReleaseAssets({ assetPaths, repository, spawn, tag });
      stdout(`Uploaded ${assetPaths.length} release asset(s) to ${tag}`);
    }

    return 0;
  } catch (error) {
    stderr(`Error creating release: ${error.message}`);
    return 1;
  }
}

function isCliEntryPoint() {
  return (
    typeof process !== 'undefined' &&
    process.argv?.[1] &&
    fileURLToPath(import.meta.url) === path.resolve(process.argv[1])
  );
}

if (isCliEntryPoint()) {
  process.exitCode = main();
}
