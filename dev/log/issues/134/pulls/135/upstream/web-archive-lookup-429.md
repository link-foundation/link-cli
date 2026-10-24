## Summary

`scripts/check-web-archive.mjs` asks the Wayback Machine availability API **once** per broken link and reads **any failed lookup as "not archived"**: a 429, a 5xx, a dropped connection, the 10 s abort or an unreadable answer. The job then fails with

```
::error title=Broken link - No Web Archive fallback::Broken link detected: <url>
No archived version was found in the Wayback Machine.
How to fix:
  1. Find an updated URL for the same or equivalent content and replace the link.
  ...
```

That message is wrong whenever the archive was never actually asked, and the author is told to replace a link that may well have a snapshot. archive.org rate-limits shared CI addresses: on 2026-10-24 it answered every availability request from our runner host with `HTTP/2 429` (an HTML "429 Too Many Requests" page, no `Retry-After` header).

Code ({{TEMPLATE}} template `main` at {{SHA}}):

- [`checkWaybackMachine`]({{CODE_URL}})

```js
if (!response.ok) {
  console.warn(`  Wayback API returned ${response.status} for ${url}`);
  return { available: false, archiveUrl: null, timestamp: null };
}
...
} catch (error) {
  console.warn(`  Failed to check Wayback Machine for ${url}: ${error.message}`);
  return { available: false, archiveUrl: null, timestamp: null };
}
```

## Reproduction

`repro-web-archive-429.sh` copies the given `check-web-archive.mjs` into a temporary directory and points its Wayback API at a local fake. The fake answers the first request with 429 and has a snapshot for every URL after that. The script then runs it on a lychee report with one broken link:

```bash
repro-web-archive-429.sh /path/to/template/scripts/check-web-archive.mjs; echo "exit=$?"
```

Output against the template (abridged):

```
Checking: https://example.com/moved-page
  Wayback API returned 429 for https://example.com/moved-page
  ✗ Not found in Web Archive
...
::error title=Broken link - No Web Archive fallback::Broken link detected: https://example.com/moved-page
No archived version was found in the Wayback Machine.
exit=1
== fake Wayback API requests
GET /wayback/available?url=https%3A%2F%2Fexample.com%2Fmoved-page
  -> 429
```

The script needs only `node` and `python3`. It lives in link-foundation/link-cli (issue 134, pull request 135) at `dev/log/issues/134/pulls/135/scripts/repro-web-archive-429.sh`. The outputs are in `dev/log/issues/134/pulls/135/analysis/`:

- `template-web-archive-429-repro.txt`: all three templates;
- `web-archive-429-repro.txt`: link-cli before and after the fix. After one 429 the fixed script retries and reports the snapshot (exit 0); with a persistent 429 it reports `Web Archive lookup failed: HTTP 429 (after 3 attempts)`;
- `web-archive-lookup-429.txt`: the same comparison against the live archive.org 429.

The rust, js and csharp templates share this code; it is reported to each of them.

## Workaround

Re-run the link check later. Before replacing a link, look the URL up on https://web.archive.org/ by hand.

## Suggested fix

1. Retry network errors, 429, 5xx and unreadable JSON with backoff.
2. Return a definite "no snapshot" answer (an empty `archived_snapshots`) at once, without retrying.
3. When the attempts run out, return an `error` and report it as **"Web Archive lookup failed"** with a re-run hint, never as "No archived version".

```js
import { setTimeout as sleep } from 'node:timers/promises';

export async function checkWaybackMachine(
  url,
  { fetchImpl = fetch, attempts = 3, delayMs = 2000, timeoutMs = 10000 } = {}
) {
  const apiUrl = `${WAYBACK_API}${encodeURIComponent(url)}`;
  const notArchived = { available: false, archiveUrl: null, timestamp: null };
  let failure = '';

  for (let attempt = 1; attempt <= attempts; attempt += 1) {
    if (attempt > 1) {
      console.log(`  Retrying the Wayback API (attempt ${attempt} of ${attempts}) after: ${failure}`);
      await sleep(delayMs * 2 ** (attempt - 2));
    }
    let response;
    try {
      response = await fetchImpl(apiUrl, {
        headers: { 'User-Agent': 'broken-link-checker/1.0 (GitHub Actions CI)' },
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
      return {
        available: true,
        archiveUrl: snapshot.url.replace(/^http:\/\//, 'https://'),
        timestamp: snapshot.timestamp,
      };
    }
    return notArchived;
  }
  return { ...notArchived, error: `${failure} (after ${attempts} attempts)` };
}
```

In `main`, collect the results that have an `error` in their own list. Print them as `::error title=Broken link - Web Archive lookup failed::… How to fix: re-run the job. If the lookup keeps failing, fix or remove the link.` and require that list to be empty for `all_archived=true`. Exporting the function with an injectable `fetchImpl` makes it testable without network access. link-cli has these tests in `.github/scripts/check-web-archive.test.mjs`:

- a dropped connection, then a snapshot;
- a persistent failure reports `fetch failed: ECONNRESET (after 2 attempts)`;
- a definite "no snapshot" answer is not retried, and a 403 is reported after one request.
