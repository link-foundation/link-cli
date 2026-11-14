#!/usr/bin/env node
// Keeps one tracking issue for a red Dependencies check on main (issue #111).
//
// The daily Dependencies run failed on main on 2026-10-08 and 2026-10-09 and
// nobody noticed, because a failed scheduled run only emails whoever last
// edited the cron line. This script runs after the check on main (schedule,
// push and manual runs):
//   - a failed run opens the tracking issue, or comments on the open one;
//   - a passing run closes the open tracking issue with a link to the run.
// The issue is found by a marker in its body, so renaming it is harmless.
//
// Env: NEEDS_JSON         toJSON(needs) of the calling job
//      GITHUB_TOKEN       token with issues: write
//      GITHUB_REPOSITORY  owner/name
//      RUN_URL            link to the workflow run
//      TRACKING_ISSUE_DRY_RUN=1  print the decision without calling the API
//      TRACKING_ISSUE_DEBUG=1    log every API request (also on runner debug)
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const MARKER = '<!-- dependencies-tracking-issue (issue #111) -->';
export const TITLE = 'Dependencies check is failing on main';

/** Returns the jobs that did not succeed, from `toJSON(needs)`. */
export function failedJobs(needs) {
  if (!needs || typeof needs !== 'object' || Object.keys(needs).length === 0)
    throw new Error('NEEDS_JSON lists no jobs');
  return Object.entries(needs)
    .filter(([, job]) => job?.result !== 'success')
    .map(([name, job]) => `${name} (${job?.result ?? 'missing'})`);
}

/** Picks what to do: `create`, `comment`, `close` or `none`. */
export function decide(failed, open) {
  if (failed.length) return open ? 'comment' : 'create';
  return open ? 'close' : 'none';
}

export function failureText(failed, runUrl) {
  return [
    `The Dependencies workflow failed on main in ${runUrl}.`,
    '',
    `Jobs that did not succeed: ${failed.join(', ')}.`,
    '',
    'A failed registry lookup, or a dependency whose blocking issue is closed now, fails',
    'the check on main; see the header of .github/scripts/check-latest-dependencies.mjs.',
    'Fix the cause on a branch, or hold the dependency back with a comment linking an open',
    'issue. The next passing run on main closes this issue.',
  ].join('\n');
}

/** Talks to the GitHub REST API; `request` is replaced in tests. */
export function client({ token, repository, debug = false, log = console.log }) {
  return async function request(method, path, body) {
    const url = `https://api.github.com/repos/${repository}${path}`;
    if (debug) log(`${method} ${url}${body ? ` ${JSON.stringify(body)}` : ''}`);
    const response = await fetch(url, {
      method,
      headers: {
        accept: 'application/vnd.github+json',
        authorization: `Bearer ${token}`,
        'x-github-api-version': '2022-11-28',
      },
      body: body ? JSON.stringify(body) : undefined,
    });
    if (!response.ok)
      throw new Error(`${method} ${path}: ${response.status} ${await response.text()}`);
    return response.json();
  };
}

export async function findOpen(request) {
  for (let page = 1; page <= 10; page++) {
    const issues = await request('GET', `/issues?state=open&per_page=100&page=${page}`);
    const found = issues.find((issue) => !issue.pull_request && issue.body?.includes(MARKER));
    if (found || issues.length < 100) return found;
  }
  return undefined;
}

export async function track({ needs, runUrl, request, dryRun = false, log = console.log }) {
  const failed = failedJobs(needs);
  const open = await findOpen(request);
  const action = decide(failed, open);
  log(
    `Dependencies on main: ${failed.length ? `failed (${failed.join(', ')})` : 'passed'}; ` +
      `tracking issue: ${open ? `#${open.number}` : 'none open'}; action: ${action}`,
  );
  if (dryRun || action === 'none') return action;
  if (action === 'create') {
    const issue = await request('POST', '/issues', {
      title: TITLE,
      body: `${MARKER}\n${failureText(failed, runUrl)}`,
    });
    log(`Opened #${issue.number}`);
  } else if (action === 'comment') {
    await request('POST', `/issues/${open.number}/comments`, {
      body: `Still failing. ${failureText(failed, runUrl)}`,
    });
    log(`Commented on #${open.number}`);
  } else {
    await request('POST', `/issues/${open.number}/comments`, {
      body: `The Dependencies check passed on main again in ${runUrl}.`,
    });
    await request('PATCH', `/issues/${open.number}`, {
      state: 'closed',
      state_reason: 'completed',
    });
    log(`Closed #${open.number}`);
  }
  return action;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const env = process.env;
  const debug = env.TRACKING_ISSUE_DEBUG === '1' || env.RUNNER_DEBUG === '1';
  track({
    needs: JSON.parse(env.NEEDS_JSON || '{}'),
    runUrl: env.RUN_URL,
    dryRun: env.TRACKING_ISSUE_DRY_RUN === '1',
    request: client({ token: env.GITHUB_TOKEN, repository: env.GITHUB_REPOSITORY, debug }),
  }).catch((error) => {
    console.error(`::error::${error.message}`);
    process.exitCode = 1;
  });
}
