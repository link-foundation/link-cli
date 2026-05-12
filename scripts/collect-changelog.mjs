#!/usr/bin/env node

/**
 * Collect changelog fragments into CHANGELOG.md
 * This script collects all .md files from changelog.d/ (except README.md)
 * and prepends them to CHANGELOG.md, then removes the processed fragments.
 */

import {
  readFileSync,
  writeFileSync,
  readdirSync,
  unlinkSync,
  existsSync,
} from 'fs';
import { join } from 'path';

const INSERT_MARKER = '<!-- changelog-insert-here -->';

/**
 * Parse command-line options.
 * @returns {{changelogDir: string, changelogFile: string, manifestPath: string}}
 */
function parseArgs() {
  const args = process.argv.slice(2);
  const getArg = (name, fallback) => {
    const index = args.indexOf(`--${name}`);
    if (index === -1) {
      return fallback;
    }
    return args[index + 1] ?? fallback;
  };

  const changelogDir = getArg('dir', process.env.CHANGELOG_DIR || 'changelog.d');
  const changelogFile = getArg(
    'output',
    process.env.CHANGELOG_FILE || 'CHANGELOG.md'
  );
  const manifestPath =
    getArg('manifest', process.env.CARGO_TOML || '') ||
    (changelogDir.startsWith('rust/') || changelogFile.startsWith('rust/')
      ? 'rust/Cargo.toml'
      : 'Cargo.toml');

  return {
    changelogDir,
    changelogFile,
    manifestPath,
  };
}

/**
 * Get version from Cargo.toml
 * @param {string} manifestPath
 * @returns {string}
 */
function getVersionFromCargo(manifestPath) {
  const cargoToml = readFileSync(manifestPath, 'utf-8');
  const match = cargoToml.match(/^version\s*=\s*"([^"]+)"/m);

  if (!match) {
    console.error('Error: Could not find version in Cargo.toml');
    process.exit(1);
  }

  return match[1];
}

/**
 * Strip frontmatter from markdown content
 * @param {string} content - Markdown content potentially with frontmatter
 * @returns {string} - Content without frontmatter
 */
function stripFrontmatter(content) {
  const frontmatterMatch = content.match(/^---\s*\n[\s\S]*?\n---\s*\n([\s\S]*)$/);
  if (frontmatterMatch) {
    return frontmatterMatch[1].trim();
  }
  return content.trim();
}

/**
 * Collect all changelog fragments
 * @param {string} changelogDir
 * @returns {string}
 */
function collectFragments(changelogDir) {
  if (!existsSync(changelogDir)) {
    return '';
  }

  const files = readdirSync(changelogDir)
    .filter((f) => f.endsWith('.md') && f !== 'README.md')
    .sort();

  const fragments = [];
  for (const file of files) {
    const rawContent = readFileSync(join(changelogDir, file), 'utf-8');
    // Strip frontmatter (which contains bump type metadata)
    const content = stripFrontmatter(rawContent);
    if (content) {
      fragments.push(content);
    }
  }

  return fragments.join('\n\n');
}

/**
 * Update CHANGELOG.md with collected fragments
 * @param {string} version
 * @param {string} fragments
 * @param {string} changelogFile
 */
function updateChangelog(version, fragments, changelogFile) {
  const dateStr = new Date().toISOString().split('T')[0];
  const newEntry = `\n## [${version}] - ${dateStr}\n\n${fragments}\n`;

  if (existsSync(changelogFile)) {
    let content = readFileSync(changelogFile, 'utf-8');

    if (content.includes(INSERT_MARKER)) {
      content = content.replace(INSERT_MARKER, `${INSERT_MARKER}${newEntry}`);
    } else {
      // Insert after the first ## heading
      const lines = content.split('\n');
      let insertIndex = -1;

      for (let i = 0; i < lines.length; i++) {
        if (lines[i].startsWith('## [')) {
          insertIndex = i;
          break;
        }
      }

      if (insertIndex >= 0) {
        lines.splice(insertIndex, 0, newEntry);
        content = lines.join('\n');
      } else {
        // Append after the main heading
        content += newEntry;
      }
    }

    writeFileSync(changelogFile, content, 'utf-8');
  } else {
    const content = `# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

${INSERT_MARKER}
${newEntry}
`;
    writeFileSync(changelogFile, content, 'utf-8');
  }

  console.log(`Updated ${changelogFile} with version ${version}`);
}

/**
 * Remove processed changelog fragments
 * @param {string} changelogDir
 */
function removeFragments(changelogDir) {
  if (!existsSync(changelogDir)) {
    return;
  }

  const files = readdirSync(changelogDir).filter(
    (f) => f.endsWith('.md') && f !== 'README.md'
  );

  for (const file of files) {
    const filePath = join(changelogDir, file);
    unlinkSync(filePath);
    console.log(`Removed ${filePath}`);
  }
}

try {
  const { changelogDir, changelogFile, manifestPath } = parseArgs();
  const version = getVersionFromCargo(manifestPath);
  console.log(`Collecting changelog fragments for version ${version}`);

  const fragments = collectFragments(changelogDir);

  if (!fragments) {
    console.log('No changelog fragments found');
    process.exit(0);
  }

  updateChangelog(version, fragments, changelogFile);
  removeFragments(changelogDir);

  console.log('Changelog collection complete');
} catch (error) {
  console.error('Error:', error.message);
  process.exit(1);
}
