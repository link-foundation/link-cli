#!/usr/bin/env node

import { readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

// Verified against the action tag's support/versions table, without network in CI:
// https://github.com/zizmorcore/zizmor-action/blob/v0.6.4/support/versions
const supportedZizmor = { 'v0.6.4': '1.30.1' };

export function checkWorkflowPolicy(text) {
  const errors = [];
  const shellText = text.replace(/\\\r?\n\s*/g, ' ');
  for (const line of shellText.split('\n')) {
    const code = line.replace(/\s*#.*$/, '');
    if (/\b(?:ubuntu|windows|macos)-latest\b/.test(code)) {
      errors.push('Pin hosted runners to an explicit image');
    }
    if (/\bnpx\b/.test(code)) {
      for (const match of code.matchAll(
        /(?:\s)(?:-p|--package)(?:\s+|=)(\S+)/g
      )) {
        const spec = match[1].replace(/^['"]|['"]$/g, '');
        if (!/^(?:@[^/\s]+\/)?[^@\s]+@\d+\.\d+\.\d+(?:-[\w.-]+)?$/.test(spec)) {
          errors.push(`Pin npx package to an exact version: ${spec}`);
        }
      }
    }
  }
  const action = text.match(/zizmorcore\/zizmor-action@(\S+)/)?.[1];
  if (action) {
    const version = text
      .slice(text.indexOf('zizmorcore/zizmor-action@'))
      .match(/\bversion:\s*(\d+\.\d+\.\d+)/)?.[1];
    const cliVersions = Array.from(
      text.matchAll(/zizmor==(\d+\.\d+\.\d+)/g),
      (match) => match[1]
    );
    if (!version || cliVersions.some((cliVersion) => cliVersion !== version)) {
      errors.push(
        'Every zizmor pass and reproduction command must use the action version'
      );
    }
    if (supportedZizmor[action] !== version) {
      errors.push(`Verify zizmor ${version} is supported by action ${action}`);
    }
  }
  return errors;
}

function workflowFiles(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    return entry.isDirectory()
      ? workflowFiles(path)
      : /\.ya?ml$/.test(path)
        ? [path]
        : [];
  });
}

if (
  process.argv[1] &&
  import.meta.url === pathToFileURL(resolve(process.argv[1])).href
) {
  const files = [
    ...workflowFiles('.github/workflows'),
    ...workflowFiles('.github/actions'),
  ];
  for (const path of files) {
    for (const error of checkWorkflowPolicy(readFileSync(path, 'utf8'))) {
      console.error(`::error file=${path}::${error}`);
      process.exitCode = 1;
    }
  }
  if (!process.exitCode) {
    console.log('Workflow package, zizmor, and hosted runner pins verified');
  }
}
