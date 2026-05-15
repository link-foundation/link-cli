import assert from 'node:assert/strict';
import { execFile, execFileSync } from 'node:child_process';
import {
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from 'node:fs';
import { createServer } from 'node:http';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import test from 'node:test';
import { promisify } from 'node:util';

import { decide, readCsprojInfo } from './check-release-needed.mjs';
import {
  DEFAULT_MAX_ATTEMPTS,
  DEFAULT_SLEEP_SECONDS,
  createNugetNuspecUrl,
  parseArgs as parseWaitForNugetArgs,
  waitForNugetPackage,
} from './wait-for-nuget.mjs';
import {
  buildNugetBadges,
  buildReleasePayload,
  prependNugetBadges,
} from './create-github-release.mjs';

const execFileAsync = promisify(execFile);

const repoRoot = new URL('../..', import.meta.url).pathname;

function runNode(script, args, options = {}) {
  return execFileSync(process.execPath, [join(repoRoot, script), ...args], {
    cwd: options.cwd ?? repoRoot,
    encoding: 'utf8',
    env: { ...process.env, ...(options.env ?? {}) },
  });
}

async function runNodeAsync(script, args, options = {}) {
  const { stdout } = await execFileAsync(
    process.execPath,
    [join(repoRoot, script), ...args],
    {
      cwd: options.cwd ?? repoRoot,
      encoding: 'utf8',
      env: { ...process.env, ...(options.env ?? {}) },
    }
  );
  return stdout;
}

function runGit(args, cwd) {
  return execFileSync('git', args, {
    cwd,
    encoding: 'utf8',
  });
}

test('merge-changesets honors the requested directory and package name', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-changesets-'));
  const changesetDir = join(dir, 'csharp', '.changeset');
  mkdirSync(changesetDir, { recursive: true });

  writeFileSync(
    join(changesetDir, 'one.md'),
    "---\n'Foundation.Data.Doublets.Cli': patch\n---\n\nFirst fix.\n"
  );
  writeFileSync(
    join(changesetDir, 'two.md'),
    "---\n'Foundation.Data.Doublets.Cli': minor\n---\n\nSecond fix.\n"
  );

  runNode('csharp/scripts/merge-changesets.mjs', [
    '--dir',
    changesetDir,
    '--package-name',
    'Foundation.Data.Doublets.Cli',
  ]);

  const files = readdirSync(changesetDir).filter((file) => file.endsWith('.md'));
  assert.equal(files.length, 1);
  const merged = readFileSync(join(changesetDir, files[0]), 'utf8');
  assert.match(merged, /'Foundation\.Data\.Doublets\.Cli': minor/);
  assert.match(merged, /First fix\./);
  assert.match(merged, /Second fix\./);
});

test('create-github-release dry run uses tag prefix and component changelog', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-release-'));
  const changelog = join(dir, 'CHANGELOG.md');
  writeFileSync(
    changelog,
    '# Changelog\n\n## [2.4.0] - 2026-05-12\n\nFixed release automation.\n\n## [2.3.0] - 2026-05-01\n\nPrevious release.\n'
  );

  const stdout = runNode('csharp/scripts/create-github-release.mjs', [
    '--release-version',
    '2.4.0',
    '--repository',
    'link-foundation/link-cli',
    '--tag-prefix',
    'csharp-v',
    '--language',
    'C#',
    '--package-id',
    'clink',
    '--changelog-path',
    changelog,
    '--dry-run',
  ]);
  const payload = JSON.parse(stdout.slice(stdout.indexOf('{')));

  assert.equal(payload.tag_name, 'csharp-v2.4.0');
  assert.equal(payload.name, 'C# v2.4.0');
  assert.match(payload.body, /Fixed release automation\./);
  assert.match(payload.body, /Package: `clink`/);
  // Issue #88: the body must lead with NuGet badges that link to the
  // exact released version (not the package landing page).
  assert.match(
    payload.body,
    /\[!\[NuGet\]\(https:\/\/img\.shields\.io\/nuget\/v\/clink\?logo=nuget&label=NuGet\)\]\(https:\/\/www\.nuget\.org\/packages\/clink\/2\.4\.0\)/
  );
  assert.match(
    payload.body,
    /\[!\[NuGet Downloads\]\(https:\/\/img\.shields\.io\/nuget\/dt\/clink\?logo=nuget&label=downloads\)\]\(https:\/\/www\.nuget\.org\/packages\/clink\/2\.4\.0\)/
  );
});

