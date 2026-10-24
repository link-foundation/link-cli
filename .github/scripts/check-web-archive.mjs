#!/usr/bin/env node

/**
 * Check broken links against the Wayback Machine (web.archive.org)
 *
 * This script reads the lychee link checker output (markdown format),
 * extracts broken URLs, and checks each one against the Wayback Machine API.
 * It then outputs a report with:
 * - Links that have a web archive version (with suggestion to replace)
 * - Links that have no web archive version (clearly marked as unrecoverable)
 *
 * Usage:
 *   node .github/scripts/check-web-archive.mjs
 *
 * Environment variables:
 *   - LYCHEE_OUTPUT: Path to lychee markdown output file (default: lychee/out.md)
 *
 * GitHub Actions outputs:
 *   - all_archived: 'true' if all broken links have a web archive version
 *
 * Exit codes:
 *   - 0: All broken links have web archive versions (or no broken links)
 *   - 1: Some broken links have no web archive version, or the Web Archive
 *        could not be asked about them
 */

import { readFileSync, appendFileSync, existsSync } from 'fs';
import { resolve } from 'path';
import { setTimeout as sleep } from 'timers/promises';
import { fileURLToPath } from 'url';

const WAYBACK_API = 'https://archive.org/wayback/available?url=';

/**
 * Write output to GitHub Actions output file
 * @param {string} name - Output name
 * @param {string} value - Output value
 */
function setOutput(name, value) {
  const outputFile = process.env.GITHUB_OUTPUT;
  if (outputFile) {
    appendFileSync(outputFile, `${name}=${value}\n`);
  }
  console.log(`${name}=${value}`);
}

/**
 * Extract the "Errors per input" section of a lychee markdown report.
 *
 * Everything after it - most importantly "## Redirects per input" - describes
 * links that resolved successfully. Scanning the whole report made every
 * redirected link look broken and failed the workflow for healthy URLs
 * (issue #96).
 * @param {string} content - The markdown content from lychee
 * @returns {string} The errors section, or an empty string when there is none
 */
