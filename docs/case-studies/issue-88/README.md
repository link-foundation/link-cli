# Issue 88 Case Study: C# Release Badge Did Not Link to the Specific Version

Issue: https://github.com/link-foundation/link-cli/issues/88

Prepared PR: https://github.com/link-foundation/link-cli/pull/89

Related release: https://github.com/link-foundation/link-cli/releases/tag/csharp-v2.4.0

## Requirements

Restated from issue #88:

1. The C# Release shields.io badge in `README.md` shows the latest C# version (e.g. `csharp-v2.4.0`) but clicking it lands on the generic releases page that mixes C# and Rust releases instead of the specific C# release version.
2. Apply the same best practices to comparable badges (the Rust Release badge has the same defect).
3. Compare the full GitHub workflow / CI/CD scripts tree against the C#, JS, and Rust AI-driven development pipeline templates and reuse best practices.
4. If the same issue exists in any template repository, report it upstream.
5. Preserve issue/PR/release data and analysis under `docs/case-studies/issue-88/`.
6. Search public sources for facts about how the shields.io GitHub release badge target works so the fix is grounded.
7. Add debug or verbose output if there is not enough data to find the root cause.
8. Plan and execute everything in this single PR.

## Timeline

- `2026-05-12T19:34:20Z`: GitHub Actions created the `csharp-v2.4.0` tag (`github-data/csharp-v2.4.0-release.json`).
- `2026-05-12T21:54:13Z`: Release `csharp-v2.4.0` was published.
- `2026-05-12T21:57Z`: Issue #88 was filed showing that the C# release badge in `README.md` linked to `/releases` rather than to `csharp-v2.4.0`. Evidence: `github-data/issue-88.json`.
- `2026-05-12T21:59Z`: Investigation reproduced the behavior. Probing showed:
  - The shields.io badge endpoint `https://img.shields.io/github/v/release/link-foundation/link-cli?filter=csharp-v*` returns the latest C# release version. Evidence: `logs/shields-filter-csharp-headers.txt`.
  - GitHub's filtered releases URL `https://github.com/link-foundation/link-cli/releases?q=C%23&expanded=true` returns HTTP 200 and lists only C# releases with the latest expanded at the top. Evidence: `logs/releases-q-csharp-headers.txt`.
  - The same pattern works for Rust with `q=Rust`. Evidence: `logs/releases-q-rust-headers.txt`.

## Evidence

- Issue and PR data: `github-data/issue-88.json`, `github-data/issue-88-comments.json`, `github-data/pr-89.json`, `github-data/pr-89-comments.json`, `github-data/pr-89-review-comments.json`, `github-data/pr-89-reviews.json`.
- Release data: `github-data/csharp-v2.4.0-release.json`.
- Probe headers: `logs/shields-filter-csharp-headers.txt`, `logs/shields-filter-rust-headers.txt`, `logs/releases-q-csharp-headers.txt`, `logs/releases-q-rust-headers.txt`, `logs/releases-tag-csharp-v2.4.0-headers.txt`.
- Template snapshots: `templates/csharp-template/README.md`, `templates/csharp-template/file-tree.txt`, `templates/js-template/README.md`, `templates/js-template/file-tree.txt`, `templates/rust-template/README.md`, `templates/rust-template/file-tree.txt`.
- Investigation timestamp: `github-data/investigation-timestamp.txt`.

## Online Facts

- The shields.io documentation for the GitHub release badge confirms a `filter` query parameter that narrows the badge to tags matching a glob, used here to separate C# and Rust release lines on the same repository. Source: https://shields.io/badges/git-hub-release
- GitHub serves a `releases` page query parameter `q` that filters by release title text and an `expanded=true` parameter that opens the matched release inline at the top. The link-cli release titles are formed `C# v<version>` and `Rust v<version>`, so `q=C%23` and `q=Rust` exactly partition the list (confirmed by inspecting the rendered HTML).
- In Markdown the badge target is the URL inside the outer parentheses: `[![alt](badge-image)](target-url)`. Markdown cannot evaluate the badge image to derive the target, so the target URL must be set explicitly. This is why the fix is a static URL that always points to the latest release of the language that matches the badge filter.
- GitHub's `/releases/latest` redirect returns the single most recent release across the whole repository, regardless of any `filter`, so it would land on the wrong language about half of the time and is not appropriate for a per-language badge.

