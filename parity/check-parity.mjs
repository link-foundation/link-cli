#!/usr/bin/env node
// Cross-language parity gate (issue #111).
//
// parity/features.json lists every CLI and library feature with its C#, Rust
// and JavaScript status, the source files that implement it and the tests
// that prove it. Each implemented cell records a fingerprint of its sources.
// The checker fails when:
//   - the manifest is malformed, or a glob matches no tracked file;
//   - a source file under a language root belongs to no feature or shared entry;
//   - a recorded fingerprint is stale (code changed without the manifest);
//   - compared with the base commit, only some implementations of a feature
//     changed and no parity exception acknowledges the one-sided change;
//   - parity/golden-fixtures.json does not match the fixture files, or a
//     consumer test no longer mentions its fixture;
//   - a suite receipt (--receipt) does not list every golden fixture.
//
// Usage:
//   node parity/check-parity.mjs                    check (base: $PARITY_BASE_SHA)
//   node parity/check-parity.mjs --base origin/main check against a local base
//   node parity/check-parity.mjs --update           refresh fingerprints and hashes
//   node parity/check-parity.mjs --acknowledge <feature> --reason <text> [--ref <url>]
//   node parity/check-parity.mjs --receipt <suite> <file>
//   add --verbose (or PARITY_DEBUG=1) to print every matched file and fingerprint.
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const LANGUAGES = ['csharp', 'rust', 'javascript'];
export const STATUSES = ['implemented', 'delegated', 'not-applicable', 'missing'];
export const FEATURES_PATH = 'parity/features.json';
export const GOLDEN_PATH = 'parity/golden-fixtures.json';
const FEATURE_ID = /^[a-z0-9]+(?:-[a-z0-9]+)*$/;

// Converts a repository glob (`**`, `*`, `?`, `{a,b}`) to an anchored RegExp.
export function globToRegExp(glob) {
  let out = '';
  for (let i = 0; i < glob.length; i++) {
    const c = glob[i];
    if (c === '*' && glob[i + 1] === '*') {
      if (glob[i + 2] === '/') {
        out += '(?:.*/)?';
        i += 2;
      } else {
        out += '.*';
        i += 1;
      }
    } else if (c === '*') out += '[^/]*';
    else if (c === '?') out += '[^/]';
    else if (c === '{') {
      const end = glob.indexOf('}', i);
      if (end < 0) throw new Error(`Unclosed brace in glob ${glob}`);
      const options = glob.slice(i + 1, end).split(',');
      out += `(?:${options.map((option) => globToRegExp(option).source.slice(1, -1)).join('|')})`;
      i = end;
    } else out += c.replace(/[.+^$()|[\]\\]/g, '\\$&');
  }
  return new RegExp(`^${out}$`);
}

export function matchFiles(globs, files) {
  const patterns = (globs ?? []).map(globToRegExp);
  return files.filter((file) => patterns.some((pattern) => pattern.test(file))).sort();
}

// sha256 over the sorted files: path, newline, content with CRLF folded to LF, NUL.
export function fingerprint(files, readText) {
  const hash = createHash('sha256');
  for (const file of [...files].sort()) {
    hash.update(`${file}\n`);
    hash.update(readText(file).replace(/\r\n/g, '\n'));
    hash.update('\0');
  }
  return hash.digest('hex').slice(0, 16);
}

export function sha256(buffer) {
  return createHash('sha256').update(buffer).digest('hex');
}

const fail = (errors, message) => errors.push(message);

