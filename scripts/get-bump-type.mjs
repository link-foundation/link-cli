#!/usr/bin/env node

/**
 * Parse changelog fragments and determine version bump type
 *
 * This script reads changeset fragments from changelog.d/ and determines
 * the version bump type based on the frontmatter in each fragment.
 *
 * Fragment format:
 * ---
 * bump: patch|minor|major
 * ---
 *
 * ### Added
 * - Your changes here
 *
 * Usage: node scripts/get-bump-type.mjs [--default <patch|minor|major>]
 *
 * Uses link-foundation libraries:
 * - use-m: Dynamic package loading without package.json dependencies
 * - lino-arguments: Unified configuration from CLI args, env vars, and .lenv files
 */

import { readFileSync, readdirSync, existsSync, appendFileSync } from 'fs';
import { join } from 'path';

// Load use-m dynamically
const { use } = eval(
  await (await fetch('https://unpkg.com/use-m/use.js')).text()
);

// Import lino-arguments for CLI argument parsing
const { makeConfig } = await use('lino-arguments');

// Parse CLI arguments
const config = makeConfig({
  yargs: ({ yargs, getenv }) =>
    yargs
      .option('default', {
        type: 'string',
        default: getenv('DEFAULT_BUMP', 'patch'),
        describe: 'Default bump type if no fragments specify one',
        choices: ['major', 'minor', 'patch'],
      }),
});

const { default: defaultBump } = config;

const CHANGELOG_DIR = 'changelog.d';

// Bump type priority (higher = more significant)
const BUMP_PRIORITY = {
  patch: 1,
  minor: 2,
  major: 3,
};

/**
 * Parse frontmatter from a markdown file
 * @param {string} content - File content
 * @returns {{bump?: string, content: string}}
 */
function parseFrontmatter(content) {
  const frontmatterMatch = content.match(/^---\s*\n([\s\S]*?)\n---\s*\n([\s\S]*)$/);

  if (!frontmatterMatch) {
    return { content };
  }

  const frontmatter = frontmatterMatch[1];
  const body = frontmatterMatch[2];

  // Parse YAML-like frontmatter (simple key: value format)
  const data = {};
  for (const line of frontmatter.split('\n')) {
    const match = line.match(/^\s*(\w+)\s*:\s*(.+?)\s*$/);
    if (match) {
      data[match[1]] = match[2];
    }
  }

  return { ...data, content: body };
}

/**
 * Get all changelog fragments and determine bump type
 * @returns {{bumpType: string, fragmentCount: number}}
 */
function determineBumpType() {
  if (!existsSync(CHANGELOG_DIR)) {
    console.log(`No ${CHANGELOG_DIR} directory found`);
    return { bumpType: defaultBump, fragmentCount: 0 };
  }

  const files = readdirSync(CHANGELOG_DIR)
    .filter((f) => f.endsWith('.md') && f !== 'README.md')
    .sort();

  if (files.length === 0) {
    console.log('No changelog fragments found');
    return { bumpType: defaultBump, fragmentCount: 0 };
  }

  let highestPriority = 0;
  let highestBumpType = defaultBump;

  for (const file of files) {
    const content = readFileSync(join(CHANGELOG_DIR, file), 'utf-8');
    const { bump } = parseFrontmatter(content);

    if (bump && BUMP_PRIORITY[bump]) {
      const priority = BUMP_PRIORITY[bump];
      if (priority > highestPriority) {
        highestPriority = priority;
        highestBumpType = bump;
      }
      console.log(`Fragment ${file}: bump=${bump}`);
    } else {
      console.log(`Fragment ${file}: no bump specified, using default`);
    }
  }

  return { bumpType: highestBumpType, fragmentCount: files.length };
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
  // Also log for visibility
  console.log(`Output: ${key}=${value}`);
}

try {
  const { bumpType, fragmentCount } = determineBumpType();

  console.log(`\nDetermined bump type: ${bumpType} (from ${fragmentCount} fragment(s))`);

  setOutput('bump_type', bumpType);
  setOutput('fragment_count', String(fragmentCount));
  setOutput('has_fragments', fragmentCount > 0 ? 'true' : 'false');

} catch (error) {
  console.error('Error:', error.message);
  process.exit(1);
}
