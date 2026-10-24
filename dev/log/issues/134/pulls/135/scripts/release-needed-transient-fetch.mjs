#!/usr/bin/env node
// Reproduce: one dropped connection fails check-release-needed.mjs.
//
// Usage: node release-needed-transient-fetch.mjs <path/to/check-release-needed.mjs>
//
// The fake fetch throws what Node's fetch throws on a dropped connection
// once, then answers like nuget.org. A lookup that retries returns the
// versions; one that does not fails the release decision.
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

const target = process.argv[2];
if (!target) {
  console.error('Usage: node release-needed-transient-fetch.mjs <check-release-needed.mjs>');
  process.exit(2);
}
const { fetchNugetVersions } = await import(pathToFileURL(resolve(target)).href);

let calls = 0;
const fetchImpl = async () => {
  calls += 1;
  if (calls === 1) {
    throw new TypeError('fetch failed', { cause: Object.assign(new Error('read ECONNRESET'), { code: 'ECONNRESET' }) });
  }
  return { ok: true, status: 200, json: async () => ({ versions: ['1.0.0'] }) };
};

try {
  // The third argument is ignored by implementations without retry options.
  const versions = await fetchNugetVersions('example', fetchImpl, { delayMs: 0 });
  console.log(`OK after ${calls} request(s): ${JSON.stringify(versions)}`);
} catch (error) {
  console.log(`FAILED after ${calls} request(s): ${error.message}`);
  process.exitCode = 1;
}