test('create-github-release dry run omits NuGet badges when no package id is provided', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-release-no-package-'));
  const changelog = join(dir, 'CHANGELOG.md');
  writeFileSync(
    changelog,
    '# Changelog\n\n## [2.4.0] - 2026-05-12\n\nFixed release automation.\n'
  );

  const stdout = runNode('csharp/scripts/create-github-release.mjs', [
    '--release-version',
    '2.4.0',
    '--repository',
    'link-foundation/link-cli',
    '--tag-prefix',
    'csharp-v',
    '--language',
    'C#',
    '--changelog-path',
    changelog,
    '--dry-run',
  ]);
  const payload = JSON.parse(stdout.slice(stdout.indexOf('{')));

  assert.doesNotMatch(payload.body, /img\.shields\.io\/nuget/);
  assert.doesNotMatch(payload.body, /Package: `/);
});

test('buildNugetBadges links both badges to the exact version', () => {
  const badges = buildNugetBadges('clink', '2.4.0');

  assert.match(badges, /img\.shields\.io\/nuget\/v\/clink\?/);
  assert.match(badges, /img\.shields\.io\/nuget\/dt\/clink\?/);
  // Both clickable badge targets (outer `](url)` of each markdown link) must
  // point to the version-specific NuGet URL.
  const targets = [...badges.matchAll(/\)\]\(([^)]+)\)/g)].map((m) => m[1]);
  assert.equal(targets.length, 2);
  for (const target of targets) {
    assert.equal(target, 'https://www.nuget.org/packages/clink/2.4.0');
  }
});

test('buildNugetBadges URL-encodes the package id', () => {
  const badges = buildNugetBadges('My.Package', '1.0.0');

  assert.match(badges, /nuget\/v\/My\.Package/);
  assert.match(badges, /packages\/My\.Package\/1\.0\.0/);
});

test('prependNugetBadges keeps existing shields.io NuGet badges intact', () => {
  const notes =
    '[![NuGet](https://img.shields.io/nuget/v/clink?label=NuGet)](https://www.nuget.org/packages/clink)\n\nExisting notes.';

  const result = prependNugetBadges(notes, 'clink', '2.4.0');

  assert.equal(result, notes);
});

test('prependNugetBadges is a no-op without a package id', () => {
  const notes = 'Release v2.4.0.';

  assert.equal(prependNugetBadges(notes, '', '2.4.0'), notes);
  assert.equal(prependNugetBadges(notes, 'clink', ''), notes);
});

test('buildNugetBadges accepts multiple package ids', () => {
  const badges = buildNugetBadges(['clink', 'Foundation.Data.Doublets.Cli'], '2.4.0');

  assert.match(badges, /img\.shields\.io\/nuget\/v\/clink\?/);
  assert.match(
    badges,
    /img\.shields\.io\/nuget\/v\/Foundation\.Data\.Doublets\.Cli\?/
  );
  assert.match(badges, /img\.shields\.io\/nuget\/dt\/clink\?/);
  assert.match(
    badges,
    /img\.shields\.io\/nuget\/dt\/Foundation\.Data\.Doublets\.Cli\?/
  );
});

test('buildReleasePayload includes badges for both CLI and library packages', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-dual-release-payload-'));
  const changelog = join(dir, 'CHANGELOG.md');
  writeFileSync(
    changelog,
    '# Changelog\n\n## [2.4.0] - 2026-05-12\n\nDual package release.\n'
  );

  const payload = buildReleasePayload({
    changelogPath: changelog,
    language: 'C#',
    packageIds: ['clink', 'Foundation.Data.Doublets.Cli'],
    releaseVersion: '2.4.0',
    tagPrefix: 'csharp-v',
  });

  assert.match(payload.body, /img\.shields\.io\/nuget\/v\/clink\?/);
  assert.match(
    payload.body,
    /img\.shields\.io\/nuget\/v\/Foundation\.Data\.Doublets\.Cli\?/
  );
  assert.match(
    payload.body,
    /Packages: `clink`, `Foundation\.Data\.Doublets\.Cli`/
  );
});

test('buildReleasePayload places NuGet badges above the package footer', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-release-payload-'));
  const changelog = join(dir, 'CHANGELOG.md');
  writeFileSync(
    changelog,
    '# Changelog\n\n## [2.4.0] - 2026-05-12\n\nFirst line.\nSecond line.\n'
  );

  const payload = buildReleasePayload({
    changelogPath: changelog,
    language: 'C#',
    packageId: 'clink',
    releaseVersion: '2.4.0',
    tagPrefix: 'csharp-v',
  });

  assert.equal(payload.tag_name, 'csharp-v2.4.0');
  assert.equal(payload.name, 'C# v2.4.0');
  const badgesIndex = payload.body.indexOf('![NuGet]');
  const notesIndex = payload.body.indexOf('First line.');
  const footerIndex = payload.body.indexOf('Package: `clink`');
  assert.notEqual(badgesIndex, -1);
  assert.notEqual(notesIndex, -1);
  assert.notEqual(footerIndex, -1);
  assert.ok(
    badgesIndex < notesIndex && notesIndex < footerIndex,
    `expected badges < notes < footer, got ${badgesIndex} ${notesIndex} ${footerIndex}`
  );
});

test('create-github-release dry run reports matching assets without uploading', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-release-assets-'));
  const changelog = join(dir, 'CHANGELOG.md');
  writeFileSync(
    changelog,
    '# Changelog\n\n## [2.4.0] - 2026-05-12\n\nFixed release automation.\n'
  );
  const artifacts = join(dir, 'artifacts');
  mkdirSync(artifacts, { recursive: true });
  writeFileSync(join(artifacts, 'clink.2.4.0.nupkg'), 'fake');
  writeFileSync(join(artifacts, 'clink.2.4.0.snupkg'), 'fake');
  writeFileSync(join(artifacts, 'unrelated.txt'), 'fake');

  const stdout = runNode('csharp/scripts/create-github-release.mjs', [
    '--release-version',
    '2.4.0',
    '--repository',
    'link-foundation/link-cli',
    '--tag-prefix',
    'csharp-v',
    '--changelog-path',
    changelog,
    '--assets-glob',
    join(artifacts, '*.nupkg'),
    '--dry-run',
  ]);
  const payload = JSON.parse(stdout.slice(stdout.indexOf('{')));

  assert.equal(payload.tag_name, 'csharp-v2.4.0');
});

test('version-and-commit creates a C# release commit when the next tag is missing', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-csharp-version-'));
  const remote = join(dir, 'remote.git');
  const work = join(dir, 'work');
  const outputFile = join(dir, 'github-output.txt');

  mkdirSync(work, { recursive: true });
  runGit(['init', '--bare', '--initial-branch=main', remote], dir);
  runGit(['init', '-b', 'main'], work);
  runGit(['config', 'user.name', 'Test User'], work);
  runGit(['config', 'user.email', 'test@example.com'], work);

  const projectDir = join(work, 'csharp', 'Foundation.Data.Doublets.Cli');
  const changesetDir = join(work, 'csharp', '.changeset');
  mkdirSync(projectDir, { recursive: true });
  mkdirSync(changesetDir, { recursive: true });
  writeFileSync(
    join(projectDir, 'Foundation.Data.Doublets.Cli.csproj'),
    '<Project Sdk="Microsoft.NET.Sdk"><PropertyGroup><Version>2.3.0</Version></PropertyGroup></Project>\n'
  );
  writeFileSync(
    join(changesetDir, 'release.md'),
    "---\n'Foundation.Data.Doublets.Cli': minor\n---\n\nFix release automation.\n"
  );

  runGit(['add', '.'], work);
  runGit(['commit', '-m', 'initial'], work);
  runGit(['remote', 'add', 'origin', remote], work);
  runGit(['push', '-u', 'origin', 'main'], work);

  runNode('csharp/scripts/version-and-commit.mjs', ['--mode', 'changeset'], {
    cwd: work,
    env: { GITHUB_OUTPUT: outputFile },
  });

  const csproj = readFileSync(
    join(projectDir, 'Foundation.Data.Doublets.Cli.csproj'),
    'utf8'
  );
  const outputs = readFileSync(outputFile, 'utf8');

  assert.match(csproj, /<Version>2\.4\.0<\/Version>/);
  assert.match(outputs, /^version_committed=true$/m);
  assert.match(outputs, /^new_version=2\.4\.0$/m);
  assert.match(runGit(['rev-parse', '--verify', 'csharp-v2.4.0'], work), /[a-f0-9]{40}/);
  assert.match(runGit(['ls-remote', '--tags', 'origin', 'csharp-v2.4.0'], work), /csharp-v2\.4\.0/);
});

test('check-release-needed decide(): changesets take the normal release path', () => {
  const result = decide({
    hasChangesets: true,
    currentVersion: '2.4.0',
    publishedVersions: ['2.2.2'],
    githubReleaseExists: false,
  });
  assert.equal(result.shouldRelease, true);
  assert.equal(result.skipBump, false);
  assert.equal(result.nugetPublished, false);
  assert.match(result.reason, /changesets present/);
});

test('check-release-needed decide(): self-heals when csproj version is missing from NuGet', () => {
  const result = decide({
    hasChangesets: false,
    currentVersion: '2.4.0',
    publishedVersions: ['2.2.2', '2.3.0'],
    githubReleaseExists: false,
  });
  assert.equal(result.shouldRelease, true);
  assert.equal(result.skipBump, true);
  assert.equal(result.nugetPublished, false);
  assert.match(result.reason, /not yet published on NuGet/);
});

test('check-release-needed decide(): self-heals when package id is unknown to NuGet', () => {
  const result = decide({
    hasChangesets: false,
    currentVersion: '0.1.0',
    publishedVersions: null,
    githubReleaseExists: false,
  });
  assert.equal(result.shouldRelease, true);
  assert.equal(result.skipBump, true);
  assert.equal(result.nugetPublished, false);
  assert.match(result.reason, /not yet registered on NuGet/);
});

test('check-release-needed decide(): self-heals GitHub release when NuGet already has the version', () => {
  const result = decide({
    hasChangesets: false,
    currentVersion: '2.4.0',
    publishedVersions: ['2.2.2', '2.4.0'],
    githubReleaseExists: false,
  });
  assert.equal(result.shouldRelease, true);
  assert.equal(result.skipBump, true);
  assert.equal(result.nugetPublished, true);
  assert.match(result.reason, /no GitHub release/);
});

test('check-release-needed decide(): no-op when both NuGet and GitHub release exist', () => {
  const result = decide({
    hasChangesets: false,
    currentVersion: '2.4.0',
    publishedVersions: ['2.2.2', '2.4.0'],
    githubReleaseExists: true,
  });
  assert.equal(result.shouldRelease, false);
  assert.equal(result.skipBump, false);
  assert.equal(result.nugetPublished, true);
  assert.match(result.reason, /no release needed/);
});

test('check-release-needed readCsprojInfo() extracts version and package id', () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-csproj-info-'));
  const csprojPath = join(dir, 'sample.csproj');
  writeFileSync(
    csprojPath,
    '<Project Sdk="Microsoft.NET.Sdk">\n  <PropertyGroup>\n    <Version>1.2.3</Version>\n    <PackageId>clink</PackageId>\n  </PropertyGroup>\n</Project>\n'
  );

  const info = readCsprojInfo(csprojPath);
  assert.equal(info.version, '1.2.3');
  assert.equal(info.packageId, 'clink');
});

function startNugetAndGithubMock({ versions, githubReleaseStatus }) {
  const sockets = new Set();
  const server = createServer((req, res) => {
    res.setHeader('connection', 'close');
    if (req.url?.startsWith('/nuget/')) {
      if (versions === null) {
        res.writeHead(404).end();
      } else {
        res.writeHead(200, { 'content-type': 'application/json' });
        res.end(JSON.stringify({ versions }));
      }
      return;
    }
    if (req.url?.startsWith('/github/')) {
      res.writeHead(githubReleaseStatus).end();
      return;
    }
    res.writeHead(500).end();
  });
  server.on('connection', (socket) => {
    sockets.add(socket);
    socket.on('close', () => sockets.delete(socket));
  });

  return new Promise((resolve) => {
    server.listen(0, '127.0.0.1', () => {
      const { port } = server.address();
      resolve({
        nugetUrl: `http://127.0.0.1:${port}/nuget`,
        githubUrl: `http://127.0.0.1:${port}/github`,
        close: () => new Promise((r) => {
          for (const socket of sockets) {
            socket.destroy();
          }
          server.close(() => r());
        }),
      });
    });
  });
}

