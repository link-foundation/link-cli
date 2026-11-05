import { spawnSync } from 'node:child_process';
import { mkdirSync, writeFileSync, appendFileSync } from 'node:fs';
import { validateAudit } from './nuget-audit.mjs';
mkdirSync('artifacts/security', { recursive: true });
const listed = spawnSync('git', ['ls-files', '*.csproj'], { encoding: 'utf8' });
if (listed.status !== 0) throw Error('Cannot enumerate the tracked project graph');
const expected = listed.stdout.trim().split('\n').filter((file) => file && !file.startsWith('dev/log/'));
if (!expected.length) throw Error('No project graph to audit');
const combined = { version: 1, projects: [] };
const audit = { version: 1, projects: [] };
function run(args) {
  const result = spawnSync('dotnet', args, {
    encoding: 'utf8',
    maxBuffer: 16 * 1024 * 1024,
  });
  appendFileSync(
    'artifacts/security/commands.log',
    `${JSON.stringify(args)}\n${result.stdout || ''}${result.stderr || ''}\n`,
  );
  if (result.status !== 0 || result.error || result.stderr?.trim())
    throw Error(
      `Dependency audit command failed: ${result.stdout || ''}${result.stderr || ''}`,
    );
  return result.stdout;
}
writeFileSync('artifacts/security/projects.json', JSON.stringify(expected, null, 2));
for (const project of expected) {
  run([
    'restore',
    project,
    '--force-evaluate',
    '-p:NuGetAudit=true',
    '-p:NuGetAuditMode=all',
    '-p:NuGetAuditLevel=low',
    '-warnaserror',
  ]);
  for (const vulnerable of [false, true]) {
    const target = vulnerable ? audit : combined;
    const output = run([
      'list',
      project,
      'package',
      '--include-transitive',
      '--format',
      'json',
      ...(vulnerable ? ['--vulnerable'] : []),
    ]);
    const data = JSON.parse(output);
    if (
      data.version !== 1 ||
      data.problems?.length ||
      !Array.isArray(data.projects) ||
      !data.projects.length
    )
      throw Error('Unverified audit provider result');
    target.projects.push(...data.projects);
    writeFileSync(
      `artifacts/security/${vulnerable ? 'audit' : 'dependency-graph'}.json`,
      JSON.stringify(target, null, 2),
    );
  }
}
writeFileSync(
  'artifacts/security/dependency-graph.json',
  JSON.stringify(combined, null, 2),
);
writeFileSync('artifacts/security/audit.json', JSON.stringify(audit, null, 2));
validateAudit(audit, combined, expected);