## Root Cause

The badge image and its Markdown link target were derived independently when the file was written. The badge image was configured with `filter=csharp-v*` so that it displays only C# tags. The Markdown link target, however, was the generic `/releases` URL, which lists every release in the repository (C# and Rust mixed). A user reading the badge "C# release v2.4.0" expects the click target to land on the C# v2.4.0 release card. Instead they land on a mixed page where the latest Rust release may be at the top.

Markdown does not run JavaScript and cannot derive the target from the badge image, so a fix has to encode the language filter in the target URL itself. GitHub provides a stable equivalent of the shields.io `filter` parameter through the releases page query parameters `q=...` and `expanded=true`.

## Solution

`README.md` line 8 and line 9: change the badge target URL.

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

Result: clicking the C# release badge lands on `releases?q=C%23&expanded=true`, which displays C# releases only with the most recent expanded inline at the top of the page. Clicking the Rust badge does the same for Rust. The badge image already matches the same filter, so the version shown on the badge always matches the version expanded on the page.

This is the smallest change that fixes the defect, requires no workflow changes, no version pinning, and keeps the page in sync with the badge automatically when new releases are published.

### Alternatives Considered

- Hard-coding `/releases/tag/csharp-v2.4.0` would link to the exact version on the badge today but would stale on every new release; it was rejected.
- Using `/releases/latest` would always link to a single newest release across the whole repository, ignoring the per-language `filter` on the badge image; it would point at a Rust release from a "C# release" badge whenever Rust released last; it was rejected.
- A custom redirector or workflow step that rewrites the README on every release would be more code to maintain for the same outcome and was rejected.

## Template Comparison

The templates referenced by the issue were inspected for the same badge defect:

- `link-foundation/csharp-ai-driven-development-pipeline-template/README.md`: badges are `CI/CD Pipeline`, `.NET Version`, and `License`. No GitHub release badge. The `releases` page is not linked from the README. There is nothing to fix upstream for this exact defect. Evidence: `templates/csharp-template/README.md`.
- `link-foundation/js-ai-driven-development-pipeline-template/README.md`: no header badges at the top. No GitHub release badge. Nothing to fix upstream for this exact defect. Evidence: `templates/js-template/README.md`.
- `link-foundation/rust-ai-driven-development-pipeline-template/README.md`: badges are `CI/CD Pipeline`, `Crates.io`, `Docs.rs`, `Rust Version`, `Codecov`, `License`. No GitHub release badge. Nothing to fix upstream for this exact defect. Evidence: `templates/rust-template/README.md`.

None of the templates publish multiple languages from the same repository, so they have no need for a per-language GitHub release badge with a `filter` parameter. The defect in link-cli is specific to the multi-language release model.

The previous case study, `docs/case-studies/issue-86/README.md`, already pulled the full file tree of each template for the NuGet indexing investigation. Those trees were re-examined for this issue and confirm that no template README links the GitHub releases page in a way that would suffer from the same problem.

## Upstream Reports

There is no defect to forward upstream for this issue. The template READMEs were re-read, and none of them produce a version badge that links to the wrong target. If a template later adds a per-language release badge with a `filter` parameter, the fix demonstrated here (`releases?q=<title-prefix>&expanded=true`) should be applied in the template at that time.

## Validation

- Manual: probe `https://github.com/link-foundation/link-cli/releases?q=C%23&expanded=true` and confirm the C# v2.4.0 card is the only one returned and is expanded. Evidence: `logs/releases-q-csharp-headers.txt`.
- Manual: probe `https://github.com/link-foundation/link-cli/releases?q=Rust&expanded=true` and confirm only Rust releases are returned. Evidence: `logs/releases-q-rust-headers.txt`.
- Visual: open the rendered README on the branch and click the C# release badge. The browser navigates to the filtered C# releases page with the latest version expanded.