test('check-release-needed CLI writes self-healing outputs when NuGet version is missing', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-check-release-'));
  const csprojPath = join(dir, 'project.csproj');
  const outputFile = join(dir, 'github-output.txt');
  writeFileSync(
    csprojPath,
    '<Project Sdk="Microsoft.NET.Sdk">\n  <PropertyGroup>\n    <Version>2.4.0</Version>\n    <PackageId>clink</PackageId>\n  </PropertyGroup>\n</Project>\n'
  );

  const mock = await startNugetAndGithubMock({
    versions: ['2.2.0', '2.2.1', '2.2.2'],
    githubReleaseStatus: 404,
  });

  try {
    await runNodeAsync(
      'csharp/scripts/check-release-needed.mjs',
      ['--csproj', csprojPath, '--repository', 'link-foundation/link-cli'],
      {
        env: {
          GITHUB_OUTPUT: outputFile,
          HAS_CHANGESETS: 'false',
          NUGET_INDEX_URL: mock.nugetUrl,
          GITHUB_API_URL: mock.githubUrl,
        },
      }
    );
  } finally {
    await mock.close();
  }

  const outputs = readFileSync(outputFile, 'utf8');
  assert.match(outputs, /^should_release=true$/m);
  assert.match(outputs, /^skip_bump=true$/m);
  assert.match(outputs, /^current_version=2\.4\.0$/m);
  assert.match(outputs, /^nuget_published=false$/m);
  assert.match(outputs, /^github_release_exists=false$/m);
});

