#!/usr/bin/env node

/**
 * Fail when any dependency of the repository is behind its latest release.
 *
 * Checked, each against its own registry:
 * - Cargo dependencies of rust/ and rust/wasm/ (crates.io), plus every
 *   package in their Cargo.lock files (`cargo update --dry-run`);
 * - NuGet packages of every csharp/ project (nuget.org);
 * - npm dependencies of js/, declared and locked (registry.npmjs.org);
 * - GitHub Actions used by the workflows (major tags of the action's repo);
 * - Node.js and .NET versions set up by the workflows (latest LTS) and tools
 *   installed with taiki-e/install-action (crates.io).
 *
 * A declared version is compared only as far as it is written: `v7` matches
 * any 7.x release, `24.x` any 24.x, and `2.0.21` exactly 2.0.21.
 *
 * Usage:
 *   node .github/scripts/check-latest-dependencies.mjs [--skip-lockfiles]
 *
 * Exit codes:
 *   - 0: everything is on its latest version
 *   - 1: something is behind (listed in the output and the step summary)
 */

import { execFileSync, spawnSync } from 'node:child_process';
import { appendFileSync, readFileSync, readdirSync } from 'node:fs';
import { dirname, join, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repoRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const userAgent = 'link-cli dependency check (https://github.com/link-foundation/link-cli)';

/** Splits a version into its numeric parts; `x` and anything after `-` or `+` end it. */
export function versionParts(version) {
  const parts = [];
  for (const part of version.replace(/^v/, '').split(/[-+]/)[0].split('.')) {
    if (!/^\d+$/.test(part)) break;
    parts.push(Number(part));
  }
  return parts;
}

/** Compares two versions part by part, missing parts counting as 0. */
export function compareVersions(a, b) {
  const left = versionParts(a);
  const right = versionParts(b);
  for (let i = 0; i < Math.max(left.length, right.length); i++) {
    const difference = (left[i] ?? 0) - (right[i] ?? 0);
    if (difference !== 0) return Math.sign(difference);
  }
  return 0;
}

/** True when `latest`, cut to as many parts as `declared` has, is newer than it. */
export function isOutdated(declared, latest) {
  const precision = versionParts(declared).length;
  const cut = versionParts(latest).slice(0, precision).join('.');
  return compareVersions(cut, declared) > 0;
}

/** The newest version without a pre-release suffix. */
export function latestStable(versions) {
  return versions
    .filter((version) => /^v?\d+(\.\d+)*$/.test(version))
    .reduce((best, version) => (best === undefined || compareVersions(version, best) > 0 ? version : best), undefined);
}

/** Registry dependencies of a Cargo.toml; path-only dependencies are skipped. */
export function parseCargoManifest(text) {
  const dependencies = [];
  let table = null; // 'list' inside [dependencies], or the name of [dependencies.NAME]
  for (const raw of text.split('\n')) {
    const line = raw.replace(/#.*$/, '').trim();
    if (line === '') continue;
    const header = /^\[(.+)\]$/.exec(line);
    if (header) {
      const path = header[1].trim();
      const inline = /(?:^|\.)(?:dev-|build-)?dependencies$/.exec(path);
      const named = /(?:^|\.)(?:dev-|build-)?dependencies\.([A-Za-z0-9_-]+)$/.exec(path);
      table = inline ? 'list' : named ? named[1] : null;
      continue;
    }
    if (table === 'list') {
      const plain = /^([A-Za-z0-9_-]+)\s*=\s*"([^"]+)"$/.exec(line);
      const detailed = /^([A-Za-z0-9_-]+)\s*=\s*\{(.*)\}$/.exec(line);
      if (plain) {
        dependencies.push({ name: plain[1], version: plain[2] });
      } else if (detailed) {
        const version = /\bversion\s*=\s*"([^"]+)"/.exec(detailed[2]);
        if (version) dependencies.push({ name: detailed[1], version: version[1] });
      }
    } else if (table) {
      const version = /^version\s*=\s*"([^"]+)"$/.exec(line);
      if (version) dependencies.push({ name: table, version: version[1] });
    }
  }
  return dependencies;
}

