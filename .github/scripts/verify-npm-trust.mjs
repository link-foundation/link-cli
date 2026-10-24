// Verify the registry's trusted publisher policy without uploading a package.
// API: https://api-docs.npmjs.com/#tag/OIDC
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';

export async function verifyNpmTrust(packageName, { env = process.env, fetch = globalThis.fetch } = {}) {
  if (!env.ACTIONS_ID_TOKEN_REQUEST_URL || !env.ACTIONS_ID_TOKEN_REQUEST_TOKEN) {
    throw new Error('npm verification requires GitHub Actions with id-token: write');
  }
  const url = new URL(env.ACTIONS_ID_TOKEN_REQUEST_URL);
  url.searchParams.set('audience', 'npm:registry.npmjs.org');
  const oidc = await fetch(url, {
    headers: { Authorization: `Bearer ${env.ACTIONS_ID_TOKEN_REQUEST_TOKEN}` },
    signal: AbortSignal.timeout(30_000),
  });
  if (!oidc.ok) throw new Error(`GitHub OIDC request failed: HTTP ${oidc.status}`);
  const { value } = await oidc.json();
  if (!value) throw new Error('GitHub OIDC response is missing a token');
  const exchange = await fetch(`https://registry.npmjs.org/-/npm/v1/oidc/token/exchange/package/${encodeURIComponent(packageName)}`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${value}` },
    signal: AbortSignal.timeout(30_000),
  });
  if (!exchange.ok || !(await exchange.json()).token) {
    throw new Error(`npm trusted publisher verification failed (HTTP ${exchange.status}). Configure this repository and js-client.yml in the package's trusted publisher settings on npmjs.com.`);
  }
  // Discard the short-lived credential. Never write it to logs, disk or outputs.
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    await verifyNpmTrust('@link-foundation/link-cli-client');
    console.log('npm trusted publisher authentication succeeded. No package was uploaded.');
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}
