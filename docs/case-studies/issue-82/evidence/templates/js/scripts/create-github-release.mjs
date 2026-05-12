#!/usr/bin/env bun

/**
 * Create GitHub Release from CHANGELOG.md
 * Usage: node scripts/create-github-release.mjs --release-version <version> --repository <repository> [--tag-prefix <prefix>] [--language <language>]
 *   release-version: Version number (e.g., 1.0.0)
 *   repository: GitHub repository (e.g., owner/repo)
 *   tag-prefix: Prefix for the git tag (default: "v", use "js-v" for multi-language repos)
 *   language: Human-readable language name for the release title (default: "JavaScript")
 */

import { spawnSync } from 'node:child_process';
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const USAGE =
  'Usage: node scripts/create-github-release.mjs --release-version <version> --repository <repository> [--tag-prefix <prefix>] [--language <language>]';

export function parseArgs(argv, env = process.env) {
  const config = {
    language: env.LANGUAGE ?? 'JavaScript',
    releaseVersion: env.VERSION ?? '',
    repository: env.REPOSITORY ?? '',
    tagPrefix: env.TAG_PREFIX ?? 'v',
  };

  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];

    if (arg === '--release-version') {
      config.releaseVersion = readOptionValue(argv, index, arg);
      index++;
    } else if (arg.startsWith('--release-version=')) {
      config.releaseVersion = arg.slice('--release-version='.length);
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
    }
  }

  return config;
}

function readOptionValue(argv, index, optionName) {
  const value = argv[index + 1];

  if (value === undefined || value.startsWith('--')) {
    throw new Error(`Missing value for ${optionName}`);
  }

  return value;
}

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

export function extractReleaseNotes(changelog, version) {
  // Read from CHANGELOG.md between this version header and the next version header.
  const versionHeaderRegex = new RegExp(
    `## ${escapeRegex(version)}[\\s\\S]*?(?=## \\d|$)`
  );
  const match = changelog.match(versionHeaderRegex);

  if (!match) {
    return `Release ${version}`;
  }

  const releaseNotes = match[0].replace(`## ${version}`, '').trim();

  return releaseNotes || `Release ${version}`;
}

export function normalizeReleaseVersionForTitle(releaseVersion) {
  const trimmedVersion = releaseVersion.trim();
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

export function buildReleaseTitle(language, releaseVersion) {
  const titleLanguage = language.trim() || 'JavaScript';
  return `[${titleLanguage}] ${normalizeReleaseVersionForTitle(releaseVersion)}`;
}

export function buildReleasePayload({ changelog, language, tag, version }) {
  return JSON.stringify({
    tag_name: tag,
    name: buildReleaseTitle(language ?? 'JavaScript', tag),
    body: extractReleaseNotes(changelog, version),
  });
}

function formatGhOutput(result) {
  return [result.stderr, result.stdout]
    .filter((output) => typeof output === 'string' && output.trim())
    .map((output) => output.trim())
    .join('\n');
}

function getGhExitDescription(result) {
  if (result.signal) {
    return `signal ${result.signal}`;
  }

  if (typeof result.status === 'number') {
    return `code ${result.status}`;
  }

  return 'unknown exit status';
}

export function createRelease({ payload, repository, spawn = spawnSync }) {
  const result = spawn(
    'gh',
    ['api', `repos/${repository}/releases`, '-X', 'POST', '--input', '-'],
    {
      encoding: 'utf8',
      input: payload,
    }
  );

  if (result.error) {
    throw new Error(`gh api failed to start: ${result.error.message}`);
  }

  if (result.status === 0) {
    return { alreadyExists: false };
  }

  const output = formatGhOutput(result);

  if (/already_exists/i.test(output)) {
    return { alreadyExists: true };
  }

  const details = output ? `:\n${output}` : '';
  throw new Error(
    `gh api failed with ${getGhExitDescription(result)}${details}`
  );
}

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
      language,
      releaseVersion: version,
      repository,
      tagPrefix,
    } = parseArgs(argv, env);

    if (!version || !repository) {
      stderr('Error: Missing required arguments');
      stderr(USAGE);
      return 1;
    }

    const tag = `${tagPrefix}${version}`;

    stdout(`Creating GitHub release for ${tag}...`);

    const changelog = readFileSync(path.join(cwd, 'CHANGELOG.md'), 'utf8');
    const payload = buildReleasePayload({ changelog, language, tag, version });
    const result = createRelease({ payload, repository, spawn });

    if (result.alreadyExists) {
      stdout(`GitHub release already exists: ${tag}. Skipping creation.`);
      return 0;
    }

    stdout(`\u2705 Created GitHub release: ${tag}`);
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