/** `<PackageReference Include="…" Version="…" />` entries of a project file. */
export function parseCsproj(text) {
  return [...text.matchAll(/<PackageReference\s+Include="([^"]+)"\s+Version="([^"]+)"/g)].map(
    ([, name, version]) => ({ name, version }),
  );
}

/** Dependencies and devDependencies of a package.json, without range operators. */
export function parsePackageJson(text) {
  const manifest = JSON.parse(text);
  return Object.entries({ ...manifest.dependencies, ...manifest.devDependencies }).map(([name, range]) => ({
    name,
    version: range.replace(/^[\^~=]/, ''),
  }));
}

/** Locked versions of a package-lock.json's direct dependencies. */
export function parsePackageLock(text, names) {
  const packages = JSON.parse(text).packages ?? {};
  return names
    .filter((name) => packages[`node_modules/${name}`])
    .map((name) => ({ name, version: packages[`node_modules/${name}`].version }));
}

/** Actions pinned to a major tag (`owner/repo[/path]@vN`), keyed by repository. */
export function parseWorkflowActions(text) {
  const actions = new Map();
  for (const [, repository, version] of text.matchAll(/uses:\s*([\w.-]+\/[\w.-]+)(?:\/[\w./-]+)?@(v\d+)\s*$/gm)) {
    actions.set(repository, version);
  }
  return [...actions].map(([name, version]) => ({ name, version }));
}

