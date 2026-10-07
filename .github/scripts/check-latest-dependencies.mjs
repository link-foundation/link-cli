#!/usr/bin/env node

/**
 * Fail when any dependency of the repository is behind its latest release.
 *
 * Checked, each against its own registry, in every manifest git tracks
 * (the projects, the examples and the case-study reproductions alike):
 * - Cargo dependencies (crates.io), plus every package in the Cargo.lock
 *   files (`cargo update --dry-run`);
 * - NuGet packages of every .csproj (nuget.org);
 * - npm dependencies, declared and locked (registry.npmjs.org);
 * - GitHub Actions used by the workflows (major tags of the action's repo);
 * - Node.js and .NET versions set up by the workflows (latest LTS) and tools
 *   installed with taiki-e/install-action (crates.io).
 *
 * A declared version is compared only as far as it is written: `v7` matches
 * any 7.x release, `24.x` any 24.x, and `2.0.21` exactly 2.0.21.
 *
 * A dependency that cannot be updated yet is held back by a comment on its
 * manifest line that links the open issue explaining the blocker:
 *
 *   foo = "1.2.0" # held back: https://github.com/owner/repo/issues/12
 *   <PackageReference Include="Foo" Version="1.2.0" /> <!-- https://github.com/owner/repo/issues/12 -->
 *   - uses: owner/action@v3 # https://github.com/owner/repo/issues/12
 *
 * The issue is looked up (with GITHUB_TOKEN when set): while it is open the
 * dependency is reported as held back; once it is closed the dependency is
 * outdated again. JSON has no comments, so npm dependencies cannot be held
 * back this way.
 *
 * Usage:
 *   node .github/scripts/check-latest-dependencies.mjs [--skip-lockfiles]
 *
 * Exit codes:
 *   - 0: everything is on its latest version
 *   - 1: something is behind (listed in the output and the step summary)
 */

import { execFileSync, spawnSync } from 'node:child_process';
import { appendFileSync, readFileSync } from 'node:fs';
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

/** The URL of the GitHub issue a manifest comment links, if any. */
export function blockerOf(comment) {
  return /https:\/\/github\.com\/[\w.-]+\/[\w.-]+\/issues\/\d+/.exec(comment ?? '')?.[0];
}

/** `entry`, plus the issue its comment links as `blocker`. */
function withBlocker(entry, comment) {
  const blocker = blockerOf(comment);
  return blocker ? { ...entry, blocker } : entry;
}

/** Splits a TOML line into its code and its `#` comment, minding quoted strings. */
function splitTomlComment(line) {
  let quote = null;
  for (let i = 0; i < line.length; i++) {
    const character = line[i];
    if (quote) {
      if (character === '\\' && quote === '"') i++;
      else if (character === quote) quote = null;
    } else if (character === '"' || character === "'") {
      quote = character;
    } else if (character === '#') {
      return [line.slice(0, i), line.slice(i + 1)];
    }
  }
  return [line, ''];
}

/** Registry dependencies of a Cargo.toml; path-only dependencies are skipped. */
export function parseCargoManifest(text) {
  const dependencies = [];
  let table = null; // 'list' inside [dependencies], or the name of [dependencies.NAME]
  for (const raw of text.split('\n')) {
    const [code, comment] = splitTomlComment(raw);
    const line = code.trim();
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
        dependencies.push(withBlocker({ name: plain[1], version: plain[2] }, comment));
      } else if (detailed) {
        const version = /\bversion\s*=\s*"([^"]+)"/.exec(detailed[2]);
        if (version) dependencies.push(withBlocker({ name: detailed[1], version: version[1] }, comment));
      }
    } else if (table) {
      const version = /^version\s*=\s*"([^"]+)"$/.exec(line);
      if (version) dependencies.push(withBlocker({ name: table, version: version[1] }, comment));
    }
  }
  return dependencies;
}

