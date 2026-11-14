import assert from 'node:assert/strict';
import test from 'node:test';
import { MARKER, decide, failedJobs, findOpen, track } from './dependency-tracking-issue.mjs';

const ok = { javascript: { result: 'success' }, latest: { result: 'success' } };
const red = { javascript: { result: 'success' }, latest: { result: 'failure' } };

/** A fake API that records every write and serves `issues` on GET. */
function fakeApi(issues) {
  const calls = [];
  const request = async (method, path, body) => {
    calls.push([method, path, body]);
    if (method === 'GET') return issues;
    return { number: 7 };
  };
  return { calls, request };
}

test('any job that did not succeed counts as a failure', () => {
  assert.deepEqual(failedJobs(ok), []);
  assert.deepEqual(failedJobs({ javascript: { result: 'failure' }, latest: { result: 'skipped' } }), [
    'javascript (failure)',
    'latest (skipped)',
  ]);
  assert.throws(() => failedJobs({}), /lists no jobs/);
  assert.equal(decide(['x'], undefined), 'create');
  assert.equal(decide(['x'], { number: 1 }), 'comment');
  assert.equal(decide([], { number: 1 }), 'close');
  assert.equal(decide([], undefined), 'none');
});

test('the tracking issue is found by its marker, never by a pull request', async () => {
  const { request } = fakeApi([
    { number: 1, body: MARKER, pull_request: {} },
    { number: 2, body: 'unrelated' },
    { number: 3, body: `${MARKER}\ntext` },
  ]);
  assert.equal((await findOpen(request)).number, 3);
});

test('a failure opens one issue, then comments on it; a pass closes it', async () => {
  const log = () => {};
  const runUrl = 'https://example.test/runs/1';
  const fresh = fakeApi([]);
  assert.equal(await track({ needs: red, runUrl, request: fresh.request, log }), 'create');
  const [, path, body] = fresh.calls.at(-1);
  assert.equal(path, '/issues');
  assert.ok(body.body.startsWith(MARKER));
  assert.match(body.body, /latest \(failure\).*\n?/);

  const open = fakeApi([{ number: 7, body: MARKER }]);
  assert.equal(await track({ needs: red, runUrl, request: open.request, log }), 'comment');
  assert.equal(open.calls.at(-1)[1], '/issues/7/comments');

  const fixed = fakeApi([{ number: 7, body: MARKER }]);
  assert.equal(await track({ needs: ok, runUrl, request: fixed.request, log }), 'close');
  assert.deepEqual(fixed.calls.at(-1), [
    'PATCH',
    '/issues/7',
    { state: 'closed', state_reason: 'completed' },
  ]);

  const quiet = fakeApi([]);
  assert.equal(await track({ needs: ok, runUrl, request: quiet.request, log }), 'none');
  assert.equal(quiet.calls.length, 1);
});

test('a dry run only reads', async () => {
  const api = fakeApi([]);
  const lines = [];
  const action = await track({
    needs: red,
    runUrl: 'u',
    request: api.request,
    dryRun: true,
    log: (line) => lines.push(line),
  });
  assert.equal(action, 'create');
  assert.deepEqual(
    api.calls.map(([method]) => method),
    ['GET'],
  );
  assert.match(lines[0], /failed \(latest \(failure\)\).*action: create/);
});
