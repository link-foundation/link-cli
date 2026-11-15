import { execFileSync } from 'node:child_process';
import { mkdtempSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { readCsprojInfo } from './check-release-needed.mjs';

export const repoRoot = fileURLToPath(new URL('../..', import.meta.url));

export function excludedProjects() {
  return execFileSync(
    'git',
    ['ls-files', '-z', '--', 'examples/*.csproj', 'experiments/*.csproj'],
    {
      cwd: repoRoot,
      encoding: 'utf8',
    },
  )
    .split('\0')
    .filter(Boolean);
}

export function verifyPackages(files, packages) {
  const versions = new Set(packages.map(({ version }) => version));
  if (versions.size !== 1) throw Error('CLI and library package versions must match');
  const expected = packages.map(({ packageId, version }) => `${packageId}.${version}.nupkg`).sort();
  const actual = files.filter((file) => /\.(s?nupkg)$/.test(file)).sort();
  if (JSON.stringify(actual) !== JSON.stringify(expected)) {
    throw Error(
      `Unexpected NuGet artifacts: ${JSON.stringify(actual)}; expected ${JSON.stringify(expected)}`,
    );
  }
}

function packExcluded() {
  const output = mkdtempSync(join(tmpdir(), 'link-cli-excluded-pack-'));
  try {
    const projects = excludedProjects();
    if (!projects.length) throw Error('No example/experiment projects found');
    for (const project of projects) {
      try {
        execFileSync(
          'dotnet',
          ['pack', project, '--no-restore', '--configuration', 'Release', '--output', output],
          {
            cwd: repoRoot,
            stdio: 'pipe',
          },
        );
      } catch (error) {
        throw Error(
          `dotnet pack failed for ${project}: ${error.stdout?.toString()} ${error.stderr?.toString()}`,
          { cause: error },
        );
      }
    }
    if (readdirSync(output).length) throw Error('Examples/experiments produced NuGet artifacts');
    console.log(
      `dotnet pack produced no artifacts for ${projects.length} example/experiment projects`,
    );
  } finally {
    rmSync(output, { recursive: true, force: true });
  }
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    if (process.argv.includes('--pack-excluded')) {
      packExcluded();
    } else {
      const packages = ['Foundation.Data.Doublets.Cli', 'Foundation.Data.Doublets.Cli.Library'].map(
        (project) => readCsprojInfo(join(repoRoot, 'csharp', project, `${project}.csproj`)),
      );
      verifyPackages(readdirSync(process.argv[2] || join(repoRoot, 'csharp/artifacts')), packages);
      console.log('Only clink and Foundation.Data.Doublets.Cli packages were produced');
    }
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