export function validateManifest(manifest) {
  const errors = [];
  if (manifest?.schema !== 1) fail(errors, `${FEATURES_PATH}: "schema" must be 1`);
  for (const language of LANGUAGES) {
    const roots = manifest?.languages?.[language]?.roots;
    if (!Array.isArray(roots) || !roots.length)
      fail(errors, `languages.${language}.roots must list at least one glob`);
  }
  for (const [index, entry] of (manifest?.shared ?? []).entries()) {
    if (!Array.isArray(entry.paths) || !entry.paths.length || !entry.reason)
      fail(errors, `shared[${index}] needs "paths" and a "reason"`);
  }
  const seen = new Set();
  if (!Array.isArray(manifest?.features) || !manifest.features.length)
    fail(errors, 'features must be a non-empty array');
  for (const feature of manifest?.features ?? []) {
    const where = `feature ${feature.id ?? '<missing id>'}`;
    if (!FEATURE_ID.test(feature.id ?? '')) fail(errors, `${where}: id must be kebab-case`);
    if (seen.has(feature.id)) fail(errors, `${where}: duplicate id`);
    seen.add(feature.id);
    if (!feature.title) fail(errors, `${where}: needs a title`);
    for (const language of LANGUAGES) {
      const cell = feature[language];
      const at = `${where}.${language}`;
      if (!cell || !STATUSES.includes(cell.status)) {
        fail(errors, `${at}: status must be one of ${STATUSES.join(', ')}`);
        continue;
      }
      if (cell.status === 'implemented') {
        if (!cell.sources?.length) fail(errors, `${at}: implemented cells need "sources"`);
        if (!cell.tests?.length) fail(errors, `${at}: implemented cells need test evidence`);
        if (!/^[0-9a-f]{16}$/.test(cell.fingerprint ?? ''))
          fail(errors, `${at}: missing fingerprint; run node parity/check-parity.mjs --update`);
      } else if (cell.status === 'delegated') {
        if (language !== 'javascript') fail(errors, `${at}: only JavaScript may delegate`);
        if (!LANGUAGES.includes(cell.to) || cell.to === language)
          fail(errors, `${at}: "to" must name the implementing language`);
        if (!cell.via?.length) fail(errors, `${at}: delegated cells need "via" globs`);
        if (!cell.tests?.length) fail(errors, `${at}: delegated cells need test evidence`);
      } else if (cell.status === 'not-applicable') {
        if (!cell.reason) fail(errors, `${at}: not-applicable cells need a "reason"`);
      } else if (!cell.issue) fail(errors, `${at}: missing cells need a tracking "issue"`);
    }
    for (const [index, exception] of (feature.parityExceptions ?? []).entries()) {
      const at = `${where}.parityExceptions[${index}]`;
      if (!exception.reason || !exception.ref) fail(errors, `${at}: needs "reason" and "ref"`);
      const keys = Object.keys(exception.fingerprints ?? {});
      if (!keys.length || keys.some((key) => !LANGUAGES.includes(key)))
        fail(errors, `${at}: "fingerprints" must map languages to fingerprints`);
    }
  }
  return errors;
}

// Globs of a cell that identify its code: sources, or the delegation glue.
const cellCode = (cell) => (cell.status === 'delegated' ? cell.via : cell.sources) ?? [];

export function computeFingerprints(manifest, files, readText) {
  const result = {};
  for (const feature of manifest.features) {
    result[feature.id] = {};
    for (const language of LANGUAGES) {
      const cell = feature[language];
      if (cell?.status === 'implemented')
        result[feature.id][language] = fingerprint(matchFiles(cell.sources, files), readText);
    }
  }
  return result;
}

export function checkGlobs(manifest, files) {
  const errors = [];
  const check = (globs, where) => {
    for (const glob of globs ?? [])
      if (!matchFiles([glob], files).length)
        fail(errors, `${where}: glob ${glob} matches no tracked file`);
  };
  for (const [index, entry] of (manifest.shared ?? []).entries())
    check(entry.paths, `shared[${index}]`);
  for (const feature of manifest.features)
    for (const language of LANGUAGES) {
      const cell = feature[language] ?? {};
      check(cell.sources, `${feature.id}.${language}.sources`);
      check(cell.via, `${feature.id}.${language}.via`);
      check(cell.tests, `${feature.id}.${language}.tests`);
    }
  return errors;
}

