#!/usr/bin/env node

/**
 * Create GitHub Release from CHANGELOG.md
 * Usage:
 *   node scripts/create-github-release.mjs --release-version <version> --repository <owner/repo> [--tag-prefix v] [--changelog-path CHANGELOG.md]
 */

import { readFileSync, existsSync } from 'fs';
import { execFileSync } from 'child_process';

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
const dryRun = args.includes('--dry-run');

if (!version || !repository) {
  console.error('Error: Missing required arguments');
  console.error(
    'Usage: node scripts/create-github-release.mjs --release-version <version> --repository <repository>'
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

  try {
    execFileSync('gh', ['release', 'view', tag, '--repo', repository], {
      stdio: 'ignore',
    });
    console.log(`Release ${tag} already exists, skipping`);
    process.exit(0);
  } catch {
    // Release does not exist yet.
  }

  try {
    execFileSync('gh', ['api', `repos/${repository}/releases`, '-X', 'POST', '--input', '-'], {
      input: JSON.stringify(payload),
      encoding: 'utf-8',
      stdio: ['pipe', 'inherit', 'inherit'],
    });
    console.log(`Created GitHub release: ${tag}`);
  } catch (error) {
    // Check if release already exists
    if (error.message && error.message.includes('already exists')) {
      console.log(`Release ${tag} already exists, skipping`);
    } else {
      throw error;
    }
  }
} catch (error) {
  console.error('Error creating release:', error.message);
  process.exit(1);
}
