// Codecov is optional until activated; Actions always retains the real reports.
import { appendFileSync, readFileSync, readdirSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export async function codecovRequired(repository, { fetch = globalThis.fetch, warn = console.warn } = {}) {
  if (!/^[\w.-]+\/[\w.-]+$/.test(repository)) throw new Error('Invalid GitHub repository');
  const [owner, name] = repository.split('/');
  const response = await fetch(`https://api.codecov.io/api/v2/github/${owner}/repos/${name}/`, {
    signal: AbortSignal.timeout(30_000),
  });
  let activated = false;
  if (response.status !== 404) {
    if (!response.ok) throw new Error(`Codecov repository lookup failed: HTTP ${response.status}`);
    const body = await response.json();
    if (typeof body?.activated !== 'boolean') throw new Error('Codecov response has no boolean activated field');
    activated = body.activated;
  }
  if (!activated) {
    warn('::warning title=Codecov is not activated::The repository is not activated on Codecov; an OIDC upload will be attempted with fail_ci_if_error=false for this reason only. Coverage remains in the Actions summary and coverage artifact. Activate the repository on Codecov to require successful uploads.');
  }
  return activated;
}

export function summarizeCoverage(reports) {
  if (reports.length === 0) throw new Error('No Cobertura coverage reports found');
  const totals = { 'lines-valid': 0, 'lines-covered': 0, 'branches-valid': 0, 'branches-covered': 0 };
  for (const report of reports) {
    const root = /<coverage\b([^>]*)>/.exec(report)?.[1];
    if (!root) throw new Error('Invalid Cobertura coverage report');
    const attributes = Object.fromEntries([...root.matchAll(/([\w-]+)=["']([^"']*)["']/g)].map(([, key, value]) => [key, value]));
    for (const key of Object.keys(totals)) {
      if (!/^\d+$/.test(attributes[key] ?? '')) throw new Error(`Invalid Cobertura coverage total: ${key}`);
      const count = Number(attributes[key]);
      if (!Number.isSafeInteger(count)) throw new Error(`Invalid Cobertura coverage total: ${key}`);
      totals[key] += count;
    }
    for (const metric of ['lines', 'branches']) {
      if (Number(attributes[`${metric}-covered`]) > Number(attributes[`${metric}-valid`])) {
        throw new Error(`Invalid Cobertura coverage totals for ${metric}`);
      }
    }
  }
  const format = (metric) => {
    const valid = totals[`${metric}-valid`];
    const covered = totals[`${metric}-covered`];
    return `${covered} / ${valid} (${valid ? `${(100 * covered / valid).toFixed(2)}%` : 'n/a'})`;
  };
  return `## C# coverage\n\n| Metric | Covered / total |\n| --- | --- |\n| Lines | ${format('lines')} |\n| Branches | ${format('branches')} |\n\nFull Cobertura reports are available in the coverage artifact.\n`;
}

function findReports(directory) {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) return findReports(path);
    return entry.isFile() && entry.name === 'coverage.cobertura.xml' ? [readFileSync(path, 'utf8')] : [];
  });
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    if (process.argv[2] === 'codecov') {
      const required = await codecovRequired(process.env.GITHUB_REPOSITORY);
      appendFileSync(process.env.GITHUB_OUTPUT, `required=${required}\n`);
    } else if (process.argv[2] === 'summary') {
      const summary = summarizeCoverage(findReports(process.argv[3]));
      appendFileSync(process.env.GITHUB_STEP_SUMMARY, summary);
      console.log(summary);
    } else {
      throw new Error('Usage: coverage.mjs codecov | summary <results-directory>');
    }
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
