## Summary

`scripts/check-release-needed.mjs` sends **one request** to NuGet and one to the GitHub releases API to decide whether to release. Neither retries. A single dropped connection (Node's fetch throws `TypeError: fetch failed`), a 429 or a 5xx answer fails the release decision, and with it the release job of a push to `main`. Because the error message is the bare `fetch failed`, the network cause (`ECONNRESET`, `ETIMEDOUT`, …) that Node keeps in `error.cause.code` is lost.

Code (template `main` at 4cc372e):

- [`fetchNugetVersions`](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/4cc372ebf2075a0c948b46881143f7d66f0f0e7e/scripts/check-release-needed.mjs#L142-L158)
- [`fetchGithubReleaseExists`](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/4cc372ebf2075a0c948b46881143f7d66f0f0e7e/scripts/check-release-needed.mjs#L169-L196)

```js
const response = await fetchImpl(url);
if (response.status === 404) {
  return null;
}
if (!response.ok) {
  throw new Error(`NuGet flat-container index returned HTTP ${response.status} for ${packageId}`);
}
```

Failing is better than guessing here, so the decision is never wrong. But one transient failure turns a good release into a red run.

## Reproduction

`release-needed-transient-fetch.mjs` imports `fetchNugetVersions` from the given file. It passes a fake fetch that throws Node's dropped-connection error once and then answers like nuget.org:

```bash
node release-needed-transient-fetch.mjs /path/to/template/scripts/check-release-needed.mjs; echo "exit=$?"
```

```
Fetching https://api.nuget.org/v3-flatcontainer/example/index.json
FAILED after 1 request(s): fetch failed
exit=1
```

link-foundation/link-cli hit the same class of failure in its dependency check (run 40084225032: `coverlet.collector: fetch failed`). It fixed both scripts in issue 134, pull request 135. The reproduction script and its output are in that pull request:

- `dev/log/issues/134/pulls/135/scripts/release-needed-transient-fetch.mjs`
- `dev/log/issues/134/pulls/135/analysis/release-needed-transient-fetch-repro.txt`

That output also shows the fixed script retrying and succeeding on the second request.

## Workaround

Re-run the failed release job.

## Suggested fix

Wrap both lookups in a small retry helper:

- retry network errors, 429 and 5xx with exponential backoff;
- return any other response at once, so 404 still means "not published";
- name the network cause in the final error.

```js
import { setTimeout as sleep } from 'node:timers/promises';

export async function fetchWithRetries(url, options, fetchImpl = fetch, { attempts = 4, delayMs = 1000 } = {}) {
  for (let attempt = 1; ; attempt += 1) {
    let response;
    try {
      response = await fetchImpl(url, options);
    } catch (error) {
      if (attempt >= attempts) {
        const cause = error.cause ? `: ${error.cause.code ?? error.cause.message ?? error.cause}` : '';
        throw new Error(`${url}: ${error.message}${cause} (after ${attempt} attempts)`);
      }
    }
    const transient = !response || response.status === 429 || response.status >= 500;
    if (!transient || attempt >= attempts) {
      return response;
    }
    console.log(`Retrying ${url} (attempt ${attempt + 1} of ${attempts})`);
    await sleep(delayMs * 2 ** (attempt - 1));
  }
}
```

`fetchNugetVersions(packageId, fetchImpl = fetch, retry = {})` and `fetchGithubReleaseExists(repository, tag, fetchImpl = fetch, retry = {})` then call `fetchWithRetries(url, options, fetchImpl, retry)`. Passing `{ delayMs: 0 }` keeps the tests fast. Tests worth adding:

- a dropped connection, then success;
- exhausted attempts report `fetch failed: ETIMEDOUT (after 2 attempts)`;
- a 404 is not retried, while a 429 is.

link-cli has these tests in `csharp/scripts/release-scripts.test.mjs`.
