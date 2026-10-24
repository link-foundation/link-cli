#!/usr/bin/env node

/**
 * Check C# files for maximum and warning line-count thresholds.
 * Exits with error code 1 if any files exceed the hard limit.
 */

import { readFileSync, readdirSync } from 'node:fs';
import { extname, join, relative, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export const MAX_LINES = 1000;
export const WARN_LINES = 900;
export const FILE_EXTENSIONS = ['.cs'];
const EXCLUDE_PATTERNS = ['bin', 'obj', '.git', 'node_modules', 'artifacts'];

/**
 * Check if a path should be excluded
 * @param {string} path
 * @returns {boolean}
 */
export function shouldExclude(path) {
  return EXCLUDE_PATTERNS.some((pattern) => path.includes(pattern));
}

/**
 * Recursively find all C# files in a directory
 * @param {string} directory
 * @returns {string[]}
 */
export function findCSharpFiles(directory) {
  const files = [];

  function walkDir(dir) {
    const entries = readdirSync(dir, { withFileTypes: true });

    for (const entry of entries) {
      const fullPath = join(dir, entry.name);

      if (shouldExclude(fullPath)) {
        continue;
      }

      if (entry.isDirectory()) {
        walkDir(fullPath);
      } else if (entry.isFile() && FILE_EXTENSIONS.includes(extname(entry.name))) {
        files.push(fullPath);
      }
    }
  }

  walkDir(directory);
  return files.sort();
}

/**
 * Count lines in a file
 * @param {string} filePath
 * @returns {number}
 */
export function countLines(filePath) {
  const content = readFileSync(filePath, 'utf-8');
  if (content.length === 0) {
    return 0;
  }

  const lineCount = content.split(/\r\n|\r|\n/u).length;
  return content.endsWith('\n') || content.endsWith('\r')
    ? lineCount - 1
    : lineCount;
}

/**
 * Classify a line count against the configured thresholds.
 * @param {number} lineCount
 * @returns {'within-limit' | 'warning' | 'violation'}
 */
export function classifyLineCount(lineCount) {
  if (lineCount > MAX_LINES) {
    return 'violation';
  }
  if (lineCount > WARN_LINES) {
    return 'warning';
  }
  return 'within-limit';
}

/**
 * Convert a path to a stable repository-relative value for output.
 * @param {string} cwd
 * @param {string} file
 * @returns {string}
 */
function relativeFilePath(cwd, file) {
  return relative(cwd, file).replaceAll('\\', '/');
}

/**
 * Check a directory and collect warning-band and hard-limit findings.
 * @param {string} cwd
 * @returns {{ files: number, warnings: Array<{ file: string, lines: number }>, violations: Array<{ file: string, lines: number }> }}
 */
export function checkDirectory(cwd = process.cwd()) {
  const files = findCSharpFiles(cwd);
  const result = {
    files: files.length,
    warnings: [],
    violations: [],
  };

  for (const file of files) {
    const lineCount = countLines(file);
    const finding = {
      file: relativeFilePath(cwd, file),
      lines: lineCount,
    };

    switch (classifyLineCount(lineCount)) {
      case 'violation':
        result.violations.push(finding);
        break;
      case 'warning':
        result.warnings.push(finding);
        break;
      case 'within-limit':
        break;
      default:
        throw new Error(`Unknown line-count classification for ${file}`);
    }
  }

  return result;
}

/**
 * Escape a GitHub Actions annotation property value.
 * @param {string} value
 * @returns {string}
 */
export function escapeAnnotationProperty(value) {
  return value
    .replaceAll('%', '%25')
    .replaceAll('\r', '%0D')
    .replaceAll('\n', '%0A')
    .replaceAll(':', '%3A')
    .replaceAll(',', '%2C');
}

/**
 * Escape a GitHub Actions annotation message value.
 * @param {string} value
 * @returns {string}
 */
export function escapeAnnotationMessage(value) {
  return value
    .replaceAll('%', '%25')
    .replaceAll('\r', '%0D')
    .replaceAll('\n', '%0A');
}

/**
 * Build a GitHub Actions warning annotation for a warning-band finding.
 * @param {{ file: string, lines: number }} finding
 * @returns {string}
 */
export function warningAnnotation(finding) {
  const message =
    `File has ${finding.lines} lines (approaching limit of ${MAX_LINES}). ` +
    `Consider extracting code to keep at or below ${WARN_LINES} lines and ` +
    'prevent concurrent PR merge limit violations.';

  return `::warning file=${escapeAnnotationProperty(finding.file)}::${escapeAnnotationMessage(message)}`;
}

/**
 * Return the process exit code for a check result.
 * @param {{ violations: Array<{ file: string, lines: number }> }} result
 * @returns {number}
 */
export function exitCodeForResult(result) {
  return result.violations.length === 0 ? 0 : 1;
}

/**
 * Print warning-band findings without failing the check.
 * @param {Array<{ file: string, lines: number }>} warnings
 */
function printWarnings(warnings) {
  if (warnings.length === 0) {
    return;
  }

  for (const warning of warnings) {
    console.log(warningAnnotation(warning));
    console.log(
      `WARNING: ${warning.file} has ${warning.lines} lines (approaching limit of ${MAX_LINES}, warning threshold: ${WARN_LINES})`
    );
  }

  console.log();
  console.log(
    `The following files are approaching the ${MAX_LINES} line limit (>${WARN_LINES} lines):`
  );
  for (const warning of warnings) {
    console.log(`  ${warning.file}`);
  }
  console.log(
    '\nConsider extracting code to prevent concurrent PR merge limit violations.\n'
  );
}

/**
 * Print hard-limit violations.
 * @param {Array<{ file: string, lines: number }>} violations
 */
function printViolations(violations) {
  if (violations.length === 0) {
    return;
  }

  console.log('Found files exceeding the line limit:\n');
  for (const violation of violations) {
    console.log(
      `  ${violation.file}: ${violation.lines} lines (exceeds ${MAX_LINES})`
    );
  }
  console.log(`\nPlease refactor these files to be under ${MAX_LINES} lines\n`);
}

/**
 * Run the command-line file-size check.
 * @param {string} cwd
 * @returns {number}
 */
export function main(cwd = process.cwd()) {
  console.log(
    `\nChecking C# files for maximum ${MAX_LINES} lines (warning above ${WARN_LINES})...\n`
  );

  const result = checkDirectory(cwd);

  printWarnings(result.warnings);

  if (result.violations.length === 0) {
    console.log(`Checked ${result.files} file(s) - all within the line limit\n`);
  } else {
    printViolations(result.violations);
  }

  return exitCodeForResult(result);
}

const entryPath = process.argv[1];
const invokedDirectly =
  typeof entryPath === 'string' &&
  entryPath.length > 0 &&
  import.meta.url === pathToFileURL(resolve(entryPath)).href;

if (invokedDirectly) {
  try {
    process.exit(main());
  } catch (error) {
    console.error('Error:', error.message);
    process.exit(1);
  }
}
