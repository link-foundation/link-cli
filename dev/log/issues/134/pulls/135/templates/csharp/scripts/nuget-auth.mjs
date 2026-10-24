/**
 * Select trusted publishing or verify the legacy API key without uploading a
 * package. NuGet's symbol verification endpoints check validity first, then
 * the package's owner and glob. Neither call publishes a package.
 */
export const NUGET_AUTH_HINT = 'Renew NUGET_API_KEY with push permission and the correct package owner/glob, or configure NuGet trusted publishing and set the NUGET_USER repository variable.';

export async function checkNugetAuthentication({
  nugetUser = '', apiKey = '', packageId = '', publishedVersion = '',
  visibilityKnown = true, fetchImpl = fetch,
} = {}) {
  if (nugetUser.trim()) {
    return {
      status: 'ok', auth: 'trusted-publishing',
      detail: 'NUGET_USER selects trusted publishing; NuGet/login will validate the repository/workflow policy and obtain a temporary key immediately before publishing.',
    };
  }
  const auth = apiKey ? 'api-key' : 'none';
  if (!apiKey) {
    return { status: 'failed', auth, detail: `NUGET_API_KEY is not configured. Publishing cannot proceed; no dotnet add package can find an unpublished release. ${NUGET_AUTH_HINT}` };
  }
  if (!packageId) {
    return { status: 'unknown', auth, detail: 'Package id is unresolved; API key validity and scope cannot be verified.' };
  }

  const id = encodeURIComponent(packageId);
  const createUrl = `https://www.nuget.org/api/v2/package/create-verification-key/${id}`;
  try {
    const response = await fetchImpl(createUrl, {
      method: 'POST', redirect: 'error',
      headers: { 'X-NuGet-ApiKey': apiKey, 'Content-Length': '0' }, body: '',
      signal: AbortSignal.timeout(20000),
    });
    if (!response.ok) {
      await response.body?.cancel?.();
      return {
        status: [401, 403].includes(response.status) ? 'failed' : 'unknown', auth,
        detail: `NuGet API key verification returned HTTP ${response.status}. ${NUGET_AUTH_HINT}`,
      };
    }
    const { Key: verificationKey } = await response.json();
    if (typeof verificationKey !== 'string' || !verificationKey) {
      return { status: 'unknown', auth, detail: 'NuGet did not return a verification key.' };
    }
    if (!visibilityKnown) {
      return { status: 'unknown', auth, detail: 'API key validity verified, but the package index was unavailable; package scope could not be verified.' };
    }
    if (!publishedVersion) {
      return {
        status: 'ok', auth,
        detail: 'API key validity verified. Before the first publish no existing package version is available to verify owner/glob; the push will enforce that scope.',
      };
    }
    const verification = await fetchImpl(
      `https://www.nuget.org/api/v2/verifykey/${id}/${encodeURIComponent(publishedVersion)}`,
      {
        method: 'GET', redirect: 'error',
        headers: { 'X-NuGet-ApiKey': verificationKey },
        signal: AbortSignal.timeout(20000),
      }
    );
    await verification.body?.cancel?.();
    return {
      status: verification.ok ? 'ok' : [401, 403].includes(verification.status) ? 'failed' : 'unknown',
      auth,
      detail: verification.ok
        ? 'API key validity and package owner/glob verified using a one-time verification key.'
        : `NuGet package owner/glob verification returned HTTP ${verification.status}. ${NUGET_AUTH_HINT}`,
    };
  } catch {
    // Request errors can include headers; never log an exception holding a key.
    return { status: 'unknown', auth, detail: 'NuGet verification request failed or timed out; API key validity/scope could not be verified.' };
  }
}
