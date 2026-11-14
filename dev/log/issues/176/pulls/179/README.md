# Issue 176: released install-action manifest is still missing

Checked on 2026-11-14 for [PR 179](https://github.com/link-foundation/link-cli/pull/179).
The cargo-audit upgrade remains **blocked and unfinished**. This PR must stay
unmerged until every acceptance criterion below is verified.

## Requirements and evidence

| Issue requirement | Status | Evidence |
| --- | --- | --- |
| Find an install-action release containing cargo-audit 0.23.0 | Blocked upstream | Latest release is [v2.90.3](https://github.com/taiki-e/install-action/releases/tag/v2.90.3), published 2026-11-08, commit `cfdb7ed4ccc7c7dce37759eae89dc5674b469b39`. Its [cargo-audit manifest](https://github.com/taiki-e/install-action/blob/cfdb7ed4ccc7c7dce37759eae89dc5674b469b39/manifests/cargo-audit.json) ends at 0.22.2. |
| Bump the action SHA in security.yml and wasm.yml | Unfinished; depends on the release | Both workflows still pin v2.90.3. There is no newer qualifying release to pin. |
| Set cargo-audit@0.23.0 and remove the held-back comment | Unfinished; depends on the release | security.yml retains the verified 0.22.2 pin and the open issue reference. |
| Verify all audit jobs use the verified binary without cargo-binstall fallback | Unfinished for 0.23.0 | The existing readiness probe rejects 0.23.0 and accepts 0.22.2 against the released manifest. |
| Keep the dependency exception active until the upgrade lands | Verified | The dependency check reports cargo-audit as held back by the open issue. The issue's reopening comment explains why closing it prematurely breaks CI. |

There are no required sub-issues. All issue comments and PR 179 conversation
comments, inline comments, and reviews were read; PR 179 initially had no feedback.

## Upstream progress

[install-action PR 2056](https://github.com/taiki-e/install-action/pull/2056),
commit `91cf0ab3ebe6fe5155e60a676023465b80804862`, now contains the 0.23.0
manifest entry. Its `x86_64_linux_musl` SHA-256 is
`4f01c74ed84b2dffd0b1d37012a62cdf2a92c96e25bdf8eec7fcc0d4d3eb7811`.
The PR is still open. Neither install-action's main branch nor its latest release
contains the entry. An unreleased PR commit does not satisfy the issue's release
requirement, so the workflow pins cannot yet be updated as requested.

## Reproduce and verify

From the repository root, run:

```sh
GITHUB_TOKEN=$(gh auth token) node experiments/issue-176/check-install-action-manifest.mjs
```

Current result (exit 1):

```text
install-action latest release: v2.90.3 (2026-11-08T10:54:37Z) cfdb7ed4ccc7c7dce37759eae89dc5674b469b39
cargo-audit manifest at that release ends at: 0.22.2
BLOCKED: cargo-audit@0.23.0 is not in the manifest of v2.90.3.
```

The same command with arguments `cargo-audit 0.22.2` exits 0 and prints the
currently pinned action SHA. This control confirms that the blocked result is
caused by the absent manifest entry, rather than authentication or network errors.

Once the probe succeeds for 0.23.0, apply its printed release SHA to both workflows,
upgrade the tool, and remove the held-back comment. Verify every Cargo audit matrix
job installs 0.23.0 without an unsupported-version warning or cargo-binstall
fallback. The Dependencies check must pass without a held-back cargo-audit entry.
Only then is the issue complete and the PR ready for merge.

## CI repairs in this PR

The initial Dependencies run, [43540789066](https://github.com/link-foundation/link-cli/actions/runs/43540789066),
started at 2026-11-14T01:28:25Z for `adeed4eb59c062aabc7bdfc6a35d75ebdfe1c638`.
Its downloaded log correctly reports cargo-audit as held back at lines 210–214,
then fails on 14 outdated declarations and lock entries at lines 216–234:
ESLint 10.15.0, links-notation 0.31.4, bumpalo 3.20.4, cc 1.11.2,
ctor 1.0.14, and link-section 0.19.4.

Those versions and their lock files are refreshed here. npm also refreshes ESLint's
compatible config-array, minimatch, and brace-expansion dependencies. Updating the
WASM lock file synchronizes its local link-cli entry with the already released
3.0.1 version. A Rust patch changelog fragment prepares the dependency update for
release. The root `.gitkeep` from task preparation is removed, as required by the
existing repository-layout test.

Focused local checks:

- Enforced dependency check: exit 0; only cargo-audit remains held back by issue 176.
- Dependency-checker tests: 20 passed.
- Repository-layout tests: 9 passed.
- npm lock update/audit: no vulnerabilities.
- `git diff --check`: passed.

The existing CI workflows cover compilation, linting, Rust and WASM tests, and
security audits. These builds are left to CI to conserve local CPU, memory, and
disk. Passing those checks does not fulfill the blocked upgrade criteria.