test('check-release-needed CLI short-circuits when NuGet and GitHub already have the release', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'link-cli-check-release-noop-'));
  const csprojPath = join(dir, 'project.csproj');
  const outputFile = join(dir, 'github-output.txt');
  writeFileSync(
    csprojPath,
    '<Project Sdk="Microsoft.NET.Sdk">\n  <PropertyGroup>\n    <Version>2.4.0</Version>\n    <PackageId>clink</PackageId>\n  </PropertyGroup>\n</Project>\n'
  );

  const mock = await startNugetAndGithubMock({
    versions: ['2.3.0', '2.4.0'],
    githubReleaseStatus: 200,
  });

  try {
    await runNodeAsync(
      'csharp/scripts/check-release-needed.mjs',
      ['--csproj', csprojPath, '--repository', 'link-foundation/link-cli'],
      {
        env: {
          GITHUB_OUTPUT: outputFile,
          HAS_CHANGESETS: 'false',
          NUGET_INDEX_URL: mock.nugetUrl,
          GITHUB_API_URL: mock.githubUrl,
        },
      }
    );
  } finally {
    await mock.close();
  }

  const outputs = readFileSync(outputFile, 'utf8');
  assert.match(outputs, /^should_release=false$/m);
  assert.match(outputs, /^skip_bump=false$/m);
  assert.match(outputs, /^nuget_published=true$/m);
  assert.match(outputs, /^github_release_exists=true$/m);
});

