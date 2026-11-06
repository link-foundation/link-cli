#!/usr/bin/env node
// Discover maintained files with Git so examples and helpers receive the same checks.
import { execFileSync } from 'node:child_process';
import { readFile, writeFile } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { ESLint } from '../../js/node_modules/eslint/lib/api.js';
import prettier from '../../js/node_modules/prettier/index.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const files = execFileSync('git', ['ls-files', '-z', '--', '*.js', '*.mjs', '*.cjs', '*.jsx'], {
  cwd: root,
  encoding: 'utf8',
})
  .split('\0')
  .filter((file) => file && !/^(dev\/log|docs\/case-studies)\//.test(file));
if (!files.length) throw new Error('No tracked JavaScript files; no checks performed');
const write = process.argv.includes('--write');
const prettierOptions = { singleQuote: true, trailingComma: 'all', printWidth: 100 };
let failures = 0;
for (const file of files) {
  const path = resolve(root, file);
  const source = await readFile(path, 'utf8');
  const options = { ...prettierOptions, filepath: path };
  if (write) await writeFile(path, await prettier.format(source, options));
  else if (!(await prettier.check(source, options))) {
    console.error(`::error file=${file}::Formatting differs; run npm --prefix js run format`);
    failures++;
  }
}
const eslint = new ESLint({ cwd: root, overrideConfigFile: resolve(root, 'js/eslint.config.mjs') });
const results = await eslint.lintFiles(files);
const formatter = await eslint.loadFormatter('stylish');
const output = await formatter.format(results);
if (output) console.error(output);
failures += results.reduce((count, item) => count + item.errorCount + item.warningCount, 0);
console.log(`JavaScript quality: ${files.length} files, ${failures} formatting/lint findings`);
process.exitCode = failures ? 1 : 0;
