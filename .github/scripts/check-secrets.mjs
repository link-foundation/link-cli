#!/usr/bin/env node
// Git is the inventory: force-added evidence must be scanned even when ignored.
import { execFileSync, spawnSync } from 'node:child_process';
import { lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { gunzipSync } from 'node:zlib';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const cli = resolve(root, 'js/node_modules/secretlint/bin/secretlint.js');
// A corrupt or unexpectedly large archive is an error, never a skipped file.
export const MAX_DECOMPRESSED_BYTES = 128 * 1024 * 1024;

// Only `.log.gz` used to be decompressed. secretlint received every other
// archive - JSON evidence and whole template snapshots in .tar.gz - as
// compressed bytes, so their contents were never scanned (issue #172).
export const isCompressed = (file) => /\.gz$/i.test(file);
export const isTarball = (file) => /\.(tar\.gz|tgz)$/i.test(file);

// `./` keeps a tracked file named `-x` from being parsed as a CLI option.
export const asOperand = (file) => (file.startsWith('/') ? file : `./${file}`);

export function gunzip(buffer) {
  return gunzipSync(buffer, { maxOutputLength: MAX_DECOMPRESSED_BYTES });
}

const field = (block, start, length) =>
  block
    .subarray(start, start + length)
    .toString('utf8')
    .replace(/\0.*$/s, '');

/**
 * List the regular files of an uncompressed tar archive (ustar, pax `path`
 * records and GNU long names). Directories and links carry no content.
 * @param {Buffer} tar
 * @returns {Array<{name: string, content: Buffer}>}
 */
export function readTarEntries(tar) {
  const entries = [];
  let longName;
  for (let offset = 0; ;) {
    // A real archive ends with a zero block; anything else was truncated.
    if (offset + 512 > tar.length) throw new Error('Truncated tar archive');
    const header = tar.subarray(offset, offset + 512);
    if (header.every((byte) => byte === 0)) break;
    const size = Number.parseInt(field(header, 124, 12).trim() || '0', 8);
    if (!Number.isSafeInteger(size) || size < 0 || offset + 512 + size > tar.length) {
      throw new Error(`Malformed tar entry at byte ${offset}`);
    }
    const content = tar.subarray(offset + 512, offset + 512 + size);
    const type = String.fromCharCode(header[156] || 48);
    const prefix = field(header, 345, 155);
    const name = longName ?? (prefix ? `${prefix}/` : '') + field(header, 0, 100);
    longName = undefined;
    if (type === 'L') longName = content.toString('utf8').replace(/\0.*$/s, '');
    else if (type === 'x') longName = /(?:^|\n)\d+ path=([^\n]*)\n/.exec(content)?.[1];
    else if (type === '0' || type === '7') entries.push({ name, content });
    offset += 512 + Math.ceil(size / 512) * 512;
  }
  return entries;
}

function scanFiles(files) {
  for (let offset = 0; offset < files.length; offset += 100) {
    const result = spawnSync(
      process.execPath,
      [cli, '--no-gitignore', '--no-glob', ...files.slice(offset, offset + 100).map(asOperand)],
      { cwd: root, stdio: 'inherit' },
    );
    if (result.error) throw result.error;
    if (result.status !== 0) process.exit(result.status ?? 1);
  }
}

function scanStdin(name, input) {
  const result = spawnSync(process.execPath, [cli, '--stdinFileName', name], {
    cwd: root,
    input,
    stdio: ['pipe', 'inherit', 'inherit'],
  });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

/** Unpack a tarball into a scratch directory and scan its regular files. */
function scanTarball(file) {
  const scratch = mkdtempSync(join(tmpdir(), 'check-secrets-'));
  try {
    const base = join(scratch, file);
    const names = [];
    for (const { name, content } of readTarEntries(gunzip(readFileSync(resolve(root, file))))) {
      const target = resolve(base, name);
      if (!target.startsWith(`${base}/`)) throw new Error(`Unsafe path ${name} in ${file}`);
      mkdirSync(dirname(target), { recursive: true });
      writeFileSync(target, content);
      names.push(target);
    }
    // Absolute paths, so secretlint still runs from the repository root and
    // finds .secretlintrc.json there.
    console.log(`Scanning ${names.length} file(s) inside ${file} (unpacked under ${base})`);
    scanFiles(names);
    return names.length;
  } finally {
    rmSync(scratch, { recursive: true, force: true });
  }
}

function main() {
  const files = process.argv.slice(2);
  if (!files.length) {
    files.push(
      ...execFileSync('git', ['ls-files', '-z'], { cwd: root, encoding: 'utf8' })
        .split('\0')
        .filter(Boolean),
    );
  }
  if (!files.length) throw new Error('No tracked files; secrets scan did not run');
  // A tracked symlink is only a path; following it would scan whatever it
  // points to, possibly outside the repository.
  const regular = files.filter((file) => !lstatSync(resolve(root, file)).isSymbolicLink());
  scanFiles(regular.filter((file) => !isCompressed(file)));
  let members = 0;
  for (const file of regular.filter(isCompressed)) {
    if (isTarball(file)) members += scanTarball(file);
    else scanStdin(file.slice(0, -3), gunzip(readFileSync(resolve(root, file))));
  }
  console.log(
    `Secrets scan: ${files.length} tracked files checked, ` +
      `including ${members} file(s) inside tarballs`,
  );
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) main();
