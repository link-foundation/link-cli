#!/usr/bin/env node

/**
 * Check for files exceeding the maximum allowed line count
 * Exits with error code 1 if any files exceed the limit
 *
 * Usage:
 *   node csharp/scripts/check-file-size.mjs [--lang csharp|rust|all] [--dir path]
 *
 * Options:
 *   --lang    Language to check: csharp, rust, or all (default: all)
 *   --dir     Directory to check (default: current working directory)
 */

import { existsSync, readFileSync, readdirSync } from 'fs';
import { join, relative, extname, resolve, sep } from 'path';
import { pathToFileURL } from 'url';

export const MAX_LINES = 1000;
// Matched against whole path components, never substrings (issue #134):
// `path.includes('bin')` used to skip rust/examples/lino_binary_dump.rs and
// `'target'` skipped rust/src/sequences/target_matcher.rs, so those files
// were never measured.
export const EXCLUDE_DIRS = ['bin', 'obj', '.git', 'node_modules', 'artifacts', 'target'];
// Verbatim copies of other repositories kept as evidence, not our sources.
export const EXCLUDE_PREFIXES = ['dev/log'];

export const LANGUAGE_CONFIG = {
  csharp: {
    extensions: ['.cs'],
    name: 'C#',
    baseDir: 'csharp',
  },
  rust: {
    extensions: ['.rs'],
    name: 'Rust',
    baseDir: 'rust',
  },
};

/**
 * Parse command line arguments
 * @returns {{ lang: string, dir: string }}
 */
function parseArgs() {
  const args = process.argv.slice(2);
  let lang = 'all';
  let dir = process.cwd();

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--lang' && args[i + 1]) {
      lang = args[i + 1];
      i++;
    } else if (args[i] === '--dir' && args[i + 1]) {
      dir = args[i + 1];
      i++;
    }
  }

  return { lang, dir };
}

/**
 * Check if a path should be excluded
 * @param {string} path
 * @param {string} rootDir directory the exclusions are relative to
 * @returns {boolean}
 */
export function shouldExclude(path, rootDir) {
  const components = relative(rootDir, path).split(sep);
  if (components.some((component) => EXCLUDE_DIRS.includes(component))) {
    return true;
  }
  const relativePath = components.join('/');
  return EXCLUDE_PREFIXES.some(
    (prefix) => relativePath === prefix || relativePath.startsWith(`${prefix}/`)
  );
}

/**
 * Recursively find all files matching extensions in a directory
 * @param {string} directory
 * @param {string[]} extensions
 * @param {string} rootDir directory the exclusions are relative to
 * @returns {string[]}
 */
export function findFiles(directory, extensions, rootDir = directory) {
  const files = [];

  function walkDir(dir) {
    // A read error now fails the check instead of silently shrinking the
    // set of measured files.
    const entries = readdirSync(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = join(dir, entry.name);

      if (shouldExclude(fullPath, rootDir)) {
        continue;
      }

      if (entry.isDirectory()) {
        walkDir(fullPath);
      } else if (entry.isFile() && extensions.includes(extname(entry.name))) {
        files.push(fullPath);
      }
    }
  }

  // A language directory that does not exist has nothing to check.
  if (existsSync(directory)) {
    walkDir(directory);
  }
  return files;
}

/**
 * Count lines in a file the way Rust's `str::lines` does, so both
 * pipelines agree: a trailing newline does not start an extra line.
 * @param {string} filePath
 * @returns {number}
 */
export function countLines(filePath) {
  const content = readFileSync(filePath, 'utf-8');
  if (content === '') {
    return 0;
  }
  const lines = content.split('\n').length;
  return content.endsWith('\n') ? lines - 1 : lines;
}

/**
 * Check files for a specific language
 * @param {string} baseDir
 * @param {object} config
 * @returns {{ files: number, violations: Array }}
 */
export function checkLanguage(baseDir, config) {
  const searchDir = join(baseDir, config.baseDir);
  const files = findFiles(searchDir, config.extensions, baseDir);
  const violations = [];

  for (const file of files) {
    const lineCount = countLines(file);
    if (lineCount > MAX_LINES) {
      violations.push({
        file: relative(baseDir, file),
        lines: lineCount,
      });
    }
  }

  return { files: files.length, violations };
}

function main() {
  try {
    const { lang, dir } = parseArgs();

    console.log(`\nChecking files for maximum ${MAX_LINES} lines...\n`);

    let totalFiles = 0;
    let allViolations = [];

    const languagesToCheck = lang === 'all' ? Object.keys(LANGUAGE_CONFIG) : [lang];

    for (const language of languagesToCheck) {
      const config = LANGUAGE_CONFIG[language];
      if (!config) {
        console.error(`Unknown language: ${language}`);
        process.exit(1);
      }

      console.log(`Checking ${config.name} files in ${config.baseDir}/...`);
      const { files, violations } = checkLanguage(dir, config);
      totalFiles += files;
      allViolations = allViolations.concat(violations);
      console.log(`  Found ${files} ${config.name} file(s)`);
    }

    console.log('');

    if (allViolations.length === 0) {
      console.log(`Checked ${totalFiles} file(s) - all within the line limit\n`);
      process.exit(0);
    } else {
      console.log('Found files exceeding the line limit:\n');
      for (const violation of allViolations) {
        console.log(
          `  ${violation.file}: ${violation.lines} lines (exceeds ${MAX_LINES})`
        );
      }
      console.log(`\nPlease refactor these files to be under ${MAX_LINES} lines\n`);
      process.exit(1);
    }
  } catch (error) {
    console.error('Error:', error.message);
    process.exit(1);
  }
}

if (import.meta.url === pathToFileURL(resolve(process.argv[1] ?? '')).href) {
  main();
}
