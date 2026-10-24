# Issue #134: false positives, false negatives, warnings and errors in CI/CD

This is the case study for [issue #134](https://github.com/link-foundation/link-cli/issues/134)
and pull request [#135](https://github.com/link-foundation/link-cli/pull/135).
Every claim below links to a log, a reproduction or a test in this directory
tree (`dev/log/issues/134/pulls/135`).

Terms used:

- **False negative**: a defect that CI does not report (a green run that should be red, or a check that silently checks nothing).
- **False positive**: CI reports a problem that is not one (an error annotation on a green run, a non-report file uploaded as coverage).
- **Noise**: a warning or error line that needs no action, but trains readers to ignore real ones.

## 1. Requirements

| # | Requirement (issue text) | Where it is addressed |
| --- | --- | --- |
| R1 | Find all false positives, false negatives, warnings and errors in CI/CD | Sections 3 and 4; [`warnings-scan.txt`](warnings-scan.txt), [`main-annotations-before.txt`](main-annotations-before.txt), [`actionlint-before.txt`](actionlint-before.txt), [`zizmor-before.txt`](zizmor-before.txt), [`policy-runners-git-audit-before.txt`](policy-runners-git-audit-before.txt) |
| R2 | Fix them all | Section 4; each row names its commit and its regression test |
| R3 | Compare the full file trees with the rust, csharp and js pipeline templates and reuse their best practices | Section 5; template copies in [`../templates`](../templates) |
| R4 | Report the same issues to the templates when they have them | Section 6; [`../upstream`](../upstream) |
| R5 | Follow the hive-mind CI/CD best practices | Section 5; copy at [`../best-practices/CI-CD-BEST-PRACTICES.md`](../best-practices/CI-CD-BEST-PRACTICES.md) |
| R6 | Collect logs and data under `dev/log/issues/134/pulls/135` and write a deep analysis | This file. Raw run logs are downloaded to `../ci-logs`, which `.gitignore` excludes because of its size; the excerpts that matter are committed here |
| R7 | Leave debug output where a root cause was unclear | `List unified site (debug)` and `Print resolved deployment URL (debug)` in docs.yml; NuGet fetch failures now log `error.cause` (`4e3e753`); `CHECK_DEPENDENCIES_DEBUG=1` logs every retried dependency lookup (`036a87d`); the release lookups always print their retries |
| R8 | Everything in this one pull request | Pull request #135 |

## 2. Timeline

All times are UTC on 2026-10-24.

| Time | Event |
| --- | --- |
| 06:58, 07:18, 08:05 | Three pushes to main in a row. JS client CI fails each time in `Release to npm` with `ENEEDAUTH`, because `@link-foundation/link-cli-client` has neither an npm token nor a trusted publisher. Every other workflow is green (`gh run list --branch main`, saved as `../ci-logs/recent-runs-main.json`, which is not committed). |
| 08:05 | Main run on `4bfa252`: green runs that still carry warnings and errors. Rust has a file-size warning, the crates.io trusted publishing fallback warning and a `failure` annotation. CodeQL C# warns about overlay-base. Docs prints `DEP0040`. C# fails its Codecov upload with `Token required` behind `fail_ci_if_error: false` and logs `NuGet availability check warning: fetch failed`. |
| 08:29 | Issue #134 opened with the main run table. |
| 08:44 to 09:57 | Fixes land as atomic commits on `issue-134-3fbe4eecd082` (Section 4). |
| 09:45 | Every workflow on `59cfa94` is green. Its only annotations are the intended `CODECOV_TOKEN` notice and GitHub's macOS capacity notice. |
| 09:52 | Template issues filed (Section 6). |
| 10:01 | The Dependencies run on `0653ed0` fails on one dropped nuget.org connection, which is fixed by retrying transient lookups (Section 4.2). |
| 10:12 | The same audit of the Rust release scripts finds that a failed crates.io lookup counted as "not published" (Section 4.1). Fixed in `f3c5331`, and the same defects in the templates are reported as rust template #218 and csharp template #76. |

## 3. How the findings were collected

1. **Logs.** I downloaded the logs of every workflow on the main commit from the issue, and of the earlier failing JS client runs, with `gh run view <id> --log`. I grepped them for `##[warning]`, `##[error]`, `warning`, `error`, `deprecat` and `hint:` ([`warnings-scan.txt`](warnings-scan.txt)).
2. **Annotations.** I listed the check-run annotations of each main run through the REST API ([`main-annotations-before.txt`](main-annotations-before.txt)). Annotations are what a reviewer sees on a green run.
3. **Static analysis.**
   - actionlint 1.7.12 with shellcheck ([`actionlint-before.txt`](actionlint-before.txt)).
   - zizmor in regular mode and pedantic mode ([`zizmor-before.txt`](zizmor-before.txt): 126 findings, 23 high).
4. **Policy tests.** I ran the repository's own workflow policy tests against main after adding the new rules ([`policy-runners-git-audit-before.txt`](policy-runners-git-audit-before.txt): 16 failures).
5. **Reproductions** in throwaway repositories for the defects that only show up on main, such as release commits and pushes ([`../scripts`](../scripts)).
6. **Template diff.** I compared the file trees of the three templates ([`../templates/*-tree.txt`](../templates)) and diffed every CI script and workflow with its link-cli counterpart.

## 4. Findings, root causes and fixes

### 4.1 Release pipelines (false negatives with real impact)

| Finding | Root cause | Fix | Verification |
| --- | --- | --- | --- |
| **Rust releases were never committed.** crates.io received 1.0.6, 1.0.7 and 1.0.8 while `rust/Cargo.toml` on main stayed at 0.1.0. All 47 changelog fragments stayed in place, so every push to main cut another release, and `rust-v1.0.8` even tags the C# release commit. | `rust/CHANGELOG.md` did not exist. `version-and-commit.rs` wrote no changelog, the ignored `git add` of the missing path failed, nothing was staged, and the script reported "No changes to commit" with exit 0. The workflow then published the uncommitted version. | `ede1d44`:<br>- create `CHANGELOG.md` when it is missing and delete consumed fragments;<br>- sync `Cargo.lock` and verify the write before staging;<br>- fail on any staging error or empty index;<br>- rebase before writing;<br>- retry only non-fast-forward pushes;<br>- tag after the push.<br>Helpers split into `release-changelog.rs` (`d895137`, reused by `collect-changelog.rs` in `2faaa09`) and `release-write.rs` (`7e34acc`). | [`../scripts/repro-rust-release-commit.sh`](../scripts/repro-rust-release-commit.sh). Before (`e53813e`): [`rust-release-repro-before.txt`](rust-release-repro-before.txt), exit 0 with nothing pushed. After: [`rust-release-repro-after.txt`](rust-release-repro-after.txt), one release commit, tag `rust-v1.0.9`, 48 fragments consumed. Script tests run via `rust/scripts/test-scripts.sh` in the lint job. |
| **C# release could leave a local tag and fail on a lost race.** | It tagged before pushing, pushed with a bare `git push` plus `--tags`, and never fetched first, so a concurrent Rust release made the push non-fast-forward. Release descriptions were interpolated into a shell, and every release added one more blank line to the CHANGELOG (12 had accumulated). | `b3de0c3`:<br>- fetch and rebase first;<br>- push `HEAD:<branch>`;<br>- retry only lost races;<br>- fail fast with `::error` on GH006/GH013 rule rejections;<br>- tag after the push;<br>- run git through `execFileSync`. | `csharp/scripts/release-scripts.test.mjs` |
| **A missing NuGet key passed the release.** | The publish step printed "NUGET_API_KEY not set, skipping NuGet publish", and the job succeeded. | `e624ec2` fails with an `::error` naming the secret. `24c7e3e` adds NuGet trusted publishing via the `NUGET_USER` variable, as in the csharp template. | Policy test "publish steps do not silently skip on missing credentials" |
| **C# code changes without a changeset passed.** | The check printed `::warning::` instead of failing. Its `${{ github.head_ref }}` comparison was flagged by shellcheck SC2193 ("can never be equal") and is a template injection. | `eb50eb6` runs `validate-changeset.mjs`, which fails, and passes `head_ref` through `env`. | [`actionlint-before.txt`](actionlint-before.txt); zizmor clean |
| **A failed crates.io lookup counted as "not published".** One dropped connection made the Rust release decision republish a fully published version, which then fails with a misleading "VERSION ALREADY PUBLISHED … bug in version bumping". | `check-release-needed.rs` printed `Warning: Could not check crates.io` and returned `false` on any error. The same applied to Docker Hub, the GitHub release and the highest published version. `version-and-commit.rs` mapped `Err(_)` to `false`/`None`, so it bumped from `Cargo.toml` instead of the highest published version. Nothing retried. | `f3c5331`: the shared `rust/scripts/registry.rs`:<br>- retries transport errors, 429 and 5xx with exponential backoff;<br>- treats 404 as "not found";<br>- fails with `::error title=crates.io lookup failed::…` once the attempts run out. | [`../scripts/repro-rust-registry-transient.sh`](../scripts/repro-rust-registry-transient.sh) runs against a local fake registry that drops one connection. Before (`066ee83`): `should_release=true`, `skip_bump=true`. After (`f3c5331`): one retry, then `should_release=false`. When the connection always drops, the script exits 1 after 4 attempts: [`rust-registry-transient-repro.txt`](rust-registry-transient-repro.txt). Four unit tests in `registry.rs` run via `version-and-commit.rs`. |
| **Queued main runs were cancelled.** | csharp, rust, js-client, docs and wasm declared one workflow-level concurrency group. It held the whole run, including the release and the registry wait, so a third push to main cancelled the pending run. | `2643614`:<br>- per-job `check-<workflow>-<ref>-<job>` groups, cancelled only when superseded off main;<br>- writer jobs share never-cancelled groups. | Policy test "writer jobs own a never-cancelled concurrency group" |

### 4.2 Checks that checked less than they claimed (false negatives)

| Finding | Root cause | Fix | Verification |
| --- | --- | --- | --- |
| **Rust file-size check skipped `.github/**` and any path containing `target`.** | `path_str.contains(pattern)` on the absolute path; `.git` is a substring of `.github`. | `0c834ee` matches whole components relative to the scanned root, prunes with `filter_entry` and skips `dev/log` evidence. | Test `exclusions_match_whole_components_not_substrings`; [`template-file-size-repro-link-cli-fixed.txt`](template-file-size-repro-link-cli-fixed.txt) |
| **C# file-size check skipped `rust/examples/lino_binary_dump.rs` (`bin`) and `rust/src/sequences/target_matcher.rs` (`target`).** | `path.includes(pattern)`. It also swallowed `readdirSync` errors and counted a trailing newline as a line. | `b0fda3c` matches components and fails on read errors. Lines are counted like Rust's `str::lines`. It now finds 121 Rust files instead of 119. | `csharp/scripts/check-file-size.test.mjs`; [`csharp-file-size-exclusions.txt`](csharp-file-size-exclusions.txt) |
| **`cargo audit` passed with warning-level advisories.** | Unmaintained, unsound and yanked advisories are warnings and leave the exit status at 0. | `12cc6ef` adds `--deny warnings` on both lockfiles, as the Rust template does. | [`cargo-audit-deny-warnings.txt`](cargo-audit-deny-warnings.txt); policy test "every cargo audit invocation denies warnings" |
| **Outdated pins were invisible to the dependency check.** | It read only major-tag actions, taiki-e tool lists and runtime versions. It ignored SHA or digest pins with a `# vX.Y.Z` comment, exact-tag actions, `cargo install --version` and `pipx run pkg==ver`. | `09ff142` teaches it those forms. It found `wasm-pack` 0.14.0 < 0.15.0 and `crates-io-auth-action` v1.0.5 < v1.0.6, both bumped in `08c2651`. | `check-latest-dependencies.test.mjs`; [`dependencies-extended-before.txt`](dependencies-extended-before.txt) / [`dependencies-extended-after.txt`](dependencies-extended-after.txt) |
| **Workflow definitions were never linted.** | Nothing ran actionlint or zizmor. The policy tests ran only when a C# or Rust path changed. | `1b8551d` adds `workflows.yml`:<br>- actionlint, digest-pinned with shellcheck;<br>- zizmor 1.31.1 plus a pedantic high/high pass;<br>- `node --test .github/scripts/*.test.mjs` on every `.github/**` change. | The Workflows run on this pull request |
| **The runner OS changed without a diff.** | `ubuntu-latest` already ran Ubuntu 26.04 while runner-images documented it as 24.04. | `eb09cdd` pins `ubuntu-24.04`, `macos-26` and `windows-2025`. | Policy test "hosted runners use pinned OS labels" |
| **Coverage was never uploaded.** | Codecov accepts tokenless uploads only from forks, so every upload failed with `Token required - not valid tokenless upload` behind `fail_ci_if_error: false`. The Codecov API reports the repository as `"activated": false, "totals": null`. | `59cfa94`:<br>- upload with the token and `fail_ci_if_error: true`;<br>- `.github/codecov.yml` sets `require_ci_to_pass`;<br>- a `Codecov Token` job fails on main with what to do (Section 7) and blocks no release; pull requests stay green. | [`codecov-before.txt`](codecov-before.txt); policy test "every Codecov upload uses a token, reads only its reports and fails loudly" |
| **One dropped connection failed the dependency check, and the C# release decision.** The Dependencies run on `0653ed0` failed with `coverlet.collector: fetch failed`, although the same dependencies passed on `5d5a3d6` minutes earlier. | Every registry lookup and `git ls-remote` ran exactly once. Node's fetch reports every network error as `fetch failed` and keeps the reason in `cause`. `check-release-needed.mjs`, which decides whether main releases C#, also queried nuget.org and the GitHub API once. | `036a87d` and `622610a`:<br>- retry network failures, 429 and 5xx with exponential backoff;<br>- fail at once on answers like 404;<br>- add the network cause and the attempt count to the message;<br>- log retries with `CHECK_DEPENDENCIES_DEBUG=1`. | [`dependencies-transient-fetch.txt`](dependencies-transient-fetch.txt); [`../scripts/release-needed-transient-fetch.mjs`](../scripts/release-needed-transient-fetch.mjs) fails on the csharp template and passes after `622610a` ([`release-needed-transient-fetch-repro.txt`](release-needed-transient-fetch-repro.txt)); four tests in `check-latest-dependencies.test.mjs` and three in `release-scripts.test.mjs` fail against the previous code |
| **Security findings in the workflows.** zizmor reported 23 high findings: unpinned third-party actions, `${{ }}` template injection into `run:`, persisted checkout credentials and broad `pages`/`id-token` permissions. | Workflows grew without an auditor. | `fb1e88e` and `eb50eb6` pin actions, pass values through `env`, set `persist-credentials: false` except in pushing jobs, and scope permissions per job. `.github/zizmor.yml` mirrors the template pin policy. | [`zizmor-before.txt`](zizmor-before.txt); zizmor job reports no findings |

### 4.3 False positives

| Finding | Root cause | Fix | Verification |
| --- | --- | --- | --- |
| **Green Rust releases carried a `failure` annotation:** `No Trusted Publishing config found for repository link-foundation/link-cli`, plus a fallback warning. | `crates-io-auth-action` ran on every release with `continue-on-error: true`, although trusted publishing is not configured on crates.io. | `5d5a3d6` gates the step on the `CRATES_IO_TRUSTED_PUBLISHING` repository variable, as `NUGET_USER` gates NuGet/login. Without it the token secret is used. A missing token fails with an `::error` naming both options. `continue-on-error` is now banned outright. | Policy tests "does not mask failures with continue-on-error" and "crates.io trusted publishing runs only when enabled"; both fail on the previous rust.yml |
| **Green C# and WebAssembly runs had an `::error title=C# release failed::` annotation.** | `execFileSync` forwards the child's stderr unless `stdio` is set, so a test's expected failure output reached the runner as a workflow command. Git push and clone chatter leaked the same way. | `88a9988` pipes stdio in the test helpers. | [`test-stderr-leak-before.txt`](test-stderr-leak-before.txt) / [`test-stderr-leak-after.txt`](test-stderr-leak-after.txt) |
| **Codecov uploaded `rust/tests/issue62_review_coverage_tests.rs` as a coverage report.** | The uploader searched the whole checkout for `*coverage*`. | `59cfa94` sets `directory:` to the dotnet test results and `plugins: noop`. | [`codecov-before.txt`](codecov-before.txt) |

### 4.4 Warnings and noise

| Warning | Root cause | Fix |
| --- | --- | --- |
| `File has 903 lines (approaching limit of 1000)`, every Rust lint | `link_storage.rs` had a private byte-identical copy of `escape_lino_reference` | `4210b1e` reuses the shared function. `7e34acc` keeps `version-and-commit.rs` (967 lines after the release fix) under the 900 warning line. |
| `Cannot build an overlay-base database because build-mode is set to "autobuild"` (CodeQL C#) | C# was analysed with autobuild | `cfd0351` switches to `build-mode: none`. The CodeQL config ignores `dev/log`, and `a787822` also ignores `docs/case-studies`, whose template copies gave "semantic analyzer unavailable (no manifest found)" warnings. |
| `[DEP0040] DeprecationWarning: The punycode module is deprecated` (Docs deploy) | `actions/deploy-pages` v5.0.1 loads `punycode` ([actions/deploy-pages#413](https://github.com/actions/deploy-pages/issues/413)) | `8d7d242` adds `NODE_OPTIONS: --disable-warning=DEP0040` on that step only |
| `License key is set in Cargo.toml but no LICENSE file(s) were found` (wasm-pack) | wasm-pack copies LICENSE only from the crate directory | `7a3e501` ships `rust/wasm/LICENSE` ([`wasm-pack-0.15-build-before-license.txt`](wasm-pack-0.15-build-before-license.txt)) |
| `hint: Using 'master' as the name for the initial branch` in every checkout | `actions/checkout` runs `git init` before any step can configure Git | `eb09cdd` sets `GIT_CONFIG_COUNT/KEY_0/VALUE_0=init.defaultBranch=main` in every workflow `env` |
| `NuGet availability check warning: fetch failed` | undici keeps the real reason in `error.cause`, which was not printed | `4e3e753` logs the cause, which is the debug output for an unclear root cause |
| `xcrun is not installed`, `No gcov data found`, `coverage.py is not installed` (Codecov) | Default uploader plugins | `59cfa94` sets `plugins: noop` |

## 5. Template comparison and best practices

The three templates were cloned at:

| Template | Commit |
| --- | --- |
| rust | `265219d` |
| csharp | `4cc372e` |
| js | default branch |

Their trees are in [`../templates/*-tree.txt`](../templates); the scripts that were compared are copied under `../templates/<lang>/scripts*`.

**Adopted from the templates:**
- `workflows.yml` (actionlint, zizmor, policy tests);
- per-job concurrency;
- the zizmor pin policy;
- `cargo audit --deny warnings`;
- the release-write and push-classification logic of `version-and-commit`;
- NuGet trusted publishing through `NUGET_USER`;
- fail-loud missing credentials;
- the rust-script pin with `--locked`;
- CodeQL `paths-ignore` for evidence directories;
- Codecov upload restricted to its reports (Rust template: `files:` with `disable_search`).

**Best-practices document.** These sections are applied:
- pin everything, and audit the pins;
- least-privilege permissions per job;
- fail loudly and never `continue-on-error`;
- a check that needs a human must fail on the default branch with what to do, while pull requests stay green;
- reproduce with a test first;
- no noise in green logs.

**Deliberate deviation.** Concurrency uses `queue: max` in the document, but actionlint 1.7.12 rejects that key. Writer jobs therefore keep per-target never-cancelled groups.

**Defects found in templates.** These were not copied (Section 6):
- substring exclusions in both file-size checks;
- release lookups that run once (csharp) or read a failure as "not published" (rust);
- the unrestricted Codecov search in the csharp template.

The js template's `check-file-line-limits.sh` uses `git ls-files` with case globs and is not affected.

## 6. Reports filed in the templates

| Template issue | Problem | Reproduction |
| --- | --- | --- |
| [rust-ai-driven-development-pipeline-template#217](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/issues/217) | `check-file-size.rs` skips `src/retarget.rs`, all of `.github/**` and every file of a checkout named `*target*` | [`../scripts/template-file-size-substring-exclusion.sh`](../scripts/template-file-size-substring-exclusion.sh), [`template-file-size-repro.txt`](template-file-size-repro.txt) |
| [csharp-ai-driven-development-pipeline-template#74](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/issues/74) | `check-file-size.mjs` skips `src/Robin.cs`, all of `.github/**` and every file of a checkout named `*bin*` | same |
| [rust-ai-driven-development-pipeline-template#218](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/issues/218) | `check-release-needed.rs` and `version-and-commit.rs` read a failed crates.io, Docker Hub or GitHub lookup as "not published", so one dropped connection re-releases a published version | [`../scripts/repro-rust-registry-transient.sh`](../scripts/repro-rust-registry-transient.sh), [`template-rust-registry-transient-repro.txt`](template-rust-registry-transient-repro.txt) |
| [csharp-ai-driven-development-pipeline-template#76](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/issues/76) | `check-release-needed.mjs` queries NuGet and the GitHub releases API once, so one dropped connection fails the release job | [`../scripts/release-needed-transient-fetch.mjs`](../scripts/release-needed-transient-fetch.mjs), [`release-needed-transient-fetch-repro.txt`](release-needed-transient-fetch-repro.txt) |
| [csharp-ai-driven-development-pipeline-template#75](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/issues/75) | The Codecov step searches the whole checkout and runs every plugin, and a missing `CODECOV_TOKEN` never fails on main (follow-up to its #34) | link-cli logs in [`codecov-before.txt`](codecov-before.txt) |

**Already reported, so not filed again:**
- the CodeQL C# `autobuild` overlay warning (csharp template #73).

**Not filed:**
- The js template's release lookups do not retry either, but `github-release-state.mjs` returns `null` ("unknown") and `npm-registry.mjs` throws, so it never guesses "not published".
- The Rust template's release script errors on a missing `CHANGELOG.md` pathspec. The template ships `CHANGELOG.md` and the failure is loud, so it is not a false negative there.

Each issue body is also kept in [`../upstream`](../upstream).

## 7. What still needs a human

These cannot be done from a pull request because they need repository settings or registry accounts. Each one fails on main with an explicit message, and pull requests stay green.

| Action | Effect until it is done |
| --- | --- |
| Activate `link-foundation/link-cli` on [Codecov](https://app.codecov.io/gh/link-foundation) and add its upload token as the `CODECOV_TOKEN` secret | The `Codecov Token` job of the C# pipeline fails on main with `CODECOV_TOKEN is missing`. Releases are not blocked. |
| Add an npm automation token as `NPM_TOKEN`, or configure `js-client.yml` as a trusted publisher of `@link-foundation/link-cli-client` (npm needs the package to exist first, so the first publish needs the token) | `Release to npm` fails on main with `NPM_TOKEN is missing`, as it did before this pull request |
| Optional: configure crates.io trusted publishing for `link-cli` and `rust.yml`, then set the `CRATES_IO_TRUSTED_PUBLISHING` variable to `true` | None. The `CARGO_REGISTRY_TOKEN` secret keeps publishing. |
| Optional: configure a nuget.org trusted publishing policy and set `NUGET_USER` | None. The `NUGET_API_KEY` secret keeps publishing. |

## 8. Remaining accepted output

- `gpg: WARNING: This key is not certified with a trusted signature!` comes from codecov-action's own uploader verification. It is printed by the action and cannot be configured.
- `npm notice npm tokens that bypass 2FA are being restricted…` is an npm registry announcement printed on every publish.
- CodeQL prints a JSON dump of its configuration that contains the word `DEPRECATED`. It is informational and does not come from this repository's config.
- `Due to capacity constraints, jobs targeting macOS arm64 runners may experience longer queue times` is a GitHub platform notice.

## 9. Tools and libraries

| Tool | Used for |
| --- | --- |
| [actionlint](https://github.com/rhysd/actionlint) 1.7.12 (Docker image bundles shellcheck) | Workflow syntax and shell checks |
| [zizmor](https://docs.zizmor.sh) 1.31.1 | Workflow security audit |
| [rust-script](https://rust-script.org) 0.36.0 | Release and check scripts with `#[cfg(test)]` suites |
| `node:test` | Dependency-free policy and script tests |
| [cargo-audit](https://github.com/rustsec/rustsec/tree/main/cargo-audit) | Advisory check with `--deny warnings` |
| [codecov-action](https://github.com/codecov/codecov-action) v7 (`directory`, `plugins`, `fail_ci_if_error`) | Coverage upload |
| [crates-io-auth-action](https://github.com/rust-lang/crates-io-auth-action), [NuGet/login](https://github.com/NuGet/login) | Trusted publishing |
| [secretlint](https://github.com/secretlint/secretlint) | Committed-secrets scan, which also covers this evidence |

## 10. Reproducing the checks locally

```bash
node --test .github/scripts/*.test.mjs          # workflow policy and dependency check tests
node --test csharp/scripts/*.test.mjs           # C# release and file-size scripts
bash rust/scripts/test-scripts.sh               # rust-script suites
docker run --rm -v "$PWD:/repo" -w /repo rhysd/actionlint:1.7.12 -color
pipx run zizmor==1.31.1 --config .github/zizmor.yml .github
bash dev/log/issues/134/pulls/135/scripts/repro-rust-release-commit.sh WORKTREE
bash dev/log/issues/134/pulls/135/scripts/template-file-size-substring-exclusion.sh \
  <csharp-template>/scripts/check-file-size.mjs <rust-template>/scripts/check-file-size.rs
bash dev/log/issues/134/pulls/135/scripts/repro-rust-registry-transient.sh HEAD [always]
REPRO_REPO=<rust-template> REPRO_SCRIPTS=scripts \
  bash dev/log/issues/134/pulls/135/scripts/repro-rust-registry-transient.sh HEAD
node dev/log/issues/134/pulls/135/scripts/release-needed-transient-fetch.mjs csharp/scripts/check-release-needed.mjs
```
