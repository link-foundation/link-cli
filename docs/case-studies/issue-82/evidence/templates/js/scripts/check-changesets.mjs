#!/usr/bin/env node

/**
 * Check for pending changeset files
 *
 * This script checks for pending changeset files in the .changeset directory
 * and outputs the count and status for use in GitHub Actions workflow conditions.
 *
 * Usage:
 *   node scripts/check-changesets.mjs
 *
 * Outputs (written to GITHUB_OUTPUT):
 *   - has_changesets: 'true' if there are pending changesets
 *   - changeset_count: number of changeset files found
 */

import { readdirSync, existsSync, appendFileSync } from 'fs';

const CHANGESET_DIR = '.changeset';

/**
 * Write output to GitHub Actions output file
 * @param {string} name - Output name
 * @param {string} value - Output value
 */
function setOutput(name, value) {
  const outputFile = process.env.GITHUB_OUTPUT;
  if (outputFile) {
    appendFileSync(outputFile, `${name}=${value}\n`);
  }
  console.log(`${name}=${value}`);
}

/**
 * Count changeset files in the .changeset directory
 * @returns {number} Number of changeset files found
 */
function countChangesetFiles() {
  if (!existsSync(CHANGESET_DIR)) {
    return 0;
  }

  const files = readdirSync(CHANGESET_DIR);
  // Filter to only count .md files, excluding README.md
  const changesetFiles = files.filter(
    (file) => file.endsWith('.md') && file !== 'README.md'
  );

  return changesetFiles.length;
}

/**
 * Main function to check for changesets
 */
function checkChangesets() {
  console.log('Checking for pending changeset files...\n');

  const changesetCount = countChangesetFiles();

  console.log(`Found ${changesetCount} changeset file(s)`);

  setOutput('has_changesets', changesetCount > 0 ? 'true' : 'false');
  setOutput('changeset_count', String(changesetCount));
}

// Run the check
checkChangesets();
