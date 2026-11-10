## Summary

Fixes #172

This PR audits every warning, error, false positive and false negative in the `main` runs at `199a07b` listed in the issue, finds the root cause of each and fixes it. Each fix has a regression test, except one configuration-only change. Everything is in one PR. The only item left needs a maintainer: the NuGet credential, described [below](#blocked-on-a-maintainer).

All evidence is in [`dev/log/issues/172/pulls/174`](https://github.com/link-foundation/link-cli/blob/issue-172-4d10c904b01e/dev/log/issues/172/pulls/174/README.md):

- full CI logs, run and job metadata, and per-job annotations
- template trees and diffs
- primary sources: lychee's report writer, NuGetGallery's scope attributes, and the CI/CD best-practices document
- the [audit](https://github.com/link-foundation/link-cli/blob/issue-172-4d10c904b01e/dev/log/issues/172/pulls/174/analysis/AUDIT.md) and the [timeline](https://github.com/link-foundation/link-cli/blob/issue-172-4d10c904b01e/dev/log/issues/172/pulls/174/analysis/timeline.md)

## Findings and fixes

| # | Kind | Problem | Root cause | Fix | Regression test |
| --- | --- | --- | --- | --- | --- |
| F1 | error | C# release preflight fails on `main`: NuGet verification returns 403, and the message blames a non-existent "PackageVerify scope" | `NUGET_USER` is unset, and `NUGET_API_KEY` lacks the Push scope (NuGetGallery requires `PackagePush` on that route) | Message corrected (`9b462e60`); the credential itself needs a maintainer | `nuget-auth.test.mjs` |
| F2 | false negative | C# lint, 3× test and build were **skipped** on `main` because the preflight failed. The same gating existed in `rust.yml` and `js-client.yml` | Code-check jobs `needs:` the release preflight | Only publish jobs wait on the preflight; `Pipeline Status` still fails `main` explicitly (`2568dead`) | `workflow-policy.test.mjs` "a failed release preflight blocks publishing but never the code checks" |
| F3 | false positive | Rust `Test (windows-2025)` cancelled at 45 min after all tests passed | The cargo cache always missed: PR caches evicted `main`'s, and `test` shared a key with `lint`. The ~3 GB post-step save ran past the timeout | Restore everywhere; save only on `main` pushes on a miss; `test` has its own key and saves before the tests run (`1f286358`) | `workflow-policy.test.mjs` cache tests |
| F4 | warning | 4× `File has 9xx lines` in Rust | Files in the 900–1000 band | Split along cohesive boundaries (`283d6c4e`) | existing size gate, now warning-free |
| F5 | false negative | C# size check silent until 1000 (`Program.cs` had 986 lines) | No warning band | Band added and two files split (`db30ae27`) | `check-file-size.test.mjs` |
| F6 | false negative | JS size check silent until 1000 | No warning band | Same band through a shared helper (`2605260d`, `be94ffa1`) | `check-file-size.test.mjs` |
| F7 | false negative | Timed-out links were ignored by the recheck | lychee writes them under `## Timeouts per input` | Both sections parsed and validated (`90d9a675`) | `link-failures.test.mjs` |
| F8 | false negative | A lychee crash (exit 1) or config error (exit 3) could be "recovered" | Any non-zero exit was treated as link failures | Only exit 2 is recoverable (`bd40ec63`) | `link-failures.test.mjs` |
| F9 | false negative | Link rot went unnoticed when no Markdown changed | No schedule | Weekly run (`bd40ec63`) | n/a (configuration) |
| F10 | false negative / positive | A record could borrow its URL from the next line | `\s` in the record regex matches newlines | Horizontal whitespace only (`a2592ddf`) | `link-failures.test.mjs` "a lychee record never borrows text from the next line" |
| F11 | false negative (security) | Secrets in committed `.tar.gz` / `.json.gz` were never scanned | Only `.log.gz` was decompressed | Every `.gz` is decompressed (capped) and tarballs are unpacked (`cd975442`) | `check-secrets.test.mjs` |
| F12 | false negative (security) | CodeQL SARIF text was printed raw (it could forge workflow commands); only the first problem was reported | Unquoted untrusted output; the check threw on the first problem | Text is JSON-quoted and all problems are collected (`a4d61615`) | `check-codeql-results.test.mjs` |
| F13 | false positive | `Crash tests` jobs that never got a runner stayed red | Missing from the infrastructure-rerun list | Added (`26133a71`) | `pipeline-status.test.mjs` (rerun list = gated workflows) |
| F14 | warning | Dependencies reported 5 outdated packages | New upstream releases | Updated; C# build has 0 warnings and 3439 tests pass (`a2de1e79`). cargo-audit is held at 0.22.2 (F19) | Dependencies workflow |
| F15 | warning | `cargo install wasm-bindgen-cli` printed a future-incompatibility warning and compiled for ~1.5 min | wasm-bindgen/wasm-bindgen#3356 (already reported upstream) | Prebuilt, pinned binaries via `taiki-e/install-action` (`5b729db4`) | actionlint + zizmor |
| F16 | notice | `ubuntu-latest` migration notice | Only from GitHub's own Dependabot jobs; every workflow here already pins `ubuntu-24.04` | none needed | existing pin test |
| F17 | notice | macOS arm64 queue-time notice | GitHub capacity | Covered by the infrastructure rerun (F13) | existing |
| F18 | false negative | Found while verifying this PR: a green Rust run for head `067cb2c3` had **skipped** Rust lint and test, although the PR changes Rust | PR change detection diffed only the last commit (`HEAD^2^..HEAD^2`), whose earlier runs had been superseded | Diff the whole PR against its base (`HEAD^1..HEAD`), as the C# detector already does (`38ef331a`) | `detect-code-changes.rs` test `main_merge_and_pr_both_check_every_change_of_the_branch` |
| F19 | warning | Found while verifying this PR: `Security` warned that install-action cannot verify `cargo-audit@0.23.0` and fell back to an unverified download | F14's bump outran the pinned action's manifest | Held at `0.22.2` with a blocker comment that points at #176, so the Dependencies check reports it as held, not outdated (`b373d4bb`) | Dependencies workflow, `Security` annotations |

## Blocked on a maintainer

**NuGet publishing credentials (F1).** No code change can fix this. A maintainer needs to do one of the following:

1. Set the repository variable `NUGET_USER` to the nuget.org account that owns `clink`, and add a nuget.org trusted-publisher policy for this repository and `csharp.yml`.
2. Or renew `NUGET_API_KEY` with the **Push** scope for `clink`.

Until then:

- `main` fails `Pipeline Status (csharp)` with that explicit remediation message.
- Pull requests pass, because the preflight is advisory there.
- Since F2, lint, tests and build still run and report the code's own status.

## Template comparison and upstream reports

The five `*-ai-driven-development-pipeline-template` repositories were compared file by file; see the [audit](https://github.com/link-foundation/link-cli/blob/issue-172-4d10c904b01e/dev/log/issues/172/pulls/174/analysis/AUDIT.md#template-comparison).

- **Adopted:**
  - horizontal-whitespace lychee parsing, from the csharp template
  - timeout-section parsing
- **Not adopted** (reasons in the audit):
  - the lychee install script
  - pin comments
  - global cancel-in-progress

The template defects found here were reported upstream:

- link-foundation/rust-ai-driven-development-pipeline-template#236: lychee timeouts and exit codes, CodeQL output quoting, PR cache eviction, size-check coverage, and release sync in monorepos
- link-foundation/rust-ai-driven-development-pipeline-template#235 already reported F18, so the link-cli evidence and fix were [added there as a comment](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/issues/235#issuecomment-6779540622)
- link-foundation/csharp-ai-driven-development-pipeline-template#100: preflight skips lint/test/build, a NuGet 403 passes the preflight, lychee, Security missing from the rerun list, CodeQL, size check, and pin comments
- link-foundation/js-ai-driven-development-pipeline-template#297: compressed logs not scanned for secrets, CodeQL message injection, PR cache writes, and lychee checksum source
- link-foundation/java-ai-driven-development-pipeline-template#22: the freshness gate conflicts with Dependabot cooldown and SHA pins, curl retries, and size-check coverage
- link-foundation/python-ai-driven-development-pipeline-template#145: the rebase retry is unreachable, PR cache writes, size-check coverage, and lychee checksum source

Drafts with permalinks, reproductions, workarounds and suggested fixes are in [`upstream-reports/`](https://github.com/link-foundation/link-cli/blob/issue-172-4d10c904b01e/dev/log/issues/172/pulls/174/analysis/upstream-reports/SUMMARY.md). The wasm-bindgen warning (F15) was already reported as wasm-bindgen/wasm-bindgen#3356.

## How to reproduce and verify

- Each regression test listed above was run against the pre-fix code and fails there.
- Run the tests locally:
  - `node --test .github/scripts/*.test.mjs` (264 pass)
  - `node --test csharp/scripts/*.test.mjs` (47 pass)
  - `rust-script --test rust/scripts/detect-code-changes.rs` (5 pass)
  - `npm --prefix js run lint` (0 findings)
  - actionlint: `docker run --rm -v "$PWD:/repo" -w /repo rhysd/actionlint`
- Changesets: `csharp/.changeset/issue-172-ci-audit.md` and `rust/changelog.d/20261110_200000_issue-172-ci-audit.md` (patch).
