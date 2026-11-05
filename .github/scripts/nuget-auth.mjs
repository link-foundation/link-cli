const request = fetch;
/** Credential selection is separate from proof of package authorization. */
export const NUGET_AUTH_HINT =
  'Renew NUGET_API_KEY with push permission and the correct package owner/glob, or configure NuGet trusted publishing and set the NUGET_USER repository variable.';

export function selectNugetCredential({
  oidcOutcome = 'skipped',
  oidcKey = '',
  apiKey = '',
} = {}) {
  if (oidcOutcome === 'success' && oidcKey)
    return {
      method: 'trusted-publishing',
      key: oidcKey,
      reason: 'Official NuGet/login exchange succeeded.',
    };
  const reason =
    oidcOutcome === 'failure'
      ? 'Official NuGet/login exchange failed; inspect its sanitized diagnostics.'
      : 'Trusted publishing unavailable: configure NUGET_USER and an exact publisher policy.';
  if (apiKey) return { method: 'token-fallback', key: apiKey, reason };
  return {
    method: 'none',
    key: '',
    reason: `${reason} No NUGET_API_KEY fallback is configured. ${NUGET_AUTH_HINT}`,
  };
}

export async function checkNugetAuthentication({
  oidcOutcome = 'skipped',
  oidcKey = '',
  apiKey = '',
  packageId = '',
  nugetUser: _nugetUser = '',
  publishedVersion: _publishedVersion = '',
  fetchImpl = request,
} = {}) {
  const selected = selectNugetCredential({ oidcOutcome, oidcKey, apiKey });
  const result = {
    auth: selected.method,
    authenticated: 'unknown',
    authorized: 'unknown',
  };
  if (selected.method === 'none')
    return { ...result, status: 'failed', detail: selected.reason };
  if (selected.method === 'trusted-publishing') {
    return {
      ...result,
      status: 'ok',
      authenticated: true,
      detail:
        'Official OIDC exchange authenticated the workflow. Exact package push authorization is unverified until publication.',
    };
  }
  if (!packageId)
    return {
      ...result,
      status: 'unknown',
      detail: `${selected.reason} Package identity unresolved; cannot probe fallback.`,
    };
  try {
    const response = await fetchImpl(
      `https://www.nuget.org/api/v2/package/create-verification-key/${encodeURIComponent(packageId)}`,
      {
        method: 'POST',
        redirect: 'error',
        headers: { 'X-NuGet-ApiKey': apiKey, 'Content-Length': '0' },
        body: '',
        signal: AbortSignal.timeout(20000),
      },
    );
    if (!response.ok) {
      await response.body?.cancel?.();
      return {
        ...result,
        status: response.status === 401 ? 'failed' : 'unknown',
        detail: `${selected.reason} Fallback verification HTTP ${response.status}. This probe may require PackageVerify scope; unsupported verification is not evidence of push rejection. ${NUGET_AUTH_HINT}`,
      };
    }
    const payload = await response.json();
    if (typeof payload.Key !== 'string' || !payload.Key)
      return {
        ...result,
        status: 'unknown',
        detail:
          'NuGet returned no verification key; authentication remains unverified.',
      };
    return {
      ...result,
      status: 'ok',
      authenticated: true,
      detail: `${selected.reason} Fallback key validity checked. A derived verification key does not prove the original key package glob; publication must enforce push authorization.`,
    };
  } catch {
    return {
      ...result,
      status: 'unknown',
      detail:
        'Fallback verification unavailable or malformed; authentication and push authorization remain unverified.',
    };
  }
}
