#!/usr/bin/env node

/**
 * Bump version in csproj, update changelog, and commit changes
 * Used by the CI/CD pipeline for C# releases
 *
 * Usage:
 *   Changeset mode: node scripts/version-and-commit-csharp.mjs --mode changeset
 *   Instant mode:   node scripts/version-and-commit-csharp.mjs --mode instant --bump-type <major|minor|patch> [--description <desc>]
 */

import {
  readFileSync,
  writeFileSync,
  appendFileSync,
  readdirSync,
  existsSync,
  unlinkSync,
} from 'fs';
import { join } from 'path';
import { execSync } from 'child_process';

// Package name must match the package name in the changeset files
const PACKAGE_NAME = 'Foundation.Data.Doublets.Cli';
const CSPROJ_PATH = 'csharp/Foundation.Data.Doublets.Cli/Foundation.Data.Doublets.Cli.csproj';
const CHANGESET_DIR = 'csharp/.changeset';
const CHANGELOG_FILE = 'csharp/CHANGELOG.md';

// Version bump type priority (higher number = higher priority)
const BUMP_PRIORITY = {
  patch: 1,
  minor: 2,
  major: 3,
};

// Simple argument parsing
const args = process.argv.slice(2);
const getArg = (name) => {
  const index = args.indexOf(`--${name}`);
  if (index === -1) return null;
  return args[index + 1] || '';
};

const mode = getArg('mode') || 'instant';
const bumpTypeArg = getArg('bump-type');
const description = getArg('description') || '';

/**
 * Execute a shell command
 * @param {string} command
 * @param {boolean} silent
 * @returns {string}
 */
function exec(command, silent = false) {
  try {
    return execSync(command, { encoding: 'utf-8', stdio: silent ? 'pipe' : 'inherit' });
  } catch (error) {
    if (silent) return '';
    throw error;
  }
}

/**
 * Append to GitHub Actions output file
 * @param {string} key
 * @param {string} value
 */
function setOutput(key, value) {
  const outputFile = process.env.GITHUB_OUTPUT;
  if (outputFile) {
    appendFileSync(outputFile, `${key}=${value}\n`);
  }
  console.log(`Output: ${key}=${value}`);
}

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
 * Update version in csproj
 * @param {string} newVersion
 */
function updateCsproj(newVersion) {
  let csproj = readFileSync(CSPROJ_PATH, 'utf-8');
  csproj = csproj.replace(
    /<Version>[^<]+<\/Version>/,
    `<Version>${newVersion}</Version>`
  );
  writeFileSync(CSPROJ_PATH, csproj, 'utf-8');
  console.log(`Updated csproj to version ${newVersion}`);
}

/**
 * Check if a git tag exists for this version
 * @param {string} version
 * @returns {boolean}
 */
function checkTagExists(version) {
  try {
    exec(`git rev-parse csharp-v${version}`, true);
    return true;
  } catch {
    return false;
  }
}

/**
 * Parse a changeset file and extract its metadata
 * @param {string} filePath
 * @returns {{type: string, description: string} | null}
 */
function parseChangeset(filePath) {
  try {
    const content = readFileSync(filePath, 'utf-8');

    // Extract version type - support both quoted and unquoted package names
    const versionTypeRegex = new RegExp(
      `^['"]?${PACKAGE_NAME.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}['"]?:\\s+(major|minor|patch)`,
      'm'
    );
    const versionTypeMatch = content.match(versionTypeRegex);

    if (!versionTypeMatch) {
      console.warn(`Warning: Could not parse version type from ${filePath}`);
      return null;
    }

    // Extract description
    const parts = content.split('---');
    const desc = parts.length >= 3 ? parts.slice(2).join('---').trim() : '';

    return {
      type: versionTypeMatch[1],
      description: desc,
    };
  } catch (error) {
    console.warn(`Warning: Failed to parse ${filePath}: ${error.message}`);
    return null;
  }
}

/**
 * Get the highest priority bump type
 * @param {string[]} types
 * @returns {string}
 */
function getHighestBumpType(types) {
  let highest = 'patch';
  for (const type of types) {
    if (BUMP_PRIORITY[type] > BUMP_PRIORITY[highest]) {
      highest = type;
    }
  }
  return highest;
}

/**
 * Get changeset files from .changeset directory
 * @returns {string[]}
 */
function getChangesetFiles() {
  if (!existsSync(CHANGESET_DIR)) {
    return [];
  }
  return readdirSync(CHANGESET_DIR).filter(
    (file) =>
      file.endsWith('.md') && file !== 'README.md' && file !== 'config.json'
  );
}

/**
 * Process changesets and return bump type and descriptions
 * @returns {{bumpType: string, descriptions: string[]} | null}
 */
function processChangesets() {
  const files = getChangesetFiles();

  if (files.length === 0) {
    console.log('No changeset files found');
    return null;
  }

  console.log(`Found ${files.length} changeset file(s)`);

  const parsedChangesets = [];
  for (const file of files) {
    const filePath = join(CHANGESET_DIR, file);
    const parsed = parseChangeset(filePath);
    if (parsed) {
      parsedChangesets.push({
        file,
        filePath,
        ...parsed,
      });
    }
  }

  if (parsedChangesets.length === 0) {
    console.log('No valid changesets could be parsed');
    return null;
  }

  const bumpTypes = parsedChangesets.map((c) => c.type);
  const highestBumpType = getHighestBumpType(bumpTypes);
  const descriptions = parsedChangesets
    .filter((c) => c.description)
    .map((c) => c.description);

  console.log(`Bump types found: ${[...new Set(bumpTypes)].join(', ')}`);
  console.log(`Using highest: ${highestBumpType}`);

  return {
    bumpType: highestBumpType,
    descriptions,
  };
}

