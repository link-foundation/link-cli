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
 *   node scripts/check-web-archive.mjs
 *
 * Environment variables:
 *   - LYCHEE_OUTPUT: Path to lychee markdown output file (default: lychee/out.md)
 *
 * GitHub Actions outputs:
 *   - all_archived: 'true' if all broken links have a web archive version
 *
 * Exit codes:
 *   - 0: All broken links have web archive versions (or no broken links)
 *   - 1: Some broken links have no web archive version
 */

import { readFileSync, appendFileSync, existsSync } from 'fs';

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
 * Extract broken URLs from lychee markdown output
 * Lychee markdown format includes lines like:
 *   * [404] https://example.com/broken-link
 *   * [ERROR] https://another-broken.com
 * @param {string} content - The markdown content from lychee
 * @returns {string[]} Array of broken URLs
 */
function extractBrokenUrls(content) {
  const urls = [];

  // Match lines with error status codes or ERROR markers followed by URLs
  // Lychee output format: [STATUS_CODE] URL or bullet points with links
  const urlPattern =
    /\[(?:4\d\d|5\d\d|ERROR|TIMEOUT|UNKNOWN)\]\s+(https?:\/\/[^\s)]+)/gi;
  let match;

  while ((match = urlPattern.exec(content)) !== null) {
    const url = match[1].trim();
    if (url && !urls.includes(url)) {
      urls.push(url);
    }
  }

  // Also match plain URL lines in broken sections
  // Lychee sometimes outputs: `[ERROR] url | description`
  const linePattern = /^\s*(?:\*|-)\s+.*?(https?:\/\/[^\s|)>\]]+)/gm;
  let lineMatch;

  while ((lineMatch = linePattern.exec(content)) !== null) {
    const url = lineMatch[1].trim().replace(/[.,;!?]+$/, '');
    if (url && !urls.includes(url) && url.startsWith('http')) {
      urls.push(url);
    }
  }

  return urls;
}

/**
 * Check if a URL has an archived version in the Wayback Machine
 * Uses the Wayback Machine Availability API:
 * https://archive.org/help/wayback_api.php
 * @param {string} url - The URL to check
 * @returns {Promise<{available: boolean, archiveUrl: string|null, timestamp: string|null}>}
 */
async function checkWaybackMachine(url) {
  const apiUrl = `${WAYBACK_API}${encodeURIComponent(url)}`;

  const controller = new AbortController();
  const timeoutId = globalThis.setTimeout(() => controller.abort(), 10000);

  try {
    const response = await fetch(apiUrl, {
      headers: {
        'User-Agent': 'broken-link-checker/1.0 (GitHub Actions CI)',
      },
      signal: controller.signal,
    });

    if (!response.ok) {
      console.warn(`  Wayback API returned ${response.status} for ${url}`);
      return { available: false, archiveUrl: null, timestamp: null };
    }

    const data = await response.json();

    if (data.archived_snapshots?.closest?.available === true) {
      const snapshot = data.archived_snapshots.closest;
      const archiveUrl = snapshot.url.replace(/^http:\/\//, 'https://');
      return {
        available: true,
        archiveUrl,
        timestamp: snapshot.timestamp,
      };
    }

    return { available: false, archiveUrl: null, timestamp: null };
  } catch (error) {
    console.warn(
      `  Failed to check Wayback Machine for ${url}: ${error.message}`
    );
    return { available: false, archiveUrl: null, timestamp: null };
  } finally {
    globalThis.clearTimeout(timeoutId);
  }
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
  const brokenUrls = extractBrokenUrls(content);

  if (brokenUrls.length === 0) {
    console.log('No broken URLs found in lychee output.');
    setOutput('all_archived', 'true');
    process.exit(0);
  }

  console.log(
    `Found ${brokenUrls.length} broken URL(s). Checking Web Archive...\n`
  );

  const withArchive = [];
  const withoutArchive = [];

  for (const url of brokenUrls) {
    console.log(`Checking: ${url}`);
    const result = await checkWaybackMachine(url);

    if (result.available) {
      const date = formatTimestamp(result.timestamp);
      console.log(`  ✓ Archived on ${date}: ${result.archiveUrl}`);
      withArchive.push({ url, archiveUrl: result.archiveUrl, date });
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

  const allArchived = withoutArchive.length === 0;
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

main().catch((error) => {
  console.error('Unexpected error:', error);
  process.exit(1);
});
