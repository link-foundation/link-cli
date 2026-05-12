# Issue 88 Case Study: C# Release Badge Link and Missing NuGet Badges in C# Releases

Issue: https://github.com/link-foundation/link-cli/issues/88

Prepared PRs:

- PR #89 (merged): https://github.com/link-foundation/link-cli/pull/89 — fixed the C# release badge URL in `README.md` so the badge in the README points to the C#-only filtered releases page rather than the mixed `/releases` list.
- PR #90 (this PR): https://github.com/link-foundation/link-cli/pull/90 — addresses the follow-up comment _"Still no NuGet badges in C# GitHub releases."_ The C# release body itself now leads with NuGet version and downloads badges that link to the exact published version, matching the Rust release body's Crates.io + Docs.rs pair.

Related release: https://github.com/link-foundation/link-cli/releases/tag/csharp-v2.4.0

## Requirements

Restated from issue #88 and its comments:

1. The C# Release shields.io badge in `README.md` shows the latest C# version (e.g. `csharp-v2.4.0`) but clicking it lands on the generic releases page that mixes C# and Rust releases instead of the specific C# release version. _(Addressed in PR #89.)_
2. Apply the same best practices to comparable badges (the Rust Release badge has the same defect). _(Addressed in PR #89.)_
3. Compare the full GitHub workflow / CI/CD scripts tree against the C#, JS, and Rust AI-driven development pipeline templates and reuse best practices.
4. If the same issue exists in any template repository, report it upstream.
5. Preserve issue/PR/release data and analysis under `docs/case-studies/issue-88/`.
6. Search public sources for facts about how the shields.io GitHub release badge target works so the fix is grounded.
7. Add debug or verbose output if there is not enough data to find the root cause.
8. Plan and execute everything in this single PR.
9. **Follow-up comment**: _"Still no NuGet badges in C# GitHub releases."_ — the C# GitHub release pages (e.g. `csharp-v2.4.0`) do not embed a NuGet badge in the release body. The Rust release pages do embed Crates.io and Docs.rs badges. _(Addressed in PR #90.)_

## Timeline

- `2026-05-12T19:34:20Z`: GitHub Actions created the `csharp-v2.4.0` tag (`github-data/csharp-v2.4.0-release.json`).
- `2026-05-12T21:54:13Z`: Release `csharp-v2.4.0` was published _without_ any NuGet badge in the release body.
- `2026-05-12T21:57Z`: Issue #88 was filed showing that the C# release badge in `README.md` linked to `/releases` rather than to `csharp-v2.4.0`. Evidence: `github-data/issue-88.json`.
- `2026-05-12T21:59Z`: Investigation reproduced the README badge link behavior. Probing showed:
  - The shields.io badge endpoint `https://img.shields.io/github/v/release/link-foundation/link-cli?filter=csharp-v*` returns the latest C# release version. Evidence: `logs/shields-filter-csharp-headers.txt`.
  - GitHub's filtered releases URL `https://github.com/link-foundation/link-cli/releases?q=C%23&expanded=true` returns HTTP 200 and lists only C# releases with the latest expanded at the top. Evidence: `logs/releases-q-csharp-headers.txt`.
  - The same pattern works for Rust with `q=Rust`. Evidence: `logs/releases-q-rust-headers.txt`.
- _PR #89 merged_: `README.md` badge targets now point at the language-filtered releases page.
- `2026-05-12T22:39Z`: konard re-opened the conversation with _"Still no NuGet badges in C# GitHub releases."_ — surfacing that the C# release **body** (the content shown on a release page like `/releases/tag/csharp-v2.4.0`) still has no NuGet badge, while the Rust release body shows Crates.io + Docs.rs badges. Evidence: `github-data/issue-88-comments.json` and `github-data/csharp-v2.4.0-release.json`.
- `2026-05-12T22:42Z`: Inspection of the Rust release script (`rust/scripts/create-github-release.rs`) confirmed it prepends `[![Crates.io]...] [![Docs.rs]...]` to the release notes. The C# release script (`csharp/scripts/create-github-release.mjs`) had no equivalent badge logic. Evidence: `github-data/csharp-v2.4.0-release.json` (no `img.shields.io` in `body`) vs. the latest Rust release body which begins with both badges.
- `2026-05-12T22:43Z`: The csharp template at `link-foundation/csharp-ai-driven-development-pipeline-template` was inspected and **already** defines `buildNuGetBadge` and `appendNuGetBadgeIfMissing` in its `scripts/create-github-release.mjs`. The link-cli pipeline diverged from that template's behavior. Evidence: `templates/csharp-ai-driven-development-pipeline-template/create-github-release.mjs` snapshot (added in this PR).
- _This PR (#90)_: link-cli's `csharp/scripts/create-github-release.mjs` now prepends NuGet version and downloads badges that link to the exact released version. The existing `csharp-v2.4.0` release body was edited via `gh release edit` to backfill the badges. Evidence: `github-data/csharp-v2.4.0-release-after-fix.json`.

## Evidence

- Issue and PR data: `github-data/issue-88.json`, `github-data/issue-88-comments.json`, `github-data/pr-89.json`, `github-data/pr-89-comments.json`, `github-data/pr-89-review-comments.json`, `github-data/pr-89-reviews.json`, `github-data/pr-90.json`, `github-data/pr-90-comments.json`, `github-data/pr-90-review-comments.json`, `github-data/pr-90-reviews.json`.
- Release data: `github-data/csharp-v2.4.0-release.json` (before fix), `github-data/csharp-v2.4.0-release-after-fix.json` (after fix).
- Probe headers: `logs/shields-filter-csharp-headers.txt`, `logs/shields-filter-rust-headers.txt`, `logs/releases-q-csharp-headers.txt`, `logs/releases-q-rust-headers.txt`, `logs/releases-tag-csharp-v2.4.0-headers.txt`.
- Template snapshots: `templates/csharp-template/README.md`, `templates/csharp-template/file-tree.txt`, `templates/js-template/README.md`, `templates/js-template/file-tree.txt`, `templates/rust-template/README.md`, `templates/rust-template/file-tree.txt`, and `templates/csharp-ai-driven-development-pipeline-template/create-github-release.mjs` (the template script with the badge logic this PR ports).
- Investigation timestamp: `github-data/investigation-timestamp.txt`.

## Online Facts

- The shields.io documentation for the GitHub release badge confirms a `filter` query parameter that narrows the badge to tags matching a glob, used here to separate C# and Rust release lines on the same repository. Source: https://shields.io/badges/git-hub-release
- The shields.io documentation for NuGet defines `https://img.shields.io/nuget/v/<package-id>` (version) and `https://img.shields.io/nuget/dt/<package-id>` (total downloads) endpoints. Source: https://shields.io/badges/nu-get
- A NuGet package's canonical landing page for a specific version is `https://www.nuget.org/packages/<id>/<version>` (the unversioned `https://www.nuget.org/packages/<id>` URL is the package home and always redirects to the latest version). Linking each release's badge to the version-specific URL keeps the click target in sync with the version the badge image renders for that release.
- GitHub serves a `releases` page query parameter `q` that filters by release title text and an `expanded=true` parameter that opens the matched release inline at the top. The link-cli release titles are formed `C# v<version>` and `[Rust] <version>`, so `q=C%23` and `q=Rust` exactly partition the list (confirmed by inspecting the rendered HTML).
- In Markdown the badge target is the URL inside the outer parentheses: `[![alt](badge-image)](target-url)`. Markdown cannot evaluate the badge image to derive the target, so the target URL must be set explicitly.

## Root Cause

There were two independent defects under the same issue:

1. **README badge target (fixed by PR #89).** The badge image was configured with `filter=csharp-v*` so that it displays only C# tags, but its Markdown link target was the generic `/releases` URL, which lists every release in the repository (C# and Rust mixed). A user reading the badge "C# release v2.4.0" expects the click target to land on the C# v2.4.0 release card; instead they landed on a mixed page.

2. **Missing NuGet badges in C# release bodies (fixed by PR #90).** `csharp/scripts/create-github-release.mjs` built its release body from the changelog plus an optional `Package: <id>` footer and posted it via `gh api repos/.../releases`. It had **no** badge generation logic. The Rust counterpart `rust/scripts/create-github-release.rs` does prepend `[![Crates.io](...)](.../<version>) [![Docs.rs](...)](.../<version>)`, and the upstream `link-foundation/csharp-ai-driven-development-pipeline-template` template's `scripts/create-github-release.mjs` already defines `buildNuGetBadge` and `appendNuGetBadgeIfMissing`. The link-cli C# pipeline simply had not been kept in sync with the template, so C# releases had no NuGet badge.

## Solution

### PR #89 — `README.md` badge target

Before:

```markdown
[![C# Release](https://img.shields.io/github/v/release/link-foundation/link-cli?filter=csharp-v*&label=C%23%20release)](https://github.com/link-foundation/link-cli/releases)
[![Rust Release](https://img.shields.io/github/v/release/link-foundation/link-cli?filter=rust-v*&label=Rust%20release)](https://github.com/link-foundation/link-cli/releases)
```

After:

```markdown
[![C# Release](https://img.shields.io/github/v/release/link-foundation/link-cli?filter=csharp-v*&label=C%23%20release)](https://github.com/link-foundation/link-cli/releases?q=C%23&expanded=true)
[![Rust Release](https://img.shields.io/github/v/release/link-foundation/link-cli?filter=rust-v*&label=Rust%20release)](https://github.com/link-foundation/link-cli/releases?q=Rust&expanded=true)
```

### PR #90 — NuGet badges in the C# release body

`csharp/scripts/create-github-release.mjs` now exports two helpers and uses them when building the release payload:

```js
export function buildNugetBadges(packageId, version) {
  const id = encodeURIComponent(packageId);
  const versionPath = encodeURIComponent(version);
  const versionUrl = `https://www.nuget.org/packages/${id}/${versionPath}`;
  const versionBadge = `[![NuGet](https://img.shields.io/nuget/v/${id}?logo=nuget&label=NuGet)](${versionUrl})`;
  const downloadsBadge = `[![NuGet Downloads](https://img.shields.io/nuget/dt/${id}?logo=nuget&label=downloads)](${versionUrl})`;
  return `${versionBadge} ${downloadsBadge}`;
}

export function prependNugetBadges(releaseNotes, packageId, version) {
  if (!packageId || !version) return releaseNotes;
  if (/img\.shields\.io\/nuget\//i.test(releaseNotes)) return releaseNotes;
  return `${buildNugetBadges(packageId, version)}\n\n${releaseNotes}`;
}
```

Effect: when the workflow calls the script with `--package-id clink --release-version <semver>`, the release body begins with a clickable NuGet version badge and a NuGet downloads badge — both linking to `https://www.nuget.org/packages/clink/<semver>` — followed by the changelog excerpt and the `Package: clink` footer. The next C# release will automatically embed the badges.

`csharp/scripts/release-scripts.test.mjs` covers:

- the dry-run end-to-end output includes both badges with the version-specific NuGet URL,
- the dry run **omits** badges when `--package-id` is absent,
- `buildNugetBadges` URL-encodes the package id and links both badges to the exact version,
- `prependNugetBadges` is a no-op when the body already contains a shields.io NuGet badge, and a no-op when either `packageId` or `version` is missing,
- `buildReleasePayload` places the badges above the changelog and the `Package:` footer.

### Backfilling the existing release

Because the published `csharp-v2.4.0` release was created before the fix, this PR also backfilled its body in place via `gh release edit csharp-v2.4.0 --notes-file <body-with-badges>.txt`. The post-fix release body is captured in `github-data/csharp-v2.4.0-release-after-fix.json`.

### Alternatives Considered

- Hard-coding `https://www.nuget.org/packages/<id>` (no version segment) for the badge target. NuGet redirects that URL to the latest version, so on a 2.4.0 release page users would jump to whichever version is latest at click time. The version-segment form keeps the click target stable per release.
- Adding the badge only to the README and leaving the release body alone. The issue comment ("Still no NuGet badges in C# GitHub releases") specifically asks for badges in the release pages themselves, where the Rust releases already show them, so a README-only fix would not close the loop.
- Updating the release body manually for every C# release. The template pattern, and the Rust precedent in this repo, is to generate badges in the release script — automation prevents drift on future releases.

## Template Comparison

The templates referenced by the issue were inspected for the same badge defects:

- `link-foundation/csharp-ai-driven-development-pipeline-template/scripts/create-github-release.mjs` _does_ already implement `buildNuGetBadge` and `appendNuGetBadgeIfMissing` and appends a NuGet badge to the release body. link-cli's C# release script had not been kept in sync — this PR ports the same idea (extended to a version + downloads pair, mirroring Rust's Crates.io + Docs.rs pair). A snapshot of the template script is preserved at `templates/csharp-ai-driven-development-pipeline-template/create-github-release.mjs`. No upstream change is required to the template for this defect.
- `link-foundation/js-ai-driven-development-pipeline-template/README.md`: no header badges at the top. Nothing to fix upstream for this exact defect. Evidence: `templates/js-template/README.md`.
- `link-foundation/rust-ai-driven-development-pipeline-template/README.md`: badges are `CI/CD Pipeline`, `Crates.io`, `Docs.rs`, `Rust Version`, `Codecov`, `License`. Nothing to fix upstream for this exact defect. Evidence: `templates/rust-template/README.md`.

None of the templates publish multiple languages from the same repository, so they have no need for a per-language GitHub release badge with a `filter` parameter; the README defect in link-cli (PR #89) is specific to the multi-language release model.

## Upstream Reports

There is no defect to forward upstream:

- For the README badge target (PR #89), no template README produces a version badge that links to the wrong target.
- For the release-body NuGet badge (PR #90), the csharp template already implements the badge logic. link-cli's drift from the template is the actual defect, and this PR closes that drift.

If a template later adds a per-language release badge with a `filter` parameter, the fix demonstrated here (`releases?q=<title-prefix>&expanded=true`) should be applied in the template at that time.

## Validation

- Manual: probe `https://github.com/link-foundation/link-cli/releases?q=C%23&expanded=true` and confirm the C# v2.4.0 card is the only one returned and is expanded. Evidence: `logs/releases-q-csharp-headers.txt`.
- Manual: probe `https://github.com/link-foundation/link-cli/releases?q=Rust&expanded=true` and confirm only Rust releases are returned. Evidence: `logs/releases-q-rust-headers.txt`.
- Automated: `node --test csharp/scripts/release-scripts.test.mjs` — six new assertions cover the NuGet badge generation, the dry-run output, the absence of badges when there is no package id, and the order of badges/notes/footer in the payload.
- Visual: open the rendered README on the branch and click the C# release badge. The browser navigates to the filtered C# releases page with the latest version expanded.
- Visual: open `https://github.com/link-foundation/link-cli/releases/tag/csharp-v2.4.0` and confirm two clickable NuGet badges (version + downloads) appear at the top of the release body and link to `https://www.nuget.org/packages/clink/2.4.0`.