// Every file under a language root must belong to a feature or a shared entry,
// so new code cannot land outside the parity manifest.
export function checkCoverage(manifest, files) {
  const errors = [];
  const owned = new Set(
    matchFiles(
      (manifest.shared ?? []).flatMap((e) => e.paths),
      files,
    ),
  );
  for (const feature of manifest.features)
    for (const language of LANGUAGES)
      for (const file of matchFiles(cellCode(feature[language] ?? {}), files)) owned.add(file);
  for (const language of LANGUAGES)
    for (const file of matchFiles(manifest.languages[language].roots, files))
      if (!owned.has(file))
        fail(errors, `${file}: ${language} source belongs to no feature in ${FEATURES_PATH}`);
  return errors;
}

export function checkFingerprints(manifest, current) {
  const errors = [];
  for (const feature of manifest.features)
    for (const language of LANGUAGES) {
      const cell = feature[language];
      if (cell?.status !== 'implemented' || !/^[0-9a-f]{16}$/.test(cell.fingerprint ?? ''))
        continue;
      const actual = current[feature.id][language];
      if (actual !== cell.fingerprint)
        fail(
          errors,
          `${feature.id}.${language}: fingerprint ${cell.fingerprint} is stale (sources hash to ` +
            `${actual}). Review the other implementations, then run node parity/check-parity.mjs --update`,
        );
    }
  return errors;
}

const implemented = (feature) =>
  LANGUAGES.filter((language) => feature[language]?.status === 'implemented');

// Implemented languages whose entry (status or fingerprint) differs from the base manifest.
export function changedLanguages(feature, baseFeature) {
  return implemented(feature).filter((language) => {
    const head = feature[language];
    const base = baseFeature[language] ?? {};
    return base.status === 'implemented' && head.fingerprint !== base.fingerprint;
  });
}

export function checkOneSidedChanges(manifest, baseManifest) {
  const errors = [];
  if (!baseManifest) return errors;
  const baseFeatures = new Map(baseManifest.features.map((feature) => [feature.id, feature]));
  for (const feature of manifest.features) {
    const baseFeature = baseFeatures.get(feature.id);
    if (!baseFeature) continue;
    const changed = changedLanguages(feature, baseFeature);
    const all = implemented(feature).filter((l) => baseFeature[l]?.status === 'implemented');
    if (!changed.length || changed.length === all.length) continue;
    const acknowledged = (feature.parityExceptions ?? []).some((exception) =>
      changed.every(
        (language) => exception.fingerprints?.[language] === feature[language].fingerprint,
      ),
    );
    if (!acknowledged) {
      const others = all.filter((language) => !changed.includes(language));
      fail(
        errors,
        `${feature.id}: only ${changed.join(', ')} changed; ${others.join(', ')} did not. Port the ` +
          `change, or acknowledge it: node parity/check-parity.mjs --acknowledge ${feature.id} ` +
          `--reason "<why the others need no change>" --ref <issue or PR URL>`,
      );
    }
  }
  return errors;
}

