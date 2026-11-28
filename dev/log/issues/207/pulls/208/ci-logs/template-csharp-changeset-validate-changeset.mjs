#!/usr/bin/env bun

/**
 * Validate changeset for CI - ensures exactly one valid changeset is added by the PR
 *
 * Key behavior:
 * - Requires one added changeset and validates every pending changeset
 * - Uses git diff to compare PR head against base branch
 * - Validates that the PR adds exactly one changeset with proper format
 * - Falls back to checking all changesets for local development
 *
 * IMPORTANT: Update the package name below to match your csproj
 */

import { execFileSync } from 'child_process';
import { readFileSync, readdirSync, existsSync } from 'fs';
import { join } from 'path';
import { parseChangesetFile } from './parse-changeset.mjs';
import { printUntrusted } from './print-untrusted.mjs';

// Package name must match the package name in the changeset files
const PACKAGE_NAME = 'MyPackage';
const CSHARP_ROOT = String(process.env.CSHARP_ROOT ?? '')
  .trim()
  .replaceAll('\\', '/')
  .replace(/\/+$/u, '');
const CHANGESET_DIR =
  CSHARP_ROOT && CSHARP_ROOT !== '.' ? `${CSHARP_ROOT}/.changeset` : '.changeset';

/**
 * Ensure a git commit is available locally, fetching if necessary
 * @param {string} sha The commit SHA to check
 */
function ensureCommitAvailable(sha) {
  try {
    execFileSync('git', ['cat-file', '-e', sha], { stdio: 'ignore' });
  } catch {
    console.log('Base commit not available locally, attempting fetch...');
    try {
      execFileSync('git', ['fetch', 'origin', sha], { stdio: 'inherit' });
    } catch {
      execFileSync('git', ['fetch', 'origin'], { stdio: 'inherit' });
    }
  }
}

/**
 * Parse git diff output and extract added changeset files
 * @param {string} diffOutput Output from git diff --name-status
 * @returns {string[]} Array of added changeset file names
 */
function parseAddedChangesets(diffOutput) {
  const addedChangesets = [];
  for (const line of diffOutput.trim().split('\n')) {
    if (!line) {
      continue;
    }
    const [status, filePath] = line.split('\t');
    if (
      status === 'A' &&
      filePath.startsWith(`${CHANGESET_DIR}/`) &&
      filePath.endsWith('.md') &&
      !filePath.endsWith('README.md')
    ) {
      addedChangesets.push(filePath.replace(`${CHANGESET_DIR}/`, ''));
    }
  }
  return addedChangesets;
}

/**
 * Try to get changesets using explicit SHA comparison
 * @param {string} baseSha Base commit SHA
 * @param {string} headSha Head commit SHA
 * @returns {string[] | null} Array of changeset files or null if failed
 */
function tryExplicitShaComparison(baseSha, headSha) {
  if (![baseSha, headSha].every((sha) => /^[a-f0-9]{40,64}$/i.test(sha)))
    throw Error('Invalid PR commit identity');
  console.log(`Comparing ${baseSha}...${headSha}`);
  try {
    ensureCommitAvailable(baseSha);
    const diffOutput = execFileSync(
      'git',
      ['diff', '--name-status', baseSha, headSha, '--'],
      {
        encoding: 'utf-8',
      },
    );
    return parseAddedChangesets(diffOutput);
  } catch (error) {
    console.log(`Git diff with explicit SHAs failed: ${error.message}`);
    return null;
  }
}

/**
 * Try to get changesets using base branch comparison
 * @param {string} prBase Base branch name
 * @returns {string[] | null} Array of changeset files or null if failed
 */
function tryBaseBranchComparison(prBase) {
  execFileSync('git', ['check-ref-format', `refs/heads/${prBase}`], {
    stdio: 'ignore',
  });
  console.log(`Comparing against base branch: ${prBase}`);
  try {
    try {
      execFileSync('git', ['fetch', 'origin', prBase], { stdio: 'inherit' });
    } catch {
      // Ignore fetch errors, we might already have it
    }
    const diffOutput = execFileSync(
      'git',
      ['diff', '--name-status', `origin/${prBase}...HEAD`, '--'],
      {
        encoding: 'utf-8',
      },
    );
    return parseAddedChangesets(diffOutput);
  } catch (error) {
    console.log(`Git diff with base ref failed: ${error.message}`);
    return null;
  }
}

/**
 * Fallback: get all changesets in directory
 * @returns {string[]} Array of all changeset file names
 */
