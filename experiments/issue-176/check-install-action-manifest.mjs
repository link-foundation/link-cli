#!/usr/bin/env node
/**
 * Issue #176: is cargo-audit 0.23.0 installable as a verified prebuilt binary
 * by a released taiki-e/install-action yet?
 *
 * install-action reads `manifests/<tool>.json` from the commit the action is
 * pinned to, so the version must be in the manifest of a *release* tag, not
 * just on the default branch.
 *
 * Usage: node experiments/issue-176/check-install-action-manifest.mjs [tool] [version]
 * Exit 0 and print the `uses:` pin when ready, exit 1 when still blocked.
 * Set GITHUB_TOKEN (e.g. `GITHUB_TOKEN=$(gh auth token)`) to avoid rate limits.
 */
const [tool = 'cargo-audit', version = '0.23.0'] = process.argv.slice(2);
const headers = {
  'User-Agent': 'link-cli-dependency-check',
  Accept: 'application/vnd.github+json',
};
if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;

async function json(url) {
  const response = await fetch(url, { headers });
  if (!response.ok) throw new Error(`${url}: HTTP ${response.status}`);
  return response.json();
}

const release = await json('https://api.github.com/repos/taiki-e/install-action/releases/latest');
const tag = release.tag_name;
const ref = await json(`https://api.github.com/repos/taiki-e/install-action/commits/${tag}`);
const manifest = await json(
  `https://raw.githubusercontent.com/taiki-e/install-action/${ref.sha}/manifests/${tool}.json`,
);
const versions = Object.keys(manifest).filter((key) => /^\d+\.\d+\.\d+$/.test(key));
const numeric = (v) => v.split('.').map(Number);
const compare = (a, b) => numeric(a).reduce((d, x, i) => d || x - numeric(b)[i], 0);
const newest = versions.sort(compare).at(-1);

console.log(`install-action latest release: ${tag} (${release.published_at}) ${ref.sha}`);
console.log(`${tool} manifest at that release ends at: ${newest}`);
if (manifest[version]) {
  console.log(`READY: ${tool}@${version} is in the manifest.`);
  console.log(`Pin: uses: taiki-e/install-action@${ref.sha} # ${tag}`);
  process.exit(0);
}
console.log(`BLOCKED: ${tool}@${version} is not in the manifest of ${tag}.`);
process.exit(1);