/** `<PackageReference Include="…" Version="…" />` entries of a project file. */
export function parseCsproj(text) {
  return [...text.matchAll(/<PackageReference\s+Include="([^"]+)"\s+Version="([^"]+)"/g)].map((match) => {
    const end = match.index + match[0].length;
    const lineEnd = text.indexOf('\n', end);
    const comments = text.slice(end, lineEnd === -1 ? undefined : lineEnd).match(/<!--.*?-->/g);
    return withBlocker({ name: match[1], version: match[2] }, comments?.join(' '));
  });
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
  for (const [, repository, version, comment] of text.matchAll(
    /uses:\s*([\w.-]+\/[\w.-]+)(?:\/[\w./-]+)?@(v\d+)\s*(?:#(.*))?$/gm,
  )) {
    const blocker = actions.get(repository)?.blocker;
    actions.set(repository, withBlocker({ name: repository, version }, blocker ?? comment));
  }
  return [...actions.values()];
}

/** Runtimes and tools a workflow installs: node-version, dotnet-version, `tool: name@version`. */
export function parseWorkflowTools(text) {
  const tools = [];
  for (const [, kind, version, comment] of text.matchAll(
    /^\s*(node|dotnet)-version:\s*['"]?([\w.]+)['"]?\s*(?:#(.*))?$/gm,
  )) {
    tools.push(withBlocker({ kind, name: kind === 'node' ? 'Node.js' : '.NET', version }, comment));
  }
  for (const [, list, comment] of text.matchAll(/^\s*tool:\s*([^#\n]+?)\s*(?:#(.*))?$/gm)) {
    for (const entry of list.split(',')) {
      const pinned = /^\s*([\w-]+)@([\d.]+)\s*$/.exec(entry);
      if (pinned) tools.push(withBlocker({ kind: 'crate', name: pinned[1], version: pinned[2] }, comment));
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

/**
 * What to do with a dependency: `current` when it is on its latest version,
 * `held` when it is behind but an open issue holds it back, `outdated`
 * otherwise. `blockerState` is the state of the linked issue, if any.
 */
export function verdict({ version, latest }, blockerState) {
  if (!isOutdated(version, latest)) return 'current';
  return blockerState === 'open' ? 'held' : 'outdated';
}

async function fetchJson(url, headers = {}) {
  const response = await fetch(url, { headers: { 'User-Agent': userAgent, Accept: 'application/json', ...headers } });
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

/** The state (`open` or `closed`) of a GitHub issue given by its URL. */
export async function issueState(url) {
  const [, owner, repository, number] = /github\.com\/([^/]+)\/([^/]+)\/issues\/(\d+)/.exec(url);
  const token = process.env.GITHUB_TOKEN;
  const headers = token ? { Authorization: `Bearer ${token}` } : {};
  return (await fetchJson(`https://api.github.com/repos/${owner}/${repository}/issues/${number}`, headers)).state;
}

function read(path) {
  return readFileSync(join(repoRoot, path), 'utf8');
}

/** Files git tracks, relative to the repository root. */
function trackedFiles() {
  return execFileSync('git', ['ls-files', '-z'], { cwd: repoRoot, encoding: 'utf8' }).split('\0').filter(Boolean);
}

/** Everything to check, as `{ registry, name, version, source[, blocker] }`. */
function collectDependencies(files) {
  const entries = [];
  const named = (name) => files.filter((file) => file === name || file.endsWith(`/${name}`));
  for (const manifest of named('Cargo.toml')) {
    for (const dependency of parseCargoManifest(read(manifest))) entries.push({ registry: 'crate', source: manifest, ...dependency });
  }
  for (const project of files.filter((file) => file.endsWith('.csproj'))) {
    for (const dependency of parseCsproj(read(project))) entries.push({ registry: 'nuget', source: project, ...dependency });
  }
  for (const manifest of named('package.json')) {
    const packageJson = parsePackageJson(read(manifest));
    for (const dependency of packageJson) entries.push({ registry: 'npm', source: manifest, ...dependency });
    const lockFile = join(dirname(manifest), 'package-lock.json');
    if (!files.includes(lockFile)) continue;
    const locked = parsePackageLock(read(lockFile), packageJson.map(({ name }) => name));
    for (const dependency of locked) entries.push({ registry: 'npm', source: lockFile, ...dependency });
  }
  for (const workflow of files.filter((file) => /^\.github\/workflows\/[^/]+\.ya?ml$/.test(file))) {
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

function checkLockfiles(files) {
  const outdated = [];
  for (const directory of files.filter((file) => /(^|\/)Cargo\.lock$/.test(file)).map(dirname)) {
    // cargo reports the updates on stderr.
    const { stderr } = spawnSync('cargo', ['update', '--dry-run'], { cwd: join(repoRoot, directory), encoding: 'utf8' });
    for (const update of parseCargoUpdates(stderr ?? '')) outdated.push({ source: `${directory}/Cargo.lock`, ...update });
  }
  return outdated;
}

/** A Markdown table of dependencies, the `blocker` column only when asked for. */
function table(entries, withBlockers) {
  const header = withBlockers
    ? ['| File | Dependency | Used | Latest | Held back by |', '|---|---|---|---|---|']
    : ['| File | Dependency | Used | Latest |', '|---|---|---|---|'];
  const rows = entries
    .sort((a, b) => a.source.localeCompare(b.source) || a.name.localeCompare(b.name))
    .map(({ source, name, version, latest, blocker }) => {
      const cells = [relative(repoRoot, join(repoRoot, source)), name, version, latest];
      if (withBlockers) cells.push(blocker);
      return `| ${cells.join(' | ')} |`;
    });
  return [...header, ...rows].join('\n');
}

function report(title, text) {
  console.log(`\n${title}:\n\n${text}`);
  if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## ${title}\n\n${text}\n`);
}

async function main() {
  const files = trackedFiles();
  const entries = collectDependencies(files);
  const cache = new Map();
  const cached = (key, lookup) => {
    if (!cache.has(key)) cache.set(key, lookup());
    return cache.get(key);
  };
  const outdated = [];
  const held = [];
  const failures = [];
  await Promise.all(
    entries.map(async (entry) => {
      try {
        entry.latest = await cached(`${entry.registry}:${entry.name}`, () => registries[entry.registry](entry.name));
        // The issue is looked up only for a dependency that is behind.
        const state =
          entry.blocker && isOutdated(entry.version, entry.latest)
            ? await cached(entry.blocker, () => issueState(entry.blocker))
            : undefined;
        const result = verdict(entry, state);
        if (result === 'held') held.push(entry);
        if (result === 'outdated') outdated.push(entry);
      } catch (error) {
        failures.push(`${entry.source}: ${entry.name}: ${error.message}`);
      }
    }),
  );
  if (!process.argv.includes('--skip-lockfiles')) outdated.push(...checkLockfiles(files));

  console.log(`Checked ${entries.length} declared versions${process.argv.includes('--skip-lockfiles') ? '' : ' and the Cargo.lock files'}.`);
  if (held.length > 0) report(`${held.length} dependencies are held back by open issues`, table(held, true));
  const closed = outdated.filter(({ blocker }) => blocker);
  if (closed.length > 0) report(`${closed.length} dependencies were held back by issues that are closed now, so they must be updated`, table(closed, true));
  if (outdated.length > 0) report(`${outdated.length} dependencies are behind their latest release`, table(outdated, false));
  for (const failure of failures) console.error(`Could not check ${failure}`);
  process.exitCode = outdated.length > 0 || failures.length > 0 ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  await main();
}