function getAllChangesets() {
  console.log(
    'Warning: Could not determine PR diff, checking all changesets in directory',
  );
  if (!existsSync(CHANGESET_DIR)) {
    return [];
  }
  return readdirSync(CHANGESET_DIR).filter(
    (file) => file.endsWith('.md') && file !== 'README.md' && file !== 'config.json',
  );
}

/**
 * Get changeset files added in the current PR using git diff
 * @returns {string[]} Array of added changeset file names
 */
function getAddedChangesetFiles() {
  const baseSha = process.env.GITHUB_BASE_SHA || process.env.BASE_SHA;
  const headSha = process.env.GITHUB_HEAD_SHA || process.env.HEAD_SHA;

  // Try explicit SHAs first
  if (baseSha && headSha) {
    const result = tryExplicitShaComparison(baseSha, headSha);
    if (result !== null) {
      return result;
    }
  }

  // Try base branch comparison
  const prBase = process.env.GITHUB_BASE_REF;
  if (prBase) {
    const result = tryBaseBranchComparison(prBase);
    if (result !== null) {
      return result;
    }
  }

  if (process.env.GITHUB_EVENT_NAME === 'pull_request' || baseSha || headSha || prBase)
    throw Error('Cannot verify the PR diff; changeset validation is incomplete');
  return getAllChangesets();
}

/**
 * Validate a single changeset file
 * @param {string} filePath Full path to the changeset file
 * @returns {{valid: boolean, type?: string, description?: string, error?: string}}
 */
function validateChangesetFile(filePath) {
  try {
    return { valid: true, ...parseChangesetFile(filePath, PACKAGE_NAME) };
  } catch (error) {
    return {
      valid: false,
      error: `Failed to read changeset file: ${error.message}`,
    };
  }
}

try {
  if (process.env.PR_AUTHOR === 'dependabot[bot]') {
    console.log('Skipping: PR opened by dependabot[bot]');
    process.exit(0);
  }
  console.log('Validating changesets added by this PR...');

  // Get changeset files added in this PR
  const addedChangesetFiles = getAddedChangesetFiles();
  const changesetCount = addedChangesetFiles.length;

  console.log(`Found ${changesetCount} changeset file(s) added by this PR`);
  if (changesetCount > 0) {
    console.log('Added changesets:');
    addedChangesetFiles.forEach((file) => console.log(`  - ${file}`));
  }

  // Ensure exactly one changeset file was added
  if (changesetCount === 0) {
    console.error(
      `::error::No changeset found in this PR. Please add a changeset file in ${CHANGESET_DIR}/ directory.`,
    );
    console.error(
      `\nTo create a changeset, add a file like ${CHANGESET_DIR}/my-change.md with:`,
    );
    console.error(`---\n'${PACKAGE_NAME}': patch\n---\n\nDescription of your changes`);
    process.exit(1);
  } else if (changesetCount > 1) {
    console.error(
      `::error::Multiple changesets found in this PR (${changesetCount}). Each PR should add exactly ONE changeset.`,
    );
    console.error('::error::Found changeset files added by this PR:');
    addedChangesetFiles.forEach((file) => console.error(`  ${file}`));
    console.error(
      '\n::error::Please combine these into a single changeset or remove the extras.',
    );
    process.exit(1);
  }

  // Validate the single changeset file
  const changesetFile = join(CHANGESET_DIR, addedChangesetFiles[0]);
  console.log(`Validating changeset: ${changesetFile}`);

  const validation = validateChangesetFile(changesetFile);

  if (!validation.valid) {
    console.error(`::error::${validation.error}`);
    console.error(`\nFile content of ${changesetFile}:`);
    try {
      printUntrusted(readFileSync(changesetFile, 'utf-8'), console.error);
    } catch {
      console.error('(could not read file)');
    }
    process.exit(1);
  }

  for (const file of readdirSync(CHANGESET_DIR).filter(
    (f) => f.endsWith('.md') && f !== 'README.md',
  )) {
    parseChangesetFile(join(CHANGESET_DIR, file), PACKAGE_NAME);
  }

  console.log('Changeset validation passed');
  console.log(`   Type: ${validation.type}`);
  console.log('   Description:');
  printUntrusted(validation.description);
} catch (error) {
  console.error('Error during changeset validation:', error.message);
  if (process.env.DEBUG) {
    console.error('Stack trace:', error.stack);
  }
  process.exit(1);
}