function extractErrorsSection(content) {
  const lines = content.split('\n');
  const start = lines.findIndex((line) => /^#+\s+Errors per input\s*$/.test(line));
  if (start === -1) {
    return '';
  }
  const heading = /^(#+)\s/.exec(lines[start])[1].length;
  const section = [];
  for (const line of lines.slice(start + 1)) {
    const next = /^(#+)\s/.exec(line);
    if (next && next[1].length <= heading) {
      break; // a sibling or parent heading ends the errors section
    }
    section.push(line);
  }
  return section.join('\n');
}

/**
 * Extract broken links from lychee markdown output.
 * Lychee markdown format includes lines like:
 *   * [404] https://example.com/broken-link
 *   * [ERROR] <file:///repo/missing.yml> | File not found
 * @param {string} content - The markdown content from lychee
 * @returns {{urls: string[], others: string[]}} Broken http(s) URLs, and broken
 *   links that cannot be looked up in the Wayback Machine (local files,
 *   unresolvable root-relative links, ...)
 */
function extractBrokenLinks(content) {
  const section = extractErrorsSection(content);
  const urls = [];
  const others = [];

  // One bullet per broken link; the status marker is always present.
  const entryPattern =
    /^\s*(?:\*|-)\s+\[(?:4\d\d|5\d\d|ERROR|TIMEOUT|UNKNOWN)\]\s+<?([^\s>|)]+)>?/gim;
  let match;
  while ((match = entryPattern.exec(section)) !== null) {
    const link = match[1].trim().replace(/[.,;!?]+$/, '');
    if (!link) continue;
    if (/^https?:\/\//i.test(link)) {
      if (!urls.includes(link)) urls.push(link);
    } else if (!others.includes(link)) {
      others.push(link);
    }
  }

  return { urls, others };
}

/**
 * Check if a URL has an archived version in the Wayback Machine
 * Uses the Wayback Machine Availability API:
 * https://archive.org/help/wayback_api.php
 *
 * Network errors, timeouts, 429 and 5xx are retried with backoff. When they
 * persist the result carries `error`, so the report says the lookup failed
 * instead of claiming that the link has no archived version (issue #134).
 * @param {string} url - The URL to check
 * @param {object} [options]
 * @param {typeof fetch} [options.fetchImpl]
 * @param {number} [options.attempts]
 * @param {number} [options.delayMs] - wait before the second attempt, doubled after
 * @param {number} [options.timeoutMs] - per request
 * @returns {Promise<{available: boolean, archiveUrl: string|null, timestamp: string|null, error?: string}>}
 */
export async function checkWaybackMachine(
  url,
  { fetchImpl = fetch, attempts = 3, delayMs = 2000, timeoutMs = 10000 } = {}
) {
  const apiUrl = `${WAYBACK_API}${encodeURIComponent(url)}`;
  const notArchived = { available: false, archiveUrl: null, timestamp: null };
  let failure = '';

  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    if (attempt > 1) {
      console.log(
        `  Retrying the Wayback API (attempt ${attempt} of ${attempts}) after: ${failure}`
      );
      await sleep(delayMs * 2 ** (attempt - 2));
    }

    let response;
    try {
      response = await fetchImpl(apiUrl, {
        headers: {
          'User-Agent': 'broken-link-checker/1.0 (GitHub Actions CI)',
        },
        signal: AbortSignal.timeout(timeoutMs),
      });
    } catch (error) {
      // fetch() rejects with a bare "fetch failed" and keeps the reason in cause.
      const cause = error.cause?.code ?? error.cause?.message;
      failure = cause ? `${error.message}: ${cause}` : error.message;
      continue;
    }

    if (response.status === 429 || response.status >= 500) {
      failure = `HTTP ${response.status}`;
      continue;
    }
    if (!response.ok) {
      return { ...notArchived, error: `HTTP ${response.status}` };
    }

    let data;
    try {
      data = await response.json();
    } catch (error) {
      failure = `unreadable answer: ${error.message}`;
      continue;
    }

    if (data.archived_snapshots?.closest?.available === true) {
      const snapshot = data.archived_snapshots.closest;
      const archiveUrl = snapshot.url.replace(/^http:\/\//, 'https://');
      return {
        available: true,
        archiveUrl,
        timestamp: snapshot.timestamp,
      };
    }

    return notArchived;
  }

  return { ...notArchived, error: `${failure} (after ${attempts} attempts)` };
}

/**
 * Format a timestamp from Wayback Machine (YYYYMMDDHHmmss) to readable date
 * @param {string} timestamp - e.g. "20231015143022"
 * @returns {string} - e.g. "2023-10-15"
 */
function formatTimestamp(timestamp) {
  if (!timestamp || timestamp.length < 8) {
    return timestamp;
  }
  const year = timestamp.slice(0, 4);
  const month = timestamp.slice(4, 6);
  const day = timestamp.slice(6, 8);
  return `${year}-${month}-${day}`;
}

/**
 * Main function
 */
async function main() {
  const lycheeOutput = process.env.LYCHEE_OUTPUT || 'lychee/out.md';

  console.log('=== Web Archive Fallback Check ===\n');
  console.log(`Reading lychee output from: ${lycheeOutput}\n`);

  if (!existsSync(lycheeOutput)) {
    console.log('No lychee output file found. Skipping web archive check.');
    setOutput('all_archived', 'true');
    process.exit(0);
  }

  const content = readFileSync(lycheeOutput, 'utf-8');
  const { urls: brokenUrls, others: unarchivableLinks } =
    extractBrokenLinks(content);

  if (unarchivableLinks.length > 0) {
    // Local files and unresolvable root-relative links have no Wayback
    // equivalent. Reporting `all_archived=true` for them turned a real lychee
    // failure into a green run (issue #96).
    console.log(
      `✗ ${unarchivableLinks.length} broken link(s) cannot be checked against the Web Archive:`
    );
    for (const link of unarchivableLinks) {
      console.log(`  ${link}`);
      console.log(
        '::error title=Broken link - not recoverable from the Web Archive::' +
          `Broken link detected: ${link}\n` +
          'It is not an http(s) URL (missing file, unresolvable relative link, ...),\n' +
          'so the Wayback Machine cannot provide a fallback.\n' +
          'How to fix: correct the path, restore the missing file, or pass --root-dir\n' +
          'to lychee so root-relative links resolve.'
      );
    }
    console.log('');
  }

  if (brokenUrls.length === 0) {
    console.log('No broken URLs found in lychee output.');
    setOutput('all_archived', unarchivableLinks.length === 0 ? 'true' : 'false');
    process.exit(unarchivableLinks.length === 0 ? 0 : 1);
  }

  console.log(
    `Found ${brokenUrls.length} broken URL(s). Checking Web Archive...\n`
  );

  const withArchive = [];
  const withoutArchive = [];
  const unchecked = [];

  for (const url of brokenUrls) {
    console.log(`Checking: ${url}`);
    const result = await checkWaybackMachine(url);

    if (result.available) {
      const date = formatTimestamp(result.timestamp);
      console.log(`  ✓ Archived on ${date}: ${result.archiveUrl}`);
      withArchive.push({ url, archiveUrl: result.archiveUrl, date });
    } else if (result.error) {
      console.log(`  ✗ Web Archive lookup failed: ${result.error}`);
      unchecked.push({ url, error: result.error });
    } else {
      console.log('  ✗ Not found in Web Archive');
      withoutArchive.push(url);
    }

    // Small delay to avoid rate-limiting the Wayback API
    await new Promise((resolve) => globalThis.setTimeout(resolve, 500));
  }

  console.log('\n=== Web Archive Check Summary ===\n');

  if (withArchive.length > 0) {
    console.log(
      `✓ ${withArchive.length} broken link(s) have Web Archive versions - consider replacing:`
    );
    for (const { url, archiveUrl, date } of withArchive) {
      console.log(`  Original: ${url}`);
      console.log(`  Archive (${date}): ${archiveUrl}`);
      console.log('');
    }

    // Print GitHub Actions annotations as suggestions (one per link)
    for (const { url, archiveUrl, date } of withArchive) {
      console.log(
        `::notice title=Broken link - Web Archive available (${date})::` +
          `Broken link detected: ${url}\n` +
          `A Web Archive snapshot from ${date} is available.\n` +
          `Suggested fix: replace the broken link with the archived version:\n` +
          `  ${archiveUrl}`
      );
    }
  }

  if (withoutArchive.length > 0) {
    console.log(
      `✗ ${withoutArchive.length} broken link(s) have NO Web Archive version:`
    );
    for (const url of withoutArchive) {
      console.log(`  ${url}`);
    }
    console.log('');

    // Print GitHub Actions annotations as errors (one per link)
    for (const url of withoutArchive) {
      console.log(
        `::error title=Broken link - No Web Archive fallback::` +
          `Broken link detected: ${url}\n` +
          `No archived version was found in the Wayback Machine.\n` +
          `How to fix:\n` +
          `  1. Find an updated URL for the same or equivalent content and replace the link.\n` +
          `  2. Remove the link if the content is no longer relevant.\n` +
          `  3. Add the URL to .lycheeignore if it is a known false positive (e.g. localhost, example.com).`
      );
    }
  }

  if (unchecked.length > 0) {
    console.log(
      `✗ ${unchecked.length} broken link(s) could not be checked against the Web Archive:`
    );
    for (const { url, error } of unchecked) {
      console.log(`  ${url}: ${error}`);
      console.log(
        `::error title=Broken link - Web Archive lookup failed::` +
          `Broken link detected: ${url}\n` +
          `The Wayback Machine could not be asked whether it has a snapshot: ${error}\n` +
          `How to fix: re-run the job. If the lookup keeps failing, fix or remove the link.`
      );
    }
    console.log('');
  }

  const allArchived =
    withoutArchive.length === 0 &&
    unarchivableLinks.length === 0 &&
    unchecked.length === 0;
  setOutput('all_archived', allArchived ? 'true' : 'false');

  if (!allArchived) {
    console.log(
      '\nAction required: Fix or remove the broken links listed above.'
    );
    console.log(
      'For links with Web Archive versions, you can replace them with the suggested archive.org URLs.'
    );
    process.exit(1);
  } else {
    console.log(
      '\nAll broken links have Web Archive versions. Consider replacing them with the suggested archive.org URLs.'
    );
    process.exit(0);
  }
}

export { extractErrorsSection, extractBrokenLinks };

// Only run when executed directly, so the unit tests can import the parsers.
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  main().catch((error) => {
    console.error('Unexpected error:', error);
    process.exit(1);
  });
}
