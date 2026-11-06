import assert from 'node:assert/strict';
import test from 'node:test';
import { validateAudit } from './nuget-audit.mjs';

const framework = {
  framework: 'net10.0',
  topLevelPackages: [{ id: 'Example', resolvedVersion: '1.0.0' }],
};
const project = { path: 'example.csproj', frameworks: [framework] };
const graph = { version: 1, projects: [project] };
const expected = ['example.csproj'];
const clean = { version: 1, projects: [{ path: 'example.csproj' }] };

test('a successful dotnet exit is insufficient without a complete, clean audit graph', () => {
  assert.doesNotThrow(() => validateAudit(clean, graph, expected));
  for (const invalid of [
    {},
    { ...clean, projects: [] },
    { ...clean, problems: ['provider failed'] },
    { ...clean, projects: [{ path: 'another.csproj' }] },
    {
      ...clean,
      projects: [
        {
          ...project,
          frameworks: [
            {
              ...framework,
              topLevelPackages: [
                { id: 'Example', resolvedVersion: '1.0.0', vulnerabilities: [{ severity: 'Low' }] },
              ],
            },
          ],
        },
      ],
    },
  ]) {
    assert.throws(() => validateAudit(invalid, graph, expected));
  }
  assert.throws(() =>
    validateAudit(clean, { ...graph, projects: [{ ...project, frameworks: [] }] }, expected),
  );
});
