// Loads the workflow files for the policy tests without a YAML parser (the
// tests run with a bare `node --test`), shared by workflow-policy.test.mjs and
// ci-governance.test.mjs.
import { readFileSync, readdirSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
export const workflowsDir = join(repoRoot, '.github', 'workflows');

export const workflows = readdirSync(workflowsDir)
  .filter((name) => name.endsWith('.yml') || name.endsWith('.yaml'))
  .map((name) => ({
    name,
    path: join(workflowsDir, name),
    text: readFileSync(join(workflowsDir, name), 'utf8'),
  }));

/** Splits a workflow into `{ name, body }` entries, one per job. */
export function readJobs(text) {
  const lines = text.split('\n');
  const jobsStart = lines.findIndex((line) => line === 'jobs:');
  if (jobsStart === -1) {
    return [];
  }
  const jobs = [];
  let current = null;
  for (const line of lines.slice(jobsStart + 1)) {
    if (/^[A-Za-z]/.test(line)) {
      break; // back to a top-level key, the jobs mapping ended
    }
    const header = /^ {2}([A-Za-z0-9_-]+):\s*$/.exec(line);
    if (header) {
      current = { name: header[1], body: [] };
      jobs.push(current);
      continue;
    }
    if (current) {
      current.body.push(line);
    }
  }
  return jobs.map((job) => ({ name: job.name, body: job.body.join('\n') }));
}
