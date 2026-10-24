#!/usr/bin/env node

/**
 * Wait for a NuGet package version to become available from the flat-container API.
 *
 * NuGet package validation and indexing usually finish within 15 minutes. The
 * release workflow uses this script after `dotnet nuget push` so GitHub releases
 * are created only after the package can be restored by users.
 *
 * Background: see issue #13. The previous inline verification used a
 * 0/5/10/20/30/60 second retry schedule that gave up after ~125 seconds, well
 * inside the normal NuGet indexing window documented at
 * https://learn.microsoft.com/en-us/nuget/nuget-org/publish-a-package#package-validation-and-indexing.
 *
 * Usage:
 *   node scripts/wait-for-nuget.mjs --package-id <id> --release-version <version>
 *
 * Optional arguments:
 *   --max-attempts <count>       Defaults to 8.
 *   --sleep-seconds <count>      Defaults to 120.
 *   --flat-container-url <url>   Defaults to https://api.nuget.org/v3-flatcontainer.
 *
 * Environment variable overrides (useful for tests):
 *   - NUGET_FLAT_CONTAINER_URL or NUGET_INDEX_URL
 *   - NUGET_WAIT_MAX_ATTEMPTS or MAX_ATTEMPTS
 *   - NUGET_WAIT_SLEEP_SECONDS or SLEEP_SECONDS
 *   - PACKAGE_ID, RELEASE_VERSION (or VERSION)
 *
 * Outputs (written to GITHUB_OUTPUT):
 *   - nuget_available: 'true' when the package version is visible.
 */

import { appendFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const DEFAULT_FLAT_CONTAINER_URL = 'https://api.nuget.org/v3-flatcontainer';
export const DEFAULT_MAX_ATTEMPTS = 8;
export const DEFAULT_SLEEP_SECONDS = 120;

function readCliOptions(argv) {
  const options = {};

  for (let index = 0; index < argv.length; index++) {
    const arg = argv[index];
    if (!arg.startsWith('--')) {
      continue;
    }

    const inlineValueIndex = arg.indexOf('=');
    if (inlineValueIndex !== -1) {
      options[arg.slice(2, inlineValueIndex)] = arg.slice(inlineValueIndex + 1);
      continue;
    }

    const value = argv[index + 1];
    if (value === undefined || value.startsWith('--')) {
      throw new Error(`Missing value for ${arg}`);
    }

    options[arg.slice(2)] = value;
    index++;
  }

  return options;
}

function parsePositiveInteger(value, optionName) {
  const parsed = Number(value);
  if (!Number.isInteger(parsed) || parsed < 1) {
    throw new Error(`${optionName} must be a positive integer`);
  }
  return parsed;
}

export function parseArgs(argv, env = process.env) {
  const cliOptions = readCliOptions(argv);

  return {
    flatContainerUrl:
      cliOptions['flat-container-url'] ||
      env.NUGET_FLAT_CONTAINER_URL ||
      env.NUGET_INDEX_URL ||
      DEFAULT_FLAT_CONTAINER_URL,
    maxAttempts: parsePositiveInteger(
      cliOptions['max-attempts'] ||
        env.NUGET_WAIT_MAX_ATTEMPTS ||
        env.MAX_ATTEMPTS ||
        String(DEFAULT_MAX_ATTEMPTS),
      '--max-attempts'
    ),
    packageId: cliOptions['package-id'] || env.PACKAGE_ID || '',
    releaseVersion:
      cliOptions['release-version'] ||
      cliOptions.version ||
      env.RELEASE_VERSION ||
      env.VERSION ||
      '',
    sleepSeconds: parsePositiveInteger(
      cliOptions['sleep-seconds'] ||
        env.NUGET_WAIT_SLEEP_SECONDS ||
        env.SLEEP_SECONDS ||
        String(DEFAULT_SLEEP_SECONDS),
      '--sleep-seconds'
    ),
  };
}

export function createNugetNuspecUrl({
  flatContainerUrl = DEFAULT_FLAT_CONTAINER_URL,
  packageId,
  version,
}) {
  const baseUrl = flatContainerUrl.replace(/\/+$/, '');
  const lowerPackageId = packageId.toLowerCase();
  const lowerVersion = version.toLowerCase();
  return `${baseUrl}/${lowerPackageId}/${lowerVersion}/${lowerPackageId}.nuspec`;
}

export async function checkNugetPackageVersion({
  fetchImpl = fetch,
  flatContainerUrl = DEFAULT_FLAT_CONTAINER_URL,
  packageId,
  version,
}) {
  const url = createNugetNuspecUrl({ flatContainerUrl, packageId, version });

  try {
    const response = await fetchImpl(url, { method: 'HEAD' });
    return {
      available: response.status === 200,
      status: response.status,
      url,
    };
  } catch (error) {
    return {
      available: false,
      error: error.message,
      status: 'network-error',
      url,
    };
  }
}

function sleep(seconds) {
  return new Promise((resolve) => {
    globalThis.setTimeout(resolve, seconds * 1000);
  });
}

function setOutput(name, value) {
  const outputFile = process.env.GITHUB_OUTPUT;
  if (outputFile) {
    appendFileSync(outputFile, `${name}=${value}\n`);
  }
  console.log(`Output: ${name}=${value}`);
}

export async function waitForNugetPackage({
  checkAvailability = checkNugetPackageVersion,
  flatContainerUrl = DEFAULT_FLAT_CONTAINER_URL,
  maxAttempts = DEFAULT_MAX_ATTEMPTS,
  packageId,
  sleepFn = sleep,
  sleepSeconds = DEFAULT_SLEEP_SECONDS,
  stdout = console.log,
  version,
}) {
  for (let attempt = 1; attempt <= maxAttempts; attempt++) {
    const result = await checkAvailability({
      flatContainerUrl,
      packageId,
      version,
    });

    stdout(
      `NuGet availability for ${packageId}@${version}: ${result.status} ` +
        `(attempt ${attempt}/${maxAttempts})`
    );

    if (result.available) {
      return true;
    }

    if (result.error) {
      stdout(`NuGet availability check warning: ${result.error}`);
    }

    if (attempt < maxAttempts) {
      stdout(`Waiting ${sleepSeconds}s before the next NuGet availability check`);
      await sleepFn(sleepSeconds);
    }
  }

  return false;
}

export async function main({
  argv = process.argv.slice(2),
  env = process.env,
  stderr = console.error,
  stdout = console.log,
} = {}) {
  try {
    const config = parseArgs(argv, env);
    if (!config.packageId || !config.releaseVersion) {
      stderr(
        'Error: --package-id and --release-version are required for NuGet availability checks'
      );
      return 1;
    }

    stdout(
      `Waiting for ${config.packageId}@${config.releaseVersion} on NuGet ` +
        `(${config.maxAttempts} attempts, ${config.sleepSeconds}s interval)`
    );

    const available = await waitForNugetPackage({
      flatContainerUrl: config.flatContainerUrl,
      maxAttempts: config.maxAttempts,
      packageId: config.packageId,
      sleepSeconds: config.sleepSeconds,
      stdout,
      version: config.releaseVersion,
    });

    setOutput('nuget_available', available ? 'true' : 'false');

    if (!available) {
      stderr(
        `${config.packageId}@${config.releaseVersion} did not become available on NuGet`
      );
      return 1;
    }

    stdout(`${config.packageId}@${config.releaseVersion} is available on NuGet`);
    return 0;
  } catch (error) {
    stderr(`Error: ${error.message}`);
    return 1;
  }
}

const invokedDirectly = process.argv[1]
  && fileURLToPath(import.meta.url) === path.resolve(process.argv[1]);

if (invokedDirectly) {
  process.exitCode = await main();
}
