// Authentication only: never uploads packages or changes versions.
import { checkCratePublisher } from './crates-publish-preflight.mjs';
import { checkNugetAuthentication, NUGET_AUTH_HINT } from './nuget-auth.mjs';
import { verifyNpmTrust } from './verify-npm-trust.mjs';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';

export async function preflight(
  registry,
  {
    env = process.env,
    crates = checkCratePublisher,
    nuget = checkNugetAuthentication,
    npm = verifyNpmTrust,
  } = {},
) {
  if (!['crates', 'nuget', 'npm'].includes(registry)) throw Error(`Unknown registry: ${registry}`);
  if (env.GITHUB_EVENT_NAME === 'pull_request') {
    return 'PR advisory: publishing credentials are unavailable to untrusted changes. Main must authenticate before builds or version writes.';
  }
  if (['changeset-pr', 'changelog-pr'].includes(env.RELEASE_MODE)) {
    return 'This dispatch only creates release metadata; registry authentication is not required.';
  }
  if (registry === 'crates') {
    const result = await crates({ packageName: 'link-cli', token: env.CARGO_REGISTRY_TOKEN });
    if (result.verdict !== 'verified')
      throw Error(
        `${result.message}. Configure crates.io trusted publishing for rust.yml and CRATES_IO_TRUSTED_PUBLISHING=true, or renew CARGO_TOKEN with link-cli publish scope.`,
      );
    return result.message;
  }
  if (registry === 'nuget') {
    const failures = [];
    for (const packageId of ['clink', 'Foundation.Data.Doublets.Cli']) {
      const result = await nuget({
        packageId,
        oidcOutcome: env.NUGET_OIDC_OUTCOME,
        oidcKey: env.NUGET_OIDC_KEY,
        apiKey: env.NUGET_API_KEY,
      });
      if (result.status !== 'ok') failures.push(`${packageId}: ${result.detail}`);
    }
    if (failures.length) throw Error(`${failures.join('\n')} ${NUGET_AUTH_HINT}`);
    return 'NuGet authentication verified; package push authorization is enforced at publication.';
  }
  await npm(env.NPM_PACKAGE, { env });
  return 'npm trusted publisher exchange verified; no package uploaded.';
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    console.log(await preflight(process.argv[2]));
  } catch (error) {
    console.error(`::error title=Release authentication preflight::${error.message}`);
    process.exitCode = 1;
  }
}
