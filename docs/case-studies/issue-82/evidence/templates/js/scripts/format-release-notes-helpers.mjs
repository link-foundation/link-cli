export function normalizeReleaseVersionForBadge(releaseVersion) {
  const trimmedVersion = releaseVersion.trim();
  const semverTagMatch = trimmedVersion.match(
    /(?:^|-)v?(\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?)$/i
  );

  if (semverTagMatch) {
    return semverTagMatch[1];
  }

  return trimmedVersion
    .replace(/^[A-Za-z][A-Za-z0-9]*-/, '')
    .replace(/^v/i, '');
}

export function encodeShieldsStaticBadgeSegment(value) {
  return encodeURIComponent(value).replace(/-/g, '--').replace(/_/g, '__');
}

export function buildNpmVersionBadge(packageName, releaseVersion) {
  const versionWithoutV = normalizeReleaseVersionForBadge(releaseVersion);
  const badgeVersion = encodeShieldsStaticBadgeSegment(versionWithoutV);
  const packageVersionPath = encodeURIComponent(versionWithoutV);

  return `[![npm version](https://img.shields.io/badge/npm-${badgeVersion}-blue.svg)](https://www.npmjs.com/package/${packageName}/v/${packageVersionPath})`;
}
