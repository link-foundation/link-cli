import { describe, expect, test } from 'bun:test';
import {
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

import {
  MAX_LINES,
  WARN_LINES,
  checkDirectory,
  classifyLineCount,
  exitCodeForResult,
  main,
  warningAnnotation,
} from './check-file-size.mjs';

function makeRepo() {
  return mkdtempSync(path.join(tmpdir(), 'check-file-size-'));
}

function writeCSharpFileWithLines(filePath, lineCount) {
  const content = Array.from(
    { length: lineCount },
    (_, index) => `// line ${index + 1}`
  ).join('\n');
  writeFileSync(filePath, content);
}

function captureConsoleLog(callback) {
  const originalLog = console.log;
  const lines = [];
  console.log = (...args) => {
    lines.push(args.join(' '));
  };

  try {
    return {
      result: callback(),
      output: lines.join('\n'),
    };
  } finally {
    console.log = originalLog;
  }
}

describe('check-file-size thresholds', () => {
  test('classifies warning band without blocking', () => {
    expect(classifyLineCount(WARN_LINES)).toBe('within-limit');
    expect(classifyLineCount(WARN_LINES + 1)).toBe('warning');
    expect(classifyLineCount(MAX_LINES)).toBe('warning');
    expect(
      exitCodeForResult({
        warnings: [{ file: 'src/near_limit.cs', lines: WARN_LINES + 1 }],
        violations: [],
      })
    ).toBe(0);
  });

  test('classifies hard limit violations as failures', () => {
    expect(classifyLineCount(MAX_LINES + 1)).toBe('violation');
    expect(
      exitCodeForResult({
        warnings: [],
        violations: [{ file: 'src/over_limit.cs', lines: MAX_LINES + 1 }],
      })
    ).toBe(1);
  });

  test('reports warnings and violations separately', () => {
    const repo = makeRepo();
    try {
      const srcDir = path.join(repo, 'src');
      mkdirSync(srcDir, { recursive: true });
      writeCSharpFileWithLines(
        path.join(srcDir, 'near_limit.cs'),
        WARN_LINES + 1
      );
      writeCSharpFileWithLines(
        path.join(srcDir, 'over_limit.cs'),
        MAX_LINES + 1
      );
      writeCSharpFileWithLines(path.join(srcDir, 'small.cs'), WARN_LINES);

      const result = checkDirectory(repo);

      expect(result.files).toBe(3);
      expect(result.warnings).toEqual([
        {
          file: 'src/near_limit.cs',
          lines: WARN_LINES + 1,
        },
      ]);
      expect(result.violations).toEqual([
        {
          file: 'src/over_limit.cs',
          lines: MAX_LINES + 1,
        },
      ]);
    } finally {
      rmSync(repo, { force: true, recursive: true });
    }
  });

  test('warning annotation uses GitHub Actions format', () => {
    expect(
      warningAnnotation({
        file: 'src/near_limit.cs',
        lines: WARN_LINES + 1,
      })
    ).toBe(
      '::warning file=src/near_limit.cs::File has 901 lines (approaching limit of 1000). Consider extracting code to keep at or below 900 lines and prevent concurrent PR merge limit violations.'
    );
  });

  test('main emits warning annotations without failing warning-only files', () => {
    const repo = makeRepo();
    try {
      const srcDir = path.join(repo, 'src');
      mkdirSync(srcDir, { recursive: true });
      writeCSharpFileWithLines(
        path.join(srcDir, 'near_limit.cs'),
        WARN_LINES + 1
      );

      const { result, output } = captureConsoleLog(() => main(repo));

      expect(result).toBe(0);
      expect(output).toContain('::warning file=src/near_limit.cs::');
      expect(output).toContain('Checked 1 file(s) - all within the line limit');
    } finally {
      rmSync(repo, { force: true, recursive: true });
    }
  });
});
