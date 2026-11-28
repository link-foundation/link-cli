#!/usr/bin/env node
/**
 * Changeset validation script.
 *
 * Validates that PR contains a proper changeset file in the .changeset/ directory
 * with the correct JS changesets format.
 *
 * Usage:
 *   bun scripts/validate-changeset.mjs
 */

import { readdirSync, readFileSync, existsSync } from 'fs';
import { isAbsolute, join } from 'path';
import { execFileSync } from 'child_process';
import { printUntrusted } from './github-actions-log.mjs';

// Package name - update this when forking the template
const PACKAGE_NAME = 'my-package';

/**
 * Get list of files changed in current PR.
 * @returns {string[]|null} Changed paths, or null when the diff is unavailable
 */
function getChangedFiles() {
  const baseBranch = process.env.GITHUB_BASE_REF || 'main';

  // A bounded single-branch checkout does not have enough history for a
  // three-dot diff. Try to make the remote base resolvable before asking Git.
  try {
    const gitDirectory = execFileSync('git', ['rev-parse', '--git-dir'], {
      encoding: 'utf8',
    }).trim();
    const fetchArguments = ['fetch', '--no-tags'];
    const shallowFile = join(
      isAbsolute(gitDirectory) ? gitDirectory : join(process.cwd(), gitDirectory),
      'shallow'
    );
    if (existsSync(shallowFile)) {
      fetchArguments.push('--unshallow');
    }
    fetchArguments.push(
      'origin',
      `+refs/heads/${baseBranch}:refs/remotes/origin/${baseBranch}`
    );
    execFileSync('git', fetchArguments, { stdio: 'ignore' });
  } catch {
    // Offline and complete repositories can still have a usable base ref.
  }

  try {
    const output = execFileSync(
      'git',
      ['diff', '--name-only', `origin/${baseBranch}...HEAD`, '--'],
      { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] }
    );
    return output.trim().split('\n').filter(Boolean);
  } catch (error) {
    console.warn('::warning::Could not determine the PR diff; validating every changeset instead.');
    printUntrusted(`Git error: ${error.message.split('\n')[0]}`);
    return null;
  }
}

/**
 * List every changeset when Git cannot safely identify the PR's subset.
 * @param {string} changesetDir - Absolute changeset directory path
 * @returns {string[]} Repository-relative changeset paths
 */
function getAllChangesets(changesetDir) {
  if (!existsSync(changesetDir)) return [];
  return readdirSync(changesetDir)
    .filter((file) => file.endsWith('.md') && file !== 'README.md')
    .map((file) => `.changeset/${file}`);
}

/**
 * Check if PR contains source code changes.
 * @param {string[]} changedFiles - List of changed files
 * @returns {boolean} True if source files were changed
 */
function hasSourceChanges(changedFiles) {
  const sourcePatterns = [
    /^src\//,
    /^tests\//,
    /^examples\//,
    /^scripts\//,
    /^pom\.xml$/,
  ];

  return changedFiles.some((file) =>
    sourcePatterns.some((pattern) => pattern.test(file))
  );
}

/**
 * Validate changeset file content.
 * @param {string} filePath - Path to changeset file
 * @returns {object} Validation result { valid: boolean, type?: string, description?: string, error?: string }
 */
function validateChangesetFile(filePath) {
  try {
    const content = readFileSync(filePath, 'utf-8');

    // Check if changeset has the correct format with frontmatter
    const frontmatterMatch = content.match(/^---\n([\s\S]*?)\n---\n([\s\S]*)$/);
    if (!frontmatterMatch) {
      return {
        valid: false,
        error: 'Changeset must have frontmatter section (---)',
      };
    }

    const frontmatter = frontmatterMatch[1];
    const description = frontmatterMatch[2].trim();

    // Check for version type (major, minor, or patch)
    const versionTypeRegex = new RegExp(
      `^['"]${PACKAGE_NAME.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}['"]:\\s+(major|minor|patch)`,
      'm'
    );
    const versionTypeMatch = frontmatter.match(versionTypeRegex);

    if (!versionTypeMatch) {
      return {
        valid: false,
        error: `Changeset must specify a version type for '${PACKAGE_NAME}' (major, minor, or patch)`,
      };
    }

    // Validate description
    if (!description) {
      return {
        valid: false,
        error: 'Changeset must include a non-empty description after the frontmatter',
      };
    }

    return {
      valid: true,
      type: versionTypeMatch[1],
      description,
    };
  } catch (error) {
    return {
      valid: false,
      error: `Failed to read changeset file: ${error.message}`,
    };
  }
}

/**
 * Main function.
 */
function main() {
  const projectRoot = process.cwd();
  const changesetDir = join(projectRoot, '.changeset');

  console.log('Validating changesets...\n');

  // Get changed files
  const changedFiles = getChangedFiles();
  console.log(`Changed files: ${changedFiles === null ? 'unavailable' : changedFiles.length}`);

  // Check if this is a source change that requires changelog
  if (changedFiles !== null && !hasSourceChanges(changedFiles)) {
    console.log('\nNo source code changes detected. Changeset not required.');
    process.exit(0);
  }

  console.log('Source code changes detected. Checking for changeset...\n');

  // Check for changeset files in changed files
  const changesetChanges = changedFiles === null
    ? getAllChangesets(changesetDir)
    : changedFiles.filter((file) =>
      file.startsWith('.changeset/') &&
      file.endsWith('.md') &&
      !file.endsWith('README.md') &&
      !file.endsWith('config.json')
    );

  if (changesetChanges.length === 0) {
    console.warn('WARNING: No changeset found in this PR.');
    console.warn('\nPlease create a changeset file in .changeset/');
    console.warn('Run: bun scripts/create-manual-changeset.mjs --bump-type patch --description "Your description"');
    console.warn('\nSee .changeset/README.md for format instructions.');

    // Exit with warning (0) not error, to not block PR
    // Change to process.exit(1) if you want to enforce changesets
    process.exit(0);
  }

  // Validate each changeset
  let allValid = true;
  for (const changesetFile of changesetChanges) {
    const changesetPath = join(projectRoot, changesetFile);
    console.log('Validating changeset:');
    printUntrusted(changesetFile);

    const result = validateChangesetFile(changesetPath);
    if (result.valid) {
      console.log(`  OK: ${result.type}`);
      printUntrusted(`${result.description.slice(0, 50)}...`);
    } else {
      console.error('  ERROR: Invalid changeset:');
      printUntrusted(result.error);
      allValid = false;
    }
  }

  if (!allValid) {
    console.error('\nSome changesets have validation errors.');
    process.exit(1);
  }

  console.log('\nAll changesets are valid!');
  process.exit(0);
}

main();
