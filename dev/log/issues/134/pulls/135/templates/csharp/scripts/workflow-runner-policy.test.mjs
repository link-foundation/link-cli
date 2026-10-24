import { describe, expect, test } from 'bun:test';
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const WORKFLOW_DIR = '.github/workflows';
function floatingRunners(workflow) {
  const findings = [];
  function visit(value, location) {
    if (typeof value === 'string' && /\b[\w-]+-latest\b/.test(value)) {
      findings.push(`${location}: ${value}`);
    } else if (Array.isArray(value)) {
      value.forEach((entry, index) => visit(entry, `${location}[${index}]`));
    } else if (value && typeof value === 'object') {
      for (const [key, entry] of Object.entries(value)) {
        visit(entry, `${location}.${key}`);
      }
    }
  }
  for (const [name, job] of Object.entries(workflow.jobs ?? {})) {
    visit(job['runs-on'], `jobs.${name}.runs-on`);
    visit(job.strategy?.matrix, `jobs.${name}.strategy.matrix`);
  }
  return findings;
}

describe('workflow runner policy', () => {
  test('rejects floating runners in scalars, lists, label objects, and matrix includes', () => {
    const workflow = Bun.YAML.parse(`
jobs:
  scalar:
    runs-on: ubuntu-latest
  list:
    runs-on: [self-hosted, windows-latest]
  labels:
    runs-on:
      labels: macos-latest
  matrix:
    runs-on: \${{ matrix.os }}
    strategy:
      matrix:
        os:
          - ubuntu-latest
        include:
          - os: windows-latest
`);
    expect(floatingRunners(workflow)).toHaveLength(5);
  });

  test('pins runners in every workflow, including multiline matrices', () => {
    const findings = readdirSync(WORKFLOW_DIR)
      .filter((name) => /\.ya?ml$/.test(name))
      .flatMap((name) => floatingRunners(
        Bun.YAML.parse(readFileSync(join(WORKFLOW_DIR, name), 'utf-8'))
      ).map((finding) => `${name}: ${finding}`));
    expect(findings, findings.join('\n')).toEqual([]);
  });

  test('keeps all three pinned platforms and .NET 8 in the release matrix', () => {
    const workflow = Bun.YAML.parse(readFileSync(join(WORKFLOW_DIR, 'release.yml'), 'utf-8'));
    expect(workflow.jobs.test.strategy.matrix.os).toEqual([
      'ubuntu-24.04', 'macos-15', 'windows-2025',
    ]);
    expect(workflow.jobs.test.steps.find((step) => step.uses?.startsWith('actions/setup-dotnet@'))
      .with['dotnet-version']).toBe('8.0.x');
  });
});