export function checkGoldenFixtures(golden, files, readBuffer, readText) {
  const errors = [];
  if (golden?.schema !== 1 || !Array.isArray(golden.fixtures) || !golden.fixtures.length)
    return [`${GOLDEN_PATH}: needs "schema": 1 and a non-empty "fixtures" array`];
  const suites = golden.suites ?? [];
  if (!suites.includes('csharp') || !suites.includes('rust'))
    fail(errors, `${GOLDEN_PATH}: "suites" must include csharp and rust`);
  const listed = new Set();
  for (const fixture of golden.fixtures) {
    listed.add(fixture.path);
    if (!files.includes(fixture.path)) {
      fail(errors, `${fixture.path}: golden fixture is not tracked`);
      continue;
    }
    const actual = sha256(readBuffer(fixture.path));
    if (actual !== fixture.sha256)
      fail(
        errors,
        `${fixture.path}: sha256 ${actual} differs; run node parity/check-parity.mjs --update`,
      );
    const name = fixture.path.split('/').pop();
    for (const suite of suites)
      if (!fixture.consumers?.[suite]?.length)
        fail(errors, `${fixture.path}: list at least one ${suite} test that reads it`);
    for (const consumer of Object.values(fixture.consumers ?? {}).flat()) {
      if (!files.includes(consumer))
        fail(errors, `${fixture.path}: consumer ${consumer} is missing`);
      else if (!readText(consumer).includes(name))
        fail(errors, `${fixture.path}: consumer ${consumer} no longer mentions ${name}`);
    }
  }
  for (const file of matchFiles(golden.include ?? [], files))
    if (!listed.has(file)) fail(errors, `${file}: fixture is missing from ${GOLDEN_PATH}`);
  return errors;
}

// A receipt is written by the golden fixture test of one suite. It proves that
// the suite loaded and hashed every fixture of the manifest.
export function checkReceipt(golden, suite, receipt) {
  const errors = [];
  if (receipt?.suite !== suite)
    fail(errors, `receipt suite is ${receipt?.suite}, expected ${suite}`);
  for (const fixture of golden.fixtures) {
    const loaded = receipt?.fixtures?.[fixture.path];
    if (loaded !== fixture.sha256)
      fail(errors, `${suite} did not load ${fixture.path} (receipt: ${loaded ?? 'absent'})`);
  }
  return errors;
}

export function updateGolden(golden, readBuffer) {
  for (const fixture of golden.fixtures) fixture.sha256 = sha256(readBuffer(fixture.path));
  return golden;
}

export function updateManifest(manifest, current) {
  for (const feature of manifest.features)
    for (const language of LANGUAGES)
      if (feature[language]?.status === 'implemented')
        feature[language].fingerprint = current[feature.id][language];
  return manifest;
}

export function acknowledge(manifest, baseManifest, id, reason, ref) {
  const feature = manifest.features.find((candidate) => candidate.id === id);
  if (!feature) throw new Error(`Unknown feature ${id}`);
  if (!reason || !ref) throw new Error('--acknowledge needs --reason and --ref');
  const baseFeature = baseManifest?.features.find((candidate) => candidate.id === id);
  if (!baseFeature) throw new Error(`${id} has no base entry; nothing to acknowledge`);
  const changed = changedLanguages(feature, baseFeature);
  if (!changed.length) throw new Error(`${id} has no changed implementation; run --update first`);
  const fingerprints = Object.fromEntries(changed.map((l) => [l, feature[l].fingerprint]));
  feature.parityExceptions = [...(feature.parityExceptions ?? []), { fingerprints, reason, ref }];
  return manifest;
}

function git(root, args) {
  return execFileSync('git', args, {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  });
}

export function readBaseManifest(root, base, log = () => {}) {
  if (!base || /^0+$/.test(base)) {
    log('No base commit; the one-sided change rule is skipped.');
    return undefined;
  }
  try {
    git(root, ['cat-file', '-e', `${base}^{commit}`]);
  } catch {
    // A shallow clone hides the base; skipping would silently pass every
    // one-sided change, so this is an error (check out with fetch-depth: 0).
    throw new Error(`base commit ${base} is not in this clone; fetch it first`);
  }
  try {
    const manifest = JSON.parse(git(root, ['show', `${base}:${FEATURES_PATH}`]));
    log(`One-sided changes are compared with ${base}.`);
    return manifest;
  } catch (error) {
    log(
      `Base ${base} has no readable ${FEATURES_PATH} (${error.message.split('\n')[0]}); skipped.`,
    );
    return undefined;
  }
}

