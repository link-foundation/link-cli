#!/usr/bin/env node

/**
 * Check for files exceeding the maximum allowed line count
 * Exits with error code 1 if any files exceed the limit
 *
 * Usage:
 *   node scripts/check-file-size.mjs [--lang csharp|rust|all] [--dir path]
 *
 * Options:
 *   --lang    Language to check: csharp, rust, or all (default: all)
 *   --dir     Directory to check (default: current working directory)
 */

import { readFileSync, readdirSync } from 'fs';
import { join, relative, extname } from 'path';

const MAX_LINES = 1000;
const EXCLUDE_PATTERNS = ['bin', 'obj', '.git', 'node_modules', 'artifacts', 'target'];

const LANGUAGE_CONFIG = {
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
 * @returns {boolean}
 */
function shouldExclude(path) {
  return EXCLUDE_PATTERNS.some((pattern) => path.includes(pattern));
}

/**
 * Recursively find all files matching extensions in a directory
 * @param {string} directory
 * @param {string[]} extensions
 * @returns {string[]}
 */
function findFiles(directory, extensions) {
  const files = [];

  function walkDir(dir) {
    try {
      const entries = readdirSync(dir, { withFileTypes: true });

      for (const entry of entries) {
        const fullPath = join(dir, entry.name);

        if (shouldExclude(fullPath)) {
          continue;
        }

        if (entry.isDirectory()) {
          walkDir(fullPath);
        } else if (entry.isFile() && extensions.includes(extname(entry.name))) {
          files.push(fullPath);
        }
      }
    } catch (error) {
      // Directory might not exist, that's ok
    }
  }

  walkDir(directory);
  return files;
}

/**
 * Count lines in a file
 * @param {string} filePath
 * @returns {number}
 */
function countLines(filePath) {
  const content = readFileSync(filePath, 'utf-8');
  return content.split('\n').length;
}

/**
 * Check files for a specific language
 * @param {string} baseDir
 * @param {object} config
 * @returns {{ files: number, violations: Array }}
 */
function checkLanguage(baseDir, config) {
  const searchDir = join(baseDir, config.baseDir);
  const files = findFiles(searchDir, config.extensions);
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
