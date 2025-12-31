#!/usr/bin/env node

/**
 * Create GitHub Release from CHANGELOG.md
 * Usage: bun run scripts/create-github-release.mjs --release-version <version> --repository <repository>
 */

import { readFileSync, existsSync } from 'fs';
import { execSync } from 'child_process';

// Simple argument parsing
const args = process.argv.slice(2);
const getArg = (name) => {
  const index = args.indexOf(`--${name}`);
  if (index === -1) return null;
  return args[index + 1];
};

const version = getArg('release-version');
const repository = getArg('repository');

if (!version || !repository) {
  console.error('Error: Missing required arguments');
  console.error(
    'Usage: bun run scripts/create-github-release.mjs --release-version <version> --repository <repository>'
  );
  process.exit(1);
}

const tag = `v${version}`;

console.log(`Creating GitHub release for ${tag}...`);

/**
 * Extract changelog content for a specific version
 * @param {string} version
 * @returns {string}
 */
function getChangelogForVersion(version) {
  const changelogPath = 'CHANGELOG.md';

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
  const releaseNotes = getChangelogForVersion(version);

  // Create release using gh CLI
  const payload = JSON.stringify({
    tag_name: tag,
    name: `v${version}`,
    body: releaseNotes,
  });

  try {
    execSync(`gh api repos/${repository}/releases -X POST --input -`, {
      input: payload,
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
