#!/usr/bin/env node

/**
 * Bump version in csproj, update changelog, and commit changes
 * Used by the CI/CD pipeline for C# releases
 *
 * Usage:
 *   Changeset mode: node csharp/scripts/version-and-commit.mjs --mode changeset
 *   Instant mode:   node csharp/scripts/version-and-commit.mjs --mode instant --bump-type <major|minor|patch> [--description <desc>]
 */

import {
  readFileSync,
  writeFileSync,
  appendFileSync,
  readdirSync,
  existsSync,
  unlinkSync,
} from 'fs';
import { join, resolve } from 'path';
import { execFileSync, execSync } from 'child_process';
import { fileURLToPath } from 'url';

// Package name must match the package name in the changeset files
const PACKAGE_NAME = 'Foundation.Data.Doublets.Cli';
const CSPROJ_PATH = 'csharp/Foundation.Data.Doublets.Cli/Foundation.Data.Doublets.Cli.csproj';
// The library project ships as a separate NuGet package
// (`Foundation.Data.Doublets.Cli`) and must share the CLI tool's release
// version so both packages reference each other consistently. The path
// is optional — when the library csproj does not exist (e.g. inside unit
// tests for this script), the bump silently skips the second update.
const LIBRARY_CSPROJ_PATH = 'csharp/Foundation.Data.Doublets.Cli.Library/Foundation.Data.Doublets.Cli.Library.csproj';
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
  return execSync(command, { encoding: 'utf-8', stdio: silent ? 'pipe' : 'inherit' });
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
 * Update version in a csproj at the given path.
 * @param {string} path
 * @param {string} newVersion
 */
function updateCsprojAt(path, newVersion) {
  let csproj = readFileSync(path, 'utf-8');
  csproj = csproj.replace(
    /<Version>[^<]+<\/Version>/,
    `<Version>${newVersion}</Version>`
  );
  writeFileSync(path, csproj, 'utf-8');
  console.log(`Updated ${path} to version ${newVersion}`);
}

/**
 * Update version in the CLI csproj and, when present, the library csproj.
 * @param {string} newVersion
 */
function updateCsproj(newVersion) {
  updateCsprojAt(CSPROJ_PATH, newVersion);
  if (existsSync(LIBRARY_CSPROJ_PATH)) {
    updateCsprojAt(LIBRARY_CSPROJ_PATH, newVersion);
  }
}

/**
 * Check if a git tag exists for this version
 * @param {string} version
 * @returns {boolean}
 */
