#!/usr/bin/env node

/**
 * Detect code changes for CI/CD pipeline
 *
 * This script detects what types of files have changed between two commits
 * and outputs the results for use in GitHub Actions workflow conditions.
 *
 * Key behavior:
 * - For PRs: compares PR head against base branch
 * - For pushes: compares HEAD against HEAD^
 * - Excludes certain folders and file types from "code changes" detection
 *
 * Excluded from code changes (don't require changesets):
 * - Markdown files (*.md) in any folder
 * - .changeset/ folder (changeset metadata)
 * - changelog.d/ folder (changelog fragments)
 * - docs/ folder (documentation)
 * - experiments/ folder (experimental scripts)
 * - examples/ folder (example scripts)
 *
 * Usage:
 *   bun run scripts/detect-code-changes.mjs
 *   node scripts/detect-code-changes.mjs
 *
 * Environment variables (set by GitHub Actions):
 *   - GITHUB_EVENT_NAME: 'pull_request' or 'push'
 *   - GITHUB_BASE_SHA: Base commit SHA for PR
 *   - GITHUB_HEAD_SHA: Head commit SHA for PR
 *
 * Outputs (written to GITHUB_OUTPUT):
 *   C# outputs:
 *   - cs-changed: 'true' if any .cs files changed
 *   - csproj-changed: 'true' if any .csproj files changed
 *   - sln-changed: 'true' if any .sln files changed
 *   - props-changed: 'true' if any .props files changed (Directory.Build.props etc.)
 *
 *   Rust outputs:
 *   - rs-changed: 'true' if any .rs files changed
 *   - toml-changed: 'true' if any .toml files changed (Cargo.toml, etc.)
 *
 *   Common outputs:
 *   - mjs-changed: 'true' if any .mjs files changed (scripts)
 *   - docs-changed: 'true' if any .md files changed
 *   - workflow-changed: 'true' if any .github/workflows/ files changed
 *   - any-code-changed: 'true' if any code files changed (excludes docs, changesets, experiments, examples)
 *   - csharp-code-changed: 'true' if any C# code files changed
 *   - rust-code-changed: 'true' if any Rust code files changed
 */

import { execSync } from 'child_process';
import { appendFileSync } from 'fs';

/**
 * Execute a shell command and return trimmed output
 * @param {string} command - The command to execute
 * @returns {string} - The trimmed command output
 */
function exec(command) {
  try {
    return execSync(command, { encoding: 'utf-8' }).trim();
  } catch (error) {
    console.error(`Error executing command: ${command}`);
    console.error(error.message);
    return '';
  }
}

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
 * Get the list of changed files between two commits
 * @returns {string[]} Array of changed file paths
 */
function getChangedFiles() {
  const eventName = process.env.GITHUB_EVENT_NAME || 'local';

  if (eventName === 'pull_request') {
    const baseSha = process.env.GITHUB_BASE_SHA;
    const headSha = process.env.GITHUB_HEAD_SHA;

    if (baseSha && headSha) {
      console.log(`Comparing PR: ${baseSha}...${headSha}`);
      try {
        // Ensure we have the base commit
        try {
          execSync(`git cat-file -e ${baseSha}`, { stdio: 'ignore' });
        } catch {
          console.log('Base commit not available locally, attempting fetch...');
          execSync(`git fetch origin ${baseSha}`, { stdio: 'inherit' });
        }
        const output = exec(`git diff --name-only ${baseSha} ${headSha}`);
        return output ? output.split('\n').filter(Boolean) : [];
      } catch (error) {
        console.error(`Git diff failed: ${error.message}`);
      }
    }
  }

  // For push events or fallback
  console.log('Comparing HEAD^ to HEAD');
  try {
    const output = exec('git diff --name-only HEAD^ HEAD');
    return output ? output.split('\n').filter(Boolean) : [];
  } catch {
    // If HEAD^ doesn't exist (first commit), list all files in HEAD
    console.log('HEAD^ not available, listing all files in HEAD');
    const output = exec('git ls-tree --name-only -r HEAD');
    return output ? output.split('\n').filter(Boolean) : [];
  }
}

/**
 * Check if a file should be excluded from code changes detection
 * @param {string} filePath - The file path to check
 * @returns {boolean} True if the file should be excluded
 */
function isExcludedFromCodeChanges(filePath) {
  // Exclude markdown files in any folder
  if (filePath.endsWith('.md')) {
    return true;
  }

  // Exclude specific folders from code changes
  const excludedFolders = ['.changeset/', 'changelog.d/', 'docs/', 'experiments/', 'examples/'];

  for (const folder of excludedFolders) {
    if (filePath.startsWith(folder)) {
      return true;
    }
  }

  return false;
}

