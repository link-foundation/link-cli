import { describe, expect, test } from 'bun:test';
import {
  mkdirSync,
  mkdtempSync,
  rmSync,
  writeFileSync,
} from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';

import {
  appendNuGetBadgeIfMissing,
  buildNuGetBadge,
  buildReleasePayload,
  buildReleaseTag,
  buildReleaseTitle,
  findPackageId,
  main,
  normalizeReleaseVersion,
  parseArgs,
} from './create-github-release.mjs';

describe('create-github-release helpers', () => {
  test('parseArgs defaults to the C# release format and accepts package id', () => {
    const config = parseArgs([
      '--release-version',
      '1.2.3',
      '--repository',
      'owner/repo',
      '--package-id',
      'MyPackage',
    ]);

    expect(config).toEqual({
      assetsGlob: '',
      language: 'C#',
      packageId: 'MyPackage',
      releaseVersion: '1.2.3',
      repository: 'owner/repo',
      tagPrefix: 'csharp_v',
    });
  });

  test('normalizes release versions from legacy and language-prefixed tags', () => {
    expect(normalizeReleaseVersion('csharp-v1.2.3')).toBe('1.2.3');
    expect(normalizeReleaseVersion('csharp_v1.2.3')).toBe('1.2.3');
    expect(normalizeReleaseVersion('v1.2.3')).toBe('1.2.3');
    expect(normalizeReleaseVersion('1.2.3-beta.1+build.7')).toBe(
      '1.2.3-beta.1+build.7'
    );
  });

  test('builds C# release tags and titles from bare semver', () => {
    expect(buildReleaseTag('csharp_v', '1.2.3')).toBe('csharp_v1.2.3');
    expect(buildReleaseTag('csharp_v', 'csharp-v1.2.3')).toBe(
      'csharp_v1.2.3'
    );
    expect(buildReleaseTitle('C#', 'csharp_v1.2.3')).toBe('[C#] 1.2.3');
  });

  test('appends a NuGet shields.io badge when release notes do not have one', () => {
    const notes = appendNuGetBadgeIfMissing('- Fix release title', 'MyPackage');

    expect(notes).toContain('- Fix release title');
    expect(notes).toContain(
      '[![NuGet](https://img.shields.io/nuget/v/MyPackage.svg)]'
    );
    expect(notes).toContain('https://www.nuget.org/packages/MyPackage');
  });

  test('does not append a second badge when release notes already contain shields.io', () => {
    const notes = appendNuGetBadgeIfMissing(
      `${buildNuGetBadge('MyPackage')}\n\n- Existing badge`,
      'MyPackage'
    );

    expect(notes.match(/img\.shields\.io/g)).toHaveLength(1);
  });

  test('builds a GitHub release payload with C# title and NuGet badge', () => {
    const payload = JSON.parse(
      buildReleasePayload({
        changelog: '## [1.2.3] - 2026-05-09\n\n- Fix release metadata\n',
        language: 'C#',
        packageId: 'MyPackage',
        releaseVersion: '1.2.3',
        tagPrefix: 'csharp_v',
      })
    );

    expect(payload).toEqual({
      tag_name: 'csharp_v1.2.3',
      name: '[C#] 1.2.3',
      body:
        '- Fix release metadata\n\n---\n\n' +
        '[![NuGet](https://img.shields.io/nuget/v/MyPackage.svg)]' +
        '(https://www.nuget.org/packages/MyPackage)',
    });
  });

  test('findPackageId reads PackageId from a project file', () => {
    const projectRoot = mkdtempSync(path.join(tmpdir(), 'csharp-release-'));
    try {
      mkdirSync(path.join(projectRoot, 'src', 'Example'), {
        recursive: true,
      });
      writeFileSync(
        path.join(projectRoot, 'src', 'Example', 'Example.csproj'),
        '<Project><PropertyGroup><PackageId>Example.Package</PackageId></PropertyGroup></Project>'
      );

      expect(findPackageId(projectRoot)).toBe('Example.Package');
    } finally {
      rmSync(projectRoot, { force: true, recursive: true });
    }
  });

  test('uploads matching NuGet package assets after creating a release', () => {
    const projectRoot = mkdtempSync(path.join(tmpdir(), 'csharp-release-'));
    try {
      mkdirSync(path.join(projectRoot, 'artifacts'), { recursive: true });
      writeFileSync(
        path.join(projectRoot, 'CHANGELOG.md'),
        '## [1.2.3] - 2026-05-12\n\n- Attach package assets\n'
      );
      writeFileSync(
        path.join(projectRoot, 'artifacts', 'MyPackage.1.2.3.nupkg'),
        'fake nupkg'
      );
      writeFileSync(
        path.join(projectRoot, 'artifacts', 'MyPackage.1.2.3.snupkg'),
        'symbols package'
      );

      const calls = [];
      const stdoutMessages = [];
      const stderrMessages = [];
      const spawn = (command, args, options) => {
        calls.push({ args, command, options });
        return { status: 0, stderr: '', stdout: '' };
      };

      const exitCode = main({
        argv: [
          '--release-version',
          '1.2.3',
          '--repository',
          'owner/repo',
          '--assets-glob',
          'artifacts/*.nupkg',
        ],
        cwd: projectRoot,
        env: {},
        spawn,
        stderr: (message) => stderrMessages.push(message),
        stdout: (message) => stdoutMessages.push(message),
      });

      expect(exitCode).toBe(0);
      expect(stderrMessages).toEqual([]);
      expect(calls).toHaveLength(2);
      expect(calls[0].command).toBe('gh');
      expect(calls[0].args).toEqual([
        'api',
        'repos/owner/repo/releases',
        '-X',
        'POST',
        '--input',
        '-',
      ]);
      expect(calls[1].command).toBe('gh');
      expect(calls[1].args).toEqual([
        'release',
        'upload',
        'csharp_v1.2.3',
        path.join(projectRoot, 'artifacts', 'MyPackage.1.2.3.nupkg'),
        '--clobber',
        '--repo',
        'owner/repo',
      ]);
      expect(stdoutMessages).toContain(
        'Uploading 1 release asset(s) to csharp_v1.2.3...'
      );
    } finally {
      rmSync(projectRoot, { force: true, recursive: true });
    }
  });

  test('uploads matching NuGet package assets when the release already exists', () => {
    const projectRoot = mkdtempSync(path.join(tmpdir(), 'csharp-release-'));
    try {
      mkdirSync(path.join(projectRoot, 'artifacts'), { recursive: true });
      writeFileSync(
        path.join(projectRoot, 'CHANGELOG.md'),
        '## [1.2.3] - 2026-05-12\n\n- Attach package assets\n'
      );
      writeFileSync(
        path.join(projectRoot, 'artifacts', 'MyPackage.1.2.3.nupkg'),
        'fake nupkg'
      );

      const calls = [];
      const stdoutMessages = [];
      const spawn = (command, args, options) => {
        calls.push({ args, command, options });
        if (args[0] === 'api') {
          return { status: 1, stderr: 'already_exists', stdout: '' };
        }

        return { status: 0, stderr: '', stdout: '' };
      };

      const exitCode = main({
        argv: [
          '--release-version',
          '1.2.3',
          '--repository',
          'owner/repo',
          '--assets-glob',
          'artifacts/*.nupkg',
        ],
        cwd: projectRoot,
        env: {},
        spawn,
        stdout: (message) => stdoutMessages.push(message),
      });

      expect(exitCode).toBe(0);
      expect(calls).toHaveLength(2);
      expect(calls[1].args[0]).toBe('release');
      expect(calls[1].args[1]).toBe('upload');
      expect(stdoutMessages).toContain(
        'GitHub release already exists: csharp_v1.2.3, reconciling assets'
      );
    } finally {
      rmSync(projectRoot, { force: true, recursive: true });
    }
  });
});