function checkTagExists(version) {
  try {
    exec(`git rev-parse --verify --quiet refs/tags/csharp-v${version}`, true);
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

const CHANGELOG_HEADER = `# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).
`;

/**
 * Insert a release entry above the newest one, separated by exactly one blank
 * line. Splicing an entry that started with "\n" after the existing blank
 * lines used to add one more empty line below the header on every release.
 * @param {string | null} changelog existing CHANGELOG.md content, or null
 * @param {string} entry the "## [version] - date" section
 * @returns {string}
 */
export function insertChangelogEntry(changelog, entry) {
  const section = entry.trim();
  const content = changelog ?? CHANGELOG_HEADER;
  const match = /^## \[/m.exec(content);
  if (!match) {
    return `${content.trimEnd()}\n\n${section}\n`;
  }
  const before = content.slice(0, match.index).trimEnd();
  const after = content.slice(match.index);
  return `${before}\n\n${section}\n\n${after}`;
}

/**
 * Update CHANGELOG.md with new version entry
 * @param {string} version
 * @param {string[]} descriptions
 */
function updateChangelog(version, descriptions) {
  const dateStr = new Date().toISOString().split('T')[0];
  const entry = `## [${version}] - ${dateStr}\n\n${descriptions.join('\n\n')}`;
  const existing = existsSync(CHANGELOG_FILE) ? readFileSync(CHANGELOG_FILE, 'utf-8') : null;
  writeFileSync(CHANGELOG_FILE, insertChangelogEntry(existing, entry), 'utf-8');
  console.log(`${existing === null ? 'Created' : 'Updated'} CHANGELOG.md with version ${version}`);
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

/**
 * Not every failed `git push` is a lost race: a repository ruleset rejection
 * also says "rejected", and rebasing against a policy refusal can never
 * succeed, so the failure is classified before retrying (same patterns as the
 * AI driven development pipeline templates).
 */
const REPOSITORY_RULE_PATTERNS = [
  'gh006',
  'gh013',
  'repository rule violations',
  'changes must be made through a pull request',
  'protected branch',
  'push declined',
];
const NON_FAST_FORWARD_PATTERNS = [
  '[rejected]',
  'non-fast-forward',
  'fetch first',
  'updates were rejected',
];

/**
 * @param {string} output combined git push output
 * @returns {'repository-rules' | 'lost-race' | 'other'}
 */
export function classifyPushFailure(output) {
  const haystack = String(output).toLowerCase();
  if (REPOSITORY_RULE_PATTERNS.some((pattern) => haystack.includes(pattern))) {
    return 'repository-rules';
  }
  if (NON_FAST_FORWARD_PATTERNS.some((pattern) => haystack.includes(pattern))) {
    return 'lost-race';
  }
  return 'other';
}

/**
 * Run git without a shell, so release descriptions containing quotes,
 * backticks or `$(...)` are passed verbatim instead of being expanded.
 * @param {string[]} gitArgs
 * @returns {string}
 */
function git(gitArgs) {
  if (process.env.VERSION_AND_COMMIT_DEBUG) {
    console.error(`[debug] git ${gitArgs.join(' ')}`);
  }
  return execFileSync('git', gitArgs, { encoding: 'utf-8', stdio: 'pipe' }).trim();
}

/** @param {unknown} error */
function gitErrorOutput(error) {
  const e = /** @type {{stdout?: string, stderr?: string, message?: string}} */ (error);
  return [e.stderr, e.stdout, e.message].filter(Boolean).join('\n');
}

/**
 * Sync with the remote before any file is touched: `git rebase` refuses a
 * dirty index, and the bump must start from the newest state of the branch
 * (the Rust release may have pushed its own version commit meanwhile).
 * @param {string} branch
 */
function syncWithRemote(branch) {
  try {
    git(['fetch', 'origin', branch]);
  } catch (error) {
    console.warn(`Warning: could not fetch origin/${branch}: ${gitErrorOutput(error)}`);
    return;
  }
  const behind = Number(git(['rev-list', '--count', `HEAD..origin/${branch}`]));
  if (behind > 0) {
    console.log(`Local branch is behind origin/${branch} by ${behind} commit(s), rebasing...`);
    git(['rebase', `origin/${branch}`]);
  }
}

/**
 * Push the release commit, retrying only genuine non-fast-forward races.
 * @param {string} branch
 * @param {number} maxAttempts
 */
function pushReleaseCommit(branch, maxAttempts = 3) {
  for (let attempt = 1; ; attempt++) {
    try {
      git(['push', 'origin', `HEAD:${branch}`]);
      return;
    } catch (error) {
      const output = gitErrorOutput(error);
      const kind = classifyPushFailure(output);
      if (kind === 'repository-rules') {
        throw new Error(
          `The push to '${branch}' was declined by a repository rule (GH006/GH013). ` +
            `Rebasing cannot change repository policy: allow the release bot to push to '${branch}' ` +
            `or release through a pull request.\n${output}`
        );
      }
      if (kind === 'other' || attempt >= maxAttempts) {
        throw new Error(`git push failed after ${attempt} attempt(s):\n${output}`);
      }
      console.log(
        `Push rejected as non-fast-forward (attempt ${attempt}/${maxAttempts}); pulling with rebase and retrying...`
      );
      try {
        git(['pull', '--rebase', 'origin', branch]);
      } catch (rebaseError) {
        try {
          git(['rebase', '--abort']);
        } catch {
          // Nothing to abort.
        }
        throw new Error(`git pull --rebase failed:\n${gitErrorOutput(rebaseError)}`);
      }
    }
  }
}

function main() {
  // Configure git
  git(['config', 'user.name', 'github-actions[bot]']);
  git(['config', 'user.email', 'github-actions[bot]@users.noreply.github.com']);

  // A detached checkout reports "HEAD"; fall back to the workflow's branch.
  let branch = '';
  try {
    branch = git(['rev-parse', '--abbrev-ref', 'HEAD']);
  } catch {
    // Unborn or detached HEAD: use the fallback below.
  }
  if (!branch || branch === 'HEAD') {
    branch = process.env.GITHUB_REF_NAME || 'main';
  }
  syncWithRemote(branch);

  let bumpType;
  let descriptions = [];

  if (mode === 'changeset') {
    // Changeset mode: get bump type from changesets
    const result = processChangesets();
    if (!result) {
      console.log('No changesets to process, exiting');
      setOutput('version_committed', 'false');
      setOutput('already_released', 'false');
      return;
    }
    bumpType = result.bumpType;
    descriptions = result.descriptions;
  } else if (mode === 'instant') {
    // Instant mode: use provided bump type
    if (!bumpTypeArg || !['major', 'minor', 'patch'].includes(bumpTypeArg)) {
      console.error(
        'Usage: node csharp/scripts/version-and-commit.mjs --mode instant --bump-type <major|minor|patch> [--description <desc>]'
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
  const tagName = `csharp-v${newVersion}`;

  // Check if this version was already released
  if (checkTagExists(newVersion)) {
    console.log(`Tag ${tagName} already exists`);
    setOutput('already_released', 'true');
    setOutput('new_version', newVersion);
    return;
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

  // Stage all changed files; `-A` also records the removed changesets.
  const stagedPaths = [CSPROJ_PATH];
  if (existsSync(CHANGELOG_FILE)) {
    stagedPaths.push(CHANGELOG_FILE);
  }
  if (existsSync(LIBRARY_CSPROJ_PATH)) {
    stagedPaths.push(LIBRARY_CSPROJ_PATH);
  }
  git(['add', '--', ...stagedPaths]);
  git(['add', '-A', '--', CHANGESET_DIR]);

  // The csproj bump always changes a file, so an empty index means the
  // release write was lost: fail instead of publishing an uncommitted version.
  const staged = git(['diff', '--cached', '--name-only']);
  if (!staged) {
    throw new Error(
      `Nothing was staged for ${tagName}; refusing to publish a version that is not committed`
    );
  }
  console.log(`Staged release files:\n${staged}`);

  // Commit changes
  const commitMsg = description
    ? `chore(csharp): release v${newVersion}\n\n${description}`
    : `chore(csharp): release v${newVersion}`;
  git(['commit', '-m', commitMsg]);
  console.log(`Committed version ${newVersion}`);

  pushReleaseCommit(branch);

  // Tag only after the commit reached the remote, so a rebase retry can never
  // leave the tag on an orphaned commit, and push exactly this tag.
  const tagMsg = description
    ? `C# Release v${newVersion}\n\n${description}`
    : `C# Release v${newVersion}`;
  git(['tag', '-a', tagName, '-m', tagMsg]);
  console.log(`Created tag ${tagName}`);
  git(['push', 'origin', tagName]);
  console.log(`Pushed release commit and tag ${tagName}`);

  setOutput('version_committed', 'true');
  setOutput('new_version', newVersion);
}

const invokedDirectly =
  process.argv[1] && fileURLToPath(import.meta.url) === resolve(process.argv[1]);

if (invokedDirectly) {
  try {
    main();
  } catch (error) {
    console.error(`::error title=C# release failed::${gitErrorOutput(error)}`);
    process.exit(1);
  }
}