function parseArgs(argv) {
  const options = { verbose: process.env.PARITY_DEBUG === '1' };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === '--update') options.update = true;
    else if (arg === '--verbose') options.verbose = true;
    else if (arg === '--base') options.base = argv[++i];
    else if (arg === '--acknowledge') options.acknowledge = argv[++i];
    else if (arg === '--reason') options.reason = argv[++i];
    else if (arg === '--ref') options.ref = argv[++i];
    else if (arg === '--receipt') options.receipt = [argv[++i], argv[++i]];
    else throw new Error(`Unknown argument ${arg}`);
  }
  return options;
}

export function run(argv, root = resolve(dirname(fileURLToPath(import.meta.url)), '..')) {
  const options = parseArgs(argv);
  const log = (message) => options.verbose && console.log(message);
  const files = git(root, ['ls-files', '-z']).split('\0').filter(Boolean);
  const readBuffer = (file) => readFileSync(resolve(root, file));
  const readText = (file) => readBuffer(file).toString('utf8');
  const manifest = JSON.parse(readText(FEATURES_PATH));
  const golden = JSON.parse(readText(GOLDEN_PATH));
  const write = (path, value) =>
    writeFileSync(resolve(root, path), `${JSON.stringify(value, null, 2)}\n`);

  if (options.receipt) {
    const [suite, file] = options.receipt;
    if (!existsSync(file)) {
      console.error(`::error::${suite} wrote no golden fixture receipt at ${file}`);
      return 1;
    }
    const errors = checkReceipt(golden, suite, JSON.parse(readFileSync(file, 'utf8')));
    for (const error of errors) console.error(`::error::${error}`);
    if (!errors.length)
      console.log(`${suite} loaded all ${golden.fixtures.length} golden fixtures`);
    return errors.length ? 1 : 0;
  }

  const schemaErrors = validateManifest(manifest);
  if (schemaErrors.length && !options.update) {
    for (const error of schemaErrors) console.error(`::error file=${FEATURES_PATH}::${error}`);
    return 1;
  }
  const current = computeFingerprints(manifest, files, readText);
  if (options.verbose)
    for (const feature of manifest.features)
      for (const language of LANGUAGES) {
        const cell = feature[language];
        const code = matchFiles(cellCode(cell), files);
        log(`${feature.id}.${language} [${cell.status}] ${current[feature.id][language] ?? '-'}`);
        for (const file of code) log(`  ${file}`);
      }
  const base = options.base ?? process.env.PARITY_BASE_SHA;
  if (options.update || options.acknowledge) {
    updateManifest(manifest, current);
    updateGolden(golden, readBuffer);
    if (options.acknowledge) {
      const baseManifest = readBaseManifest(root, base ?? 'origin/main', console.log);
      acknowledge(manifest, baseManifest, options.acknowledge, options.reason, options.ref);
    }
    write(FEATURES_PATH, manifest);
    write(GOLDEN_PATH, golden);
    console.log(`Updated ${FEATURES_PATH} and ${GOLDEN_PATH}`);
  }
  const oneSided = () => {
    try {
      return checkOneSidedChanges(manifest, readBaseManifest(root, base, console.log));
    } catch (error) {
      return [error.message];
    }
  };
  const errors = [
    ...validateManifest(manifest),
    ...checkGlobs(manifest, files),
    ...checkCoverage(manifest, files),
    ...checkFingerprints(manifest, current),
    ...oneSided(),
    ...checkGoldenFixtures(golden, files, readBuffer, readText),
  ];
  for (const error of errors) console.error(`::error::${error}`);
  const counts = Object.fromEntries(
    STATUSES.map((status) => [
      status,
      manifest.features.reduce(
        (sum, feature) => sum + LANGUAGES.filter((l) => feature[l]?.status === status).length,
        0,
      ),
    ]),
  );
  console.log(
    `Parity: ${manifest.features.length} features, ${JSON.stringify(counts)}, ` +
      `${golden.fixtures.length} golden fixtures, ${errors.length} findings`,
  );
  return errors.length ? 1 : 0;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  process.exitCode = run(process.argv.slice(2));
}