/**
 * Check if a file is in the csharp folder
 * @param {string} filePath - The file path to check
 * @returns {boolean} True if the file is in csharp folder
 */
function isCsharpFile(filePath) {
  return filePath.startsWith('csharp/');
}

/**
 * Check if a file is in the rust folder
 * @param {string} filePath - The file path to check
 * @returns {boolean} True if the file is in rust folder
 */
function isRustFile(filePath) {
  return filePath.startsWith('rust/');
}

/**
 * Main function to detect changes
 */
function detectChanges() {
  console.log('Detecting file changes for CI/CD...\n');

  const changedFiles = getChangedFiles();

  console.log('Changed files:');
  if (changedFiles.length === 0) {
    console.log('  (none)');
  } else {
    changedFiles.forEach((file) => console.log(`  ${file}`));
  }
  console.log('');

  // Detect .cs file changes (C# source files)
  const csChanged = changedFiles.some((file) => file.endsWith('.cs'));
  setOutput('cs-changed', csChanged ? 'true' : 'false');

  // Detect .csproj file changes (project files)
  const csprojChanged = changedFiles.some((file) => file.endsWith('.csproj'));
  setOutput('csproj-changed', csprojChanged ? 'true' : 'false');

  // Detect .sln file changes (solution files)
  const slnChanged = changedFiles.some((file) => file.endsWith('.sln'));
  setOutput('sln-changed', slnChanged ? 'true' : 'false');

  // Detect .props file changes (Directory.Build.props, etc.)
  const propsChanged = changedFiles.some((file) => file.endsWith('.props'));
  setOutput('props-changed', propsChanged ? 'true' : 'false');

  // Detect .rs file changes (Rust source files)
  const rsChanged = changedFiles.some((file) => file.endsWith('.rs'));
  setOutput('rs-changed', rsChanged ? 'true' : 'false');

  // Detect .toml file changes (Cargo.toml, etc.)
  const tomlChanged = changedFiles.some((file) => file.endsWith('.toml'));
  setOutput('toml-changed', tomlChanged ? 'true' : 'false');

  // Detect .mjs file changes (scripts)
  const mjsChanged = changedFiles.some((file) => file.endsWith('.mjs'));
  setOutput('mjs-changed', mjsChanged ? 'true' : 'false');

  // Detect documentation changes (any .md file)
  const docsChanged = changedFiles.some((file) => file.endsWith('.md'));
  setOutput('docs-changed', docsChanged ? 'true' : 'false');

  // Detect workflow changes
  const workflowChanged = changedFiles.some((file) =>
    file.startsWith('.github/workflows/')
  );
  setOutput('workflow-changed', workflowChanged ? 'true' : 'false');

  // Detect code changes (excluding docs, changesets, experiments, examples folders, and markdown files)
  const codeChangedFiles = changedFiles.filter(
    (file) => !isExcludedFromCodeChanges(file)
  );

  console.log('\nFiles considered as code changes:');
  if (codeChangedFiles.length === 0) {
    console.log('  (none)');
  } else {
    codeChangedFiles.forEach((file) => console.log(`  ${file}`));
  }
  console.log('');

  // Check if any code files changed (all languages)
  const codePattern = /\.(cs|csproj|sln|props|rs|toml|mjs|json|yml|yaml)$|\.github\/workflows\//;
  const codeChanged = codeChangedFiles.some((file) => codePattern.test(file));
  setOutput('any-code-changed', codeChanged ? 'true' : 'false');

  // Check if C# code files changed
  const csharpPattern = /\.(cs|csproj|sln|props)$/;
  const csharpCodeChangedFiles = codeChangedFiles.filter((file) => isCsharpFile(file) || csharpPattern.test(file));
  const csharpCodeChanged = csharpCodeChangedFiles.some((file) => csharpPattern.test(file));
  setOutput('csharp-code-changed', csharpCodeChanged ? 'true' : 'false');

  // Check if Rust code files changed
  const rustPattern = /\.(rs|toml)$/;
  const rustCodeChangedFiles = codeChangedFiles.filter((file) => isRustFile(file) || rustPattern.test(file));
  const rustCodeChanged = rustCodeChangedFiles.some((file) => rustPattern.test(file));
  setOutput('rust-code-changed', rustCodeChanged ? 'true' : 'false');

  console.log('\nChange detection completed.');
}

// Run the detection
detectChanges();
