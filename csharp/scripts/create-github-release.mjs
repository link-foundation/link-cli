#!/usr/bin/env node

/**
 * Create GitHub Release from CHANGELOG.md
 * Usage:
 *   node csharp/scripts/create-github-release.mjs --release-version <version> --repository <owner/repo> [--tag-prefix v] [--changelog-path CHANGELOG.md] [--assets-glob csharp/artifacts/*.nupkg]
 */

import { readFileSync, existsSync, readdirSync } from 'fs';
import { execFileSync } from 'child_process';
import { dirname, basename, join, isAbsolute } from 'path';

// Simple argument parsing
const args = process.argv.slice(2);
const getArg = (name, fallback = null) => {
  const index = args.indexOf(`--${name}`);
  if (index === -1) return fallback;
  return args[index + 1] ?? fallback;
};

const version = getArg('release-version');
const repository = getArg('repository');
const tagPrefix = getArg('tag-prefix', 'v');
const changelogPath = getArg('changelog-path', 'CHANGELOG.md');
const language = getArg('language', '');
const packageId = getArg('package-id', '');
const assetsGlob = getArg('assets-glob', '');
const dryRun = args.includes('--dry-run');

/**
 * Resolve a simple `directory/*.ext` glob to a list of file paths.
 * Only `*` in the file name part is supported; matches are returned in name order.
 */
function resolveAssets(pattern) {
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

if (!version || !repository) {
  console.error('Error: Missing required arguments');
  console.error(
    'Usage: node csharp/scripts/create-github-release.mjs --release-version <version> --repository <repository>'
  );
  process.exit(1);
}

const tag = `${tagPrefix}${version}`;

console.log(`Creating GitHub release for ${tag}...`);

/**
 * Extract changelog content for a specific version
 * @param {string} version
 * @param {string} changelogPath
 * @returns {string}
 */
function getChangelogForVersion(version, changelogPath) {
  if (!existsSync(changelogPath)) {
    return `Release v${version}`;
  }

  const content = readFileSync(changelogPath, 'utf-8');

  // Find the section for this version
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

try {
  const releaseNotes = getChangelogForVersion(version, changelogPath);
  const body = packageId
    ? `${releaseNotes}\n\nPackage: \`${packageId}\``
    : releaseNotes;

  const payload = {
    tag_name: tag,
    name: language ? `${language} v${version}` : `v${version}`,
    body,
  };

  if (dryRun) {
    console.log(JSON.stringify(payload, null, 2));
    process.exit(0);
  }

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