/**
 * Update CHANGELOG.md with new version entry
 * @param {string} version
 * @param {string[]} descriptions
 */
function updateChangelog(version, descriptions) {
  const dateStr = new Date().toISOString().split('T')[0];
  const content = descriptions.join('\n\n');
  const newEntry = `\n## [${version}] - ${dateStr}\n\n${content}\n`;

  if (existsSync(CHANGELOG_FILE)) {
    let changelog = readFileSync(CHANGELOG_FILE, 'utf-8');
    const lines = changelog.split('\n');
    let insertIndex = -1;

    // Find the first version entry
    for (let i = 0; i < lines.length; i++) {
      if (lines[i].startsWith('## [')) {
        insertIndex = i;
        break;
      }
    }

    if (insertIndex >= 0) {
      lines.splice(insertIndex, 0, newEntry);
      changelog = lines.join('\n');
    } else {
      // No existing version entries, append after header
      changelog += newEntry;
    }

    writeFileSync(CHANGELOG_FILE, changelog, 'utf-8');
    console.log(`Updated CHANGELOG.md with version ${version}`);
  } else {
    // Create new changelog file
    const newChangelog = `# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
${newEntry}`;
    writeFileSync(CHANGELOG_FILE, newChangelog, 'utf-8');
    console.log(`Created CHANGELOG.md with version ${version}`);
  }
}

/**
 * Remove processed changeset files
 */
function removeChangesetFiles() {
  const files = getChangesetFiles();
  for (const file of files) {
    const filePath = join(CHANGESET_DIR, file);
    unlinkSync(filePath);
    console.log(`Removed changeset: ${file}`);
  }
}

try {
  // Configure git
  exec('git config user.name "github-actions[bot]"');
  exec('git config user.email "github-actions[bot]@users.noreply.github.com"');

  let bumpType;
  let descriptions = [];

  if (mode === 'changeset') {
    // Changeset mode: get bump type from changesets
    const result = processChangesets();
    if (!result) {
      console.log('No changesets to process, exiting');
      setOutput('version_committed', 'false');
      setOutput('already_released', 'false');
      process.exit(0);
    }
    bumpType = result.bumpType;
    descriptions = result.descriptions;
  } else if (mode === 'instant') {
    // Instant mode: use provided bump type
    if (!bumpTypeArg || !['major', 'minor', 'patch'].includes(bumpTypeArg)) {
      console.error(
        'Usage: node scripts/version-and-commit-csharp.mjs --mode instant --bump-type <major|minor|patch> [--description <desc>]'
      );
      process.exit(1);
    }
    bumpType = bumpTypeArg;
    if (description) {
      descriptions = [description];
    }
  } else {
    console.error('Invalid mode. Use --mode changeset or --mode instant');
    process.exit(1);
  }

  const current = getCurrentVersion();
  const newVersion = calculateNewVersion(current, bumpType);

  // Check if this version was already released
  if (checkTagExists(newVersion)) {
    console.log(`Tag csharp-v${newVersion} already exists`);
    setOutput('already_released', 'true');
    setOutput('new_version', newVersion);
    process.exit(0);
  }

  // Update version in csproj
  updateCsproj(newVersion);

  // Update changelog if we have descriptions
  if (descriptions.length > 0) {
    updateChangelog(newVersion, descriptions);
  }

  // Remove changeset files (only in changeset mode)
  if (mode === 'changeset') {
    removeChangesetFiles();
  }

  // Stage all changed files
  exec(`git add ${CSPROJ_PATH} ${CHANGELOG_FILE} ${CHANGESET_DIR}/`);

  // Check if there are changes to commit
  try {
    exec('git diff --cached --quiet', true);
    // No changes to commit
    console.log('No changes to commit');
    setOutput('version_committed', 'false');
    setOutput('new_version', newVersion);
    process.exit(0);
  } catch {
    // There are changes to commit (git diff exits with 1 when there are differences)
  }

  // Commit changes
  const commitMsg = description
    ? `chore(csharp): release v${newVersion}\n\n${description}`
    : `chore(csharp): release v${newVersion}`;
  exec(`git commit -m "${commitMsg.replace(/"/g, '\\"')}"`);
  console.log(`Committed version ${newVersion}`);

  // Create tag
  const tagMsg = description
    ? `C# Release v${newVersion}\n\n${description}`
    : `C# Release v${newVersion}`;
  exec(`git tag -a csharp-v${newVersion} -m "${tagMsg.replace(/"/g, '\\"')}"`);
  console.log(`Created tag csharp-v${newVersion}`);

  // Push changes and tag
  exec('git push');
  exec('git push --tags');
  console.log('Pushed changes and tags');

  setOutput('version_committed', 'true');
  setOutput('new_version', newVersion);
} catch (error) {
  console.error('Error:', error.message);
  process.exit(1);
}
