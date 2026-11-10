# Upstream template audit for defects fixed in link-cli PR #174

These reports are for the maintainers of the five link-foundation pipeline templates. Each template was inspected read-only at the commit listed below. Each draft was filed as one issue, with its first line as the title and the rest as the body.

| Template | Commit | Filed issue |
|---|---|---|
| rust-ai-driven-development-pipeline-template | `88fd935f69e303ac20f02d414bf92ab32d99a104` | https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/issues/236 |
| csharp-ai-driven-development-pipeline-template | `1f3b92f71c22717102a21cc5576a514cf11e674f` | https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/issues/100 |
| js-ai-driven-development-pipeline-template | `34415b00490262a8582028099139fe913e7e0cb3` | https://github.com/link-foundation/js-ai-driven-development-pipeline-template/issues/297 |
| java-ai-driven-development-pipeline-template | `0d3a4e3e96d5e93d50a133b1f9eadd91241629cd` | https://github.com/link-foundation/java-ai-driven-development-pipeline-template/issues/22 |
| python-ai-driven-development-pipeline-template | `f28c9d836cd18426adbc2f17629ac58202e04a83` | https://github.com/link-foundation/python-ai-driven-development-pipeline-template/issues/145 |

Defects:

| # | Defect | link-cli fix |
|---|---|---|
| 1 | lychee timeouts are ignored, exit codes 1 and 3 are treated as recoverable, and there is no schedule | yes |
| 2 | compressed tracked files are not scanned for secrets | yes |
| 3 | the CodeQL gate prints unquoted messages (workflow-command injection), stops at the first problem, or hides findings | yes |
| 4 | PR runs save caches, or use GHA Docker `mode=max`, that evict main's caches | yes |
| 5 | the file-size check skips non-primary languages, or has no 900-line warning band | yes |
| 6 | a workflow with a terminal gate is missing from the infrastructure-rerun list | yes |
| 7 | a NuGet 403 (key without push scope) passes the credential preflight | yes |
| 8 | python: the release push's rebase retry is unreachable; the lychee installer is verified only against its own release's checksum | no link-cli counterpart |
| 9 | java: the freshness gate conflicts with Dependabot cooldown and rejects SHA pins; curl has no `--retry-all-errors` | no link-cli counterpart |
| 10 | rust: the release source guard rejects sibling-language release commits in a monorepo | no link-cli counterpart |
| 11 | csharp: SHA pins carry major-only `# vN` comments | no link-cli counterpart |
| 12 | a failed release-credential preflight skips the lint, test and build jobs | yes |
| 13 | pull-request change detection diffs only the newest commit, so a cancelled earlier run plus a docs-only commit skips the checks | yes |

Results by template:

| # | rust | csharp | js | java | python |
|---|---|---|---|---|---|
| 1 | confirmed | confirmed | absent | N/A (no links workflow) | absent |
| 2 | absent (gunzips `.gz`; `.tgz`/`.zip` would pass raw, none tracked) | N/A (no secrets scanner) | confirmed | N/A (no secrets scanner) | latent (same `secretlint "**/*"`, no compressed files tracked) |
| 3 | confirmed (injection, stops at first, diagnostics hide findings) | confirmed, partial (stops at first, lists no findings; no injection) | confirmed, partial (injection only) | N/A | absent |
| 4 | confirmed | absent (no caches) | confirmed (minor: PR-only Docker `mode=max`, per-commit lychee cache) | absent | confirmed (minor: same as js) |
| 5 | confirmed (`.rs`/`.md` only) | confirmed (`.cs` only) | absent | confirmed (`.java` only, no warning band) | confirmed (`.py`/`.md` only) |
| 6 | absent | confirmed (Security not rerun) | absent | absent | N/A (no rerun workflow, Ubuntu only) |
| 7 | N/A | confirmed | N/A | N/A | N/A |
| 8 | N/A | N/A | confirmed (lychee checksum part only) | N/A | confirmed (dead rebase plus lychee checksum) |
| 9 | N/A | N/A | N/A (same curl pattern in `scripts/setup-preview-gh.sh:19`, noted) | confirmed (9a, 9b, 9c) | N/A |
| 10 | confirmed | N/A | N/A | N/A | N/A |
| 11 | N/A | confirmed (22 pins) | N/A | N/A | N/A |
| 12 | absent | confirmed | absent | N/A (no preflight job) | absent |
| 13 | confirmed | N/A (no change detection) | absent (full PR diff unless the previous head passed; per-commit only without `GITHUB_BASE_SHA`) | N/A (no change detection) | N/A (no change detection) |

**Duplicates.** `gh issue list --state all --search ...` was run for every defect in every repository. No open or closed issue duplicates any confirmed item except defect 13, which is already open as rust#235; the link-cli evidence was added there as a comment instead of a new issue. The related closed issues each draft cites are:

| Template | Related closed issues |
|---|---|
| rust | #228, #136, #170, #40, #211, #226, #163, #154, #231 |
| csharp | #47, #96, #51, #66, #80, #74, #82 |
| js | #278, #286, #266, #138, #184, #165, #176, #280, #291 |
| java | #14, #16 |
| python | #73, #131, #66, #74, #122, #141 |

Draft files:

| File | Items |
|---|---|
| `rust.md` | 1, 3, 4, 5, 10, 13 |
| `csharp.md` | 12, 7, 1, 6, 3, 5, 11 |
| `js.md` | 2, 3, 4, 8 |
| `java.md` | 9, 5 |
| `python.md` | 8, 4, 5 |