/** Runtimes and tools a workflow installs: node-version, dotnet-version, `tool: name@version`. */
export function parseWorkflowTools(text) {
  const tools = [];
  for (const [, kind, version] of text.matchAll(/^\s*(node|dotnet)-version:\s*['"]?([\w.]+)['"]?\s*$/gm)) {
    tools.push({ kind, name: kind === 'node' ? 'Node.js' : '.NET', version });
  }
  for (const [, list] of text.matchAll(/^\s*tool:\s*(.+)$/gm)) {
    for (const entry of list.split(',')) {
      const pinned = /^\s*([\w-]+)@([\d.]+)\s*$/.exec(entry);
      if (pinned) tools.push({ kind: 'crate', name: pinned[1], version: pinned[2] });
    }
  }
  return tools;
}

/** `Updating name vA -> vB` lines of `cargo update --dry-run`. */
export function parseCargoUpdates(output) {
  return [...output.matchAll(/^\s*Updating (\S+) v(\S+) -> v(\S+)/gm)].map(([, name, from, to]) => ({
    name,
    version: from,
    latest: to,
  }));
}

async function fetchJson(url) {
  const response = await fetch(url, { headers: { 'User-Agent': userAgent, Accept: 'application/json' } });
  if (!response.ok) throw new Error(`${url} answered ${response.status}`);
  return response.json();
}

const registries = {
  crate: async (name) => (await fetchJson(`https://crates.io/api/v1/crates/${name}`)).crate.max_stable_version,
  nuget: async (name) =>
    latestStable((await fetchJson(`https://api.nuget.org/v3-flatcontainer/${name.toLowerCase()}/index.json`)).versions),
  npm: async (name) => (await fetchJson(`https://registry.npmjs.org/${name.replaceAll('/', '%2F')}/latest`)).version,
  action: async (name) => {
    const tags = execFileSync('git', ['ls-remote', '--tags', `https://github.com/${name}`], { encoding: 'utf8' })
      .split('\n')
      .map((line) => line.split('refs/tags/')[1])
      .filter(Boolean);
    return `v${versionParts(latestStable(tags))[0]}`;
  },
  node: async () => (await fetchJson('https://nodejs.org/dist/index.json')).find((release) => release.lts).version,
  dotnet: async () =>
    latestStable(
      (await fetchJson('https://dotnetcli.blob.core.windows.net/dotnet/release-metadata/releases-index.json'))[
        'releases-index'
      ]
        .filter((release) => release['release-type'] === 'lts' && release['support-phase'] === 'active')
        .map((release) => release['channel-version']),
    ),
};

function read(path) {
  return readFileSync(join(repoRoot, path), 'utf8');
}

function findFiles(directory, suffix) {
  return readdirSync(join(repoRoot, directory), { recursive: true })
    .filter((name) => name.endsWith(suffix) && !/(^|[\\/])(bin|obj|node_modules|target)[\\/]/.test(name))
    .map((name) => join(directory, name));
}

/** Everything to check, as `{ registry, name, version, source }`. */
function collectDependencies() {
  const entries = [];
  for (const manifest of ['rust/Cargo.toml', 'rust/wasm/Cargo.toml']) {
    for (const dependency of parseCargoManifest(read(manifest))) entries.push({ registry: 'crate', source: manifest, ...dependency });
  }
  for (const project of findFiles('csharp', '.csproj')) {
    for (const dependency of parseCsproj(read(project))) entries.push({ registry: 'nuget', source: project, ...dependency });
  }
  const packageJson = parsePackageJson(read('js/package.json'));
  for (const dependency of packageJson) entries.push({ registry: 'npm', source: 'js/package.json', ...dependency });
  const locked = parsePackageLock(read('js/package-lock.json'), packageJson.map(({ name }) => name));
  for (const dependency of locked) entries.push({ registry: 'npm', source: 'js/package-lock.json', ...dependency });
  for (const workflow of findFiles('.github/workflows', '.yml')) {
    const text = read(workflow);
    for (const action of parseWorkflowActions(text)) entries.push({ registry: 'action', source: workflow, ...action });
    for (const { kind, ...tool } of parseWorkflowTools(text)) entries.push({ registry: kind, source: workflow, ...tool });
  }
  const seen = new Set();
  return entries.filter(({ registry, source, name, version }) => {
    const key = [registry, source, name, version].join(' ');
    return !seen.has(key) && seen.add(key);
  });
}

function checkLockfiles() {
  const outdated = [];
  for (const directory of ['rust', 'rust/wasm']) {
    // cargo reports the updates on stderr.
    const { stderr } = spawnSync('cargo', ['update', '--dry-run'], { cwd: join(repoRoot, directory), encoding: 'utf8' });
    for (const update of parseCargoUpdates(stderr ?? '')) outdated.push({ source: `${directory}/Cargo.lock`, ...update });
  }
  return outdated;
}

async function main() {
  const entries = collectDependencies();
  const cache = new Map();
  const latest = (registry, name) => {
    const key = `${registry}:${name}`;
    if (!cache.has(key)) cache.set(key, registries[registry](name));
    return cache.get(key);
  };
  const outdated = [];
  const failures = [];
  await Promise.all(
    entries.map(async (entry) => {
      try {
        entry.latest = await latest(entry.registry, entry.name);
        if (isOutdated(entry.version, entry.latest)) outdated.push(entry);
      } catch (error) {
        failures.push(`${entry.source}: ${entry.name}: ${error.message}`);
      }
    }),
  );
  if (!process.argv.includes('--skip-lockfiles')) outdated.push(...checkLockfiles());

  console.log(`Checked ${entries.length} declared versions${process.argv.includes('--skip-lockfiles') ? '' : ' and the Cargo.lock files'}.`);
  const lines = outdated
    .sort((a, b) => a.source.localeCompare(b.source) || a.name.localeCompare(b.name))
    .map(({ source, name, version, latest: newest }) => `| ${relative(repoRoot, join(repoRoot, source))} | ${name} | ${version} | ${newest} |`);
  if (lines.length > 0) {
    const table = ['| File | Dependency | Used | Latest |', '|---|---|---|---|', ...lines].join('\n');
    console.log(`\n${lines.length} dependencies are behind their latest release:\n\n${table}`);
    if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## Outdated dependencies\n\n${table}\n`);
  }
  for (const failure of failures) console.error(`Could not check ${failure}`);
  process.exitCode = lines.length > 0 || failures.length > 0 ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