test('wait-for-nuget defaults to two-minute checks across the NuGet indexing window', () => {
  const config = parseWaitForNugetArgs(
    ['--package-id', 'clink', '--release-version', '2.4.0'],
    {}
  );

  assert.equal(config.packageId, 'clink');
  assert.equal(config.releaseVersion, '2.4.0');
  assert.equal(config.maxAttempts, DEFAULT_MAX_ATTEMPTS);
  assert.equal(config.sleepSeconds, DEFAULT_SLEEP_SECONDS);
  assert.equal(DEFAULT_MAX_ATTEMPTS, 8);
  assert.equal(DEFAULT_SLEEP_SECONDS, 120);
});

test('wait-for-nuget builds the flat-container nuspec URL', () => {
  assert.equal(
    createNugetNuspecUrl({
      flatContainerUrl: 'https://api.nuget.org/v3-flatcontainer/',
      packageId: 'Clink',
      version: '2.4.0',
    }),
    'https://api.nuget.org/v3-flatcontainer/clink/2.4.0/clink.nuspec'
  );
});

test('wait-for-nuget succeeds when indexing takes longer than the old 125 second loop', async () => {
  let attempts = 0;
  const sleeps = [];

  const available = await waitForNugetPackage({
    checkAvailability: async () => {
      attempts++;
      return {
        available: attempts === 8,
        status: attempts === 8 ? 200 : 404,
      };
    },
    maxAttempts: 8,
    packageId: 'clink',
    sleepFn: async (seconds) => {
      sleeps.push(seconds);
    },
    sleepSeconds: 120,
    stdout: () => {},
    version: '2.4.0',
  });

  assert.equal(available, true);
  assert.equal(attempts, 8);
  assert.deepEqual(sleeps, [120, 120, 120, 120, 120, 120, 120]);
});

test('wait-for-nuget fails only after exhausting all attempts', async () => {
  let attempts = 0;
  const sleeps = [];

  const available = await waitForNugetPackage({
    checkAvailability: async () => {
      attempts++;
      return { available: false, status: 404 };
    },
    maxAttempts: 3,
    packageId: 'clink',
    sleepFn: async (seconds) => {
      sleeps.push(seconds);
    },
    sleepSeconds: 120,
    stdout: () => {},
    version: '2.4.0',
  });

  assert.equal(available, false);
  assert.equal(attempts, 3);
  assert.deepEqual(sleeps, [120, 120]);
});
