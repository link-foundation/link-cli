import assert from 'node:assert/strict';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { gunzipSync, gzipSync } from 'node:zlib';

const modulePath = process.argv[2];
if (!modulePath) {
  throw new Error('Pass the path to Hive Mind’s src/development-log.lib.mjs');
}
const { verifyDevelopmentLogDirectory } = await import(pathToFileURL(resolve(modulePath)).href);

const directory = await mkdtemp(join(tmpdir(), 'hive-log-gzip-'));
try {
  const file = join(directory, 'job.log.gz');
  const message = 'CI job completed\n';
  // Add a synthetic token-shaped comment to the gzip header. The decoded log
  // contains only the message above.
  const comment = Buffer.from(` token=ghp_${'Ab1cD2'.repeat(6)} \0`);
  const gzip = gzipSync(message);
  gzip[3] |= 0x10; // FCOMMENT in the gzip header
  const archive = Buffer.concat([gzip.subarray(0, 10), comment, gzip.subarray(10)]);
  await writeFile(file, archive);
  assert.equal(gunzipSync(archive).toString(), message);

  await verifyDevelopmentLogDirectory(directory);

  const after = await readFile(file);
  console.log('Original header:', archive.subarray(0, 4).toString('hex'));
  console.log('Header after verification:', after.subarray(0, 4).toString('hex'));
  assert.equal(gunzipSync(after).toString(), message);
} finally {
  await rm(directory, { recursive: true, force: true });
}
