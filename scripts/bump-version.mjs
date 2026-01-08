#!/usr/bin/env node

/**
 * Bump version in csproj file
 * Usage: bun run scripts/bump-version.mjs --bump-type <major|minor|patch> [--dry-run]
 */

import { readFileSync, writeFileSync } from 'fs';

// Simple argument parsing
const args = process.argv.slice(2);
const getArg = (name) => {
  const index = args.indexOf(`--${name}`);
  if (index === -1) return null;
  return args[index + 1];
};
const hasFlag = (name) => args.includes(`--${name}`);

const bumpType = getArg('bump-type');
const dryRun = hasFlag('dry-run');

if (!bumpType || !['major', 'minor', 'patch'].includes(bumpType)) {
  console.error(
    'Usage: bun run scripts/bump-version.mjs --bump-type <major|minor|patch> [--dry-run]'
  );
  process.exit(1);
}

const CSPROJ_PATH = 'src/MyPackage/MyPackage.csproj';

/**
 * Get current version from csproj
 * @returns {{major: number, minor: number, patch: number}}
 */
function getCurrentVersion() {
  const csproj = readFileSync(CSPROJ_PATH, 'utf-8');
  const match = csproj.match(/<Version>(\d+)\.(\d+)\.(\d+)<\/Version>/);

  if (!match) {
    console.error('Error: Could not parse version from csproj');
    process.exit(1);
  }

  return {
    major: parseInt(match[1], 10),
    minor: parseInt(match[2], 10),
    patch: parseInt(match[3], 10),
  };
}

/**
 * Calculate new version based on bump type
 * @param {{major: number, minor: number, patch: number}} current
 * @param {string} bumpType
 * @returns {string}
 */
function calculateNewVersion(current, bumpType) {
  const { major, minor, patch } = current;

  switch (bumpType) {
    case 'major':
      return `${major + 1}.0.0`;
    case 'minor':
      return `${major}.${minor + 1}.0`;
    case 'patch':
      return `${major}.${minor}.${patch + 1}`;
    default:
      throw new Error(`Invalid bump type: ${bumpType}`);
  }
}

/**
 * Update version in csproj file
 * @param {string} newVersion
 */
function updateCsproj(newVersion) {
  let csproj = readFileSync(CSPROJ_PATH, 'utf-8');
  csproj = csproj.replace(
    /<Version>[^<]+<\/Version>/,
    `<Version>${newVersion}</Version>`
  );
  writeFileSync(CSPROJ_PATH, csproj, 'utf-8');
}

try {
  const current = getCurrentVersion();
  const currentStr = `${current.major}.${current.minor}.${current.patch}`;
  const newVersion = calculateNewVersion(current, bumpType);

  console.log(`Current version: ${currentStr}`);
  console.log(`New version: ${newVersion}`);

  if (dryRun) {
    console.log('Dry run - no changes made');
  } else {
    updateCsproj(newVersion);
    console.log('Updated csproj');
  }
} catch (error) {
  console.error('Error:', error.message);
  process.exit(1);
}
