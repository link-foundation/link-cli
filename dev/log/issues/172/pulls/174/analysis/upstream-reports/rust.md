# CI gates: lychee timeouts pass, CodeQL output is unquoted, PR runs evict main's caches, .mjs files skip the size check, and release sync rejects sibling-language releases

Repository: `link-foundation/rust-ai-driven-development-pipeline-template`
Commit inspected: `88fd935f69e303ac20f02d414bf92ab32d99a104`

These defects were found and fixed downstream in link-foundation/link-cli (https://github.com/link-foundation/link-cli/pull/174). No open duplicate exists. The related closed issues #228, #136, #170, #40, #211, #154 and #231 cover neighbouring behaviour but not these gaps.

---

## 1. The link check ignores lychee's "Timeouts per input" section and does not distinguish exit codes 1 and 3 from 2

**Summary.** `extractErrorsSection` reads only the `Errors per input` section. A link that times out is listed under a separate `Timeouts per input` heading, which the recheck never sees. If the report contains a recoverable error plus a timeout, the run reports `all_recovered=true` and goes green. A report that contains only timeouts has no `Errors per input` section, so it is rejected as "malformed" instead of being treated as a link failure. Any non-zero exit code is treated as recoverable, including 1 (runtime error) and 3 (config error). The workflow also has no `schedule:`, so link rot on an idle repository is never detected.

**Evidence**
- `scripts/check-web-archive.mjs:63-70` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/scripts/check-web-archive.mjs#L63))
  ```js
  export function extractErrorsSection(content) {
    const lines = content.split('\n');
    const start = lines.findIndex((line) =>
      /^#+\s+Errors per input\s*$/.test(line)
    );
    if (start === -1) {
      return content;
  ```
- `scripts/check-web-archive.mjs:217-218` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/scripts/check-web-archive.mjs#L217)): only the `Errors` summary row is counted, and every non-zero exit code is handled the same way.
  ```js
  const reported = /\|[^|\n]*Errors\s*\|\s*(\d+)\s*\|/.exec(content);
  const failed = exitCode !== undefined && exitCode !== '0';
  ```
- `scripts/recheck-broken-links.mjs:69` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/scripts/recheck-broken-links.mjs#L69)): `const section = extractErrorsSection(content);`
- `.github/workflows/links.yml:3-31` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/.github/workflows/links.yml#L3)): the triggers are `push`, `pull_request` and `workflow_dispatch`, with no `schedule:`. L108 `if: steps.lychee.outputs.exit_code != 0` sends every non-zero exit code to the recheck step.

**MRE.** Feed `scripts/recheck-broken-links.mjs` a lychee markdown report with `LYCHEE_EXIT_CODE=2` that contains:
- `## Errors per input` with one `[503]` URL that answers 200 on recheck;
- `## Timeouts per input` with one `[TIMEOUT]` URL;
- a summary table with `| ⏳ Timeouts | 1 |`.

Result: `all_recovered=true` and the job passes, although the timeout was never retried.

Remove the Errors section and the result is "Missing, empty, malformed or incomplete lychee report", not a link failure.

**Workaround.** Raise lychee's `timeout`/`max_retries` and inspect the uploaded report manually. Add a `schedule:` trigger to `links.yml`.

**Suggested fix.** Port link-cli's fixes:
- `extractFailureSections` and the updated `validateLycheeReport` in `.github/scripts/check-web-archive.mjs`. They parse both Errors and Timeouts and count both summary rows.
- Use them from `.github/scripts/recheck-broken-links.mjs`.
- Treat only exit code 2 as recoverable (`if (failed && exitCode !== '2') throw`).
- Add `schedule: - cron: '41 5 * * 1'` to `.github/workflows/links.yml`.

The changes are in https://github.com/link-foundation/link-cli/pull/174.

---

## 3. The CodeQL gate prints SARIF text unquoted, stops at the first malformed item, and lets diagnostics hide findings

**Summary.** SARIF `message.text` comes from analysed code and query output. It is copied verbatim into the error text, so a message containing a newline followed by `::` is read by the runner as a workflow command. Separately:
- a malformed notification or result throws immediately, so the remaining problems are never listed;
- when diagnostics exist, the script throws before reporting findings, so a run with both shows only the diagnostics.

**Evidence**
- `scripts/check-codeql-results.mjs:32-34` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/scripts/check-codeql-results.mjs#L32))
  ```js
  failures.push(`${context}, ${field}: ${notification.descriptor?.id ?? notification.descriptor?.index ?? '?'} (${level}) ` +
    `${location?.artifactLocation?.uri ?? '?'}:${location?.region?.startLine ?? '?'}: ` +
    `${notification.message.text ?? notification.message.markdown}`);
  ```
- `scripts/check-codeql-results.mjs:67-69` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/scripts/check-codeql-results.mjs#L67)): findings use the same unquoted `${result.message.text ?? result.message.markdown}`.
- `scripts/check-codeql-results.mjs:22,28,63` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/scripts/check-codeql-results.mjs#L22)): `throw new Error(\`Malformed SARIF ...\`)` on the first bad item.
- `scripts/check-codeql-results.mjs:73-74` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/scripts/check-codeql-results.mjs#L73))
  ```js
  if (diagnostics.length) throw new Error(`${diagnostics.length} incomplete CodeQL analysis diagnostic(s):\n${diagnostics.join('\n')}`);
  if (findings.length) throw new Error(`${findings.length} unresolved CodeQL finding(s):\n${findings.join('\n')}`);
  ```
- The script is invoked from `.github/workflows/security.yml:93`.

**MRE.** Run the script on a SARIF directory containing:
- one result with `"message": {"text": "x\n::error title=spoof::fake"}`;
- one notification at level `error`.

The step log shows only the diagnostic, and the finding is never reported. With the diagnostic removed, the log contains a forged `::error title=spoof::` annotation.

**Workaround.** Read the uploaded SARIF artifacts directly rather than relying on the step log.

**Suggested fix.** Port link-cli's `.github/scripts/check-codeql-results.mjs`:
- quote every untrusted value with `const quote = (v) => JSON.stringify(String(v ?? '?'))`;
- collect every malformed item, diagnostic and finding;
- report them all before failing.

See https://github.com/link-foundation/link-cli/pull/174.

---

## 4. Pull-request runs save caches that evict main's caches

**Summary.** Every Cargo job uses `actions/cache@v6`, which restores and saves. When the job runs on `pull_request`, the saved entry is scoped to the PR merge ref. Main cannot read it, but it still counts toward the repository's 10 GB LRU budget. Each PR therefore saves a full `target/` and pushes main's reusable entries out. `fresh-merge` runs only on pull requests, so every entry it saves is unusable by main. `docker-build` exports `mode=max` layers to the GHA cache from PRs as well.

**Evidence**
- `.github/workflows/release.yml:288` (`fresh-merge`, `github.event_name == 'pull_request'` only) and L305-311 ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/.github/workflows/release.yml#L305))
  ```yaml
  - name: Cache cargo registry
    uses: actions/cache@v6
    with:
      path: |
        ~/.cargo/registry
        ~/.cargo/git
        target
      key: ${{ runner.os }}-cargo-fresh-merge-${{ hashFiles('**/Cargo.lock') }}
  ```
- The same restore-and-save `actions/cache@v6` appears at `release.yml` L460, L557, L568, L631, L682 and L747.
- `.github/workflows/release.yml:374` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/.github/workflows/release.yml#L374)): `cache-to: type=gha,mode=max,scope=docker-image`. The `docker-build` job runs on PRs too.
- `.github/workflows/links.yml:68` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/.github/workflows/links.yml#L68)): a cache keyed on `github.run_id`, so every run saves a new entry.

**MRE.** Open a few PRs that each change `Cargo.lock`, then list caches with `gh cache list --sort created_at`. Entries with `ref=refs/pull/N/merge` grow, and the `refs/heads/main` entries disappear. The next main run is a cold build.

**Workaround.** Delete PR-scoped caches periodically with `gh cache delete`.

**Suggested fix.**
- Restore with `actions/cache/restore` everywhere.
- Save with `actions/cache/save` only when `github.event_name == 'push' && github.ref == 'refs/heads/main' && steps.<restore>.outputs.cache-hit != 'true'`.
- Drop `cache-to` (or use `mode=min`) on PR Docker builds.

See link-cli `.github/workflows/rust.yml`, `crash.yml`, `docs.yml`, `js-client.yml` and `wasm.yml` in https://github.com/link-foundation/link-cli/pull/174.

---

## 5. The file-size limit covers only `.rs` and `.md`, so the JavaScript CI helpers are never checked

**Summary.** The size check has a 1000-line limit with a 900-line warning, but only for `.rs` and `.md` files. The repository's CI logic is largely `.mjs`. Those files have no limit and no early warning, so they can grow without bound.

**Evidence**
- `scripts/check-file-size.rs:19-26` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/scripts/check-file-size.rs#L19))
  ```rust
  const MAX_LINES: usize = 1000;
  const WARN_LINES: usize = 900;
  ...
  const FILE_EXTENSIONS: &[&str] = &[".rs", ".md"];
  ```
- Unchecked today: `scripts/check-web-archive.mjs` (440 lines), `scripts/recheck-broken-links.test.mjs` (510 lines).

**MRE.** Append 1200 lines to any `scripts/*.mjs` file and run `rust-script scripts/check-file-size.rs`. It passes with no warning.

**Workaround.** Review helper size by hand.

**Suggested fix.** Add `.mjs`/`.js` to the checked extensions with the same 1000/900 band. link-cli did this for JavaScript in `.github/scripts/check-javascript-quality.mjs` (a 900-line warning band), in https://github.com/link-foundation/link-cli/pull/174.

---

## 10. Release synchronization rejects any concurrent release commit from another language in a monorepo

**Summary.** The README documents a multi-language monorepo layout (`rust/Cargo.toml`). When `version-and-commit` finds that main moved, `source_guard::verify` checks the new commits:
- any changed path with a component starting with `.` (for example `.changeset/x.md`) or under `scripts`, `docs`, `tests` and similar directories is rejected;
- any other file not on the list is rejected as an "Untested source/dependency change";
- apart from `CHANGELOG.md` edits and `changelog.d/*.md` deletions, the only files allowed to change are `Cargo.toml`, `Cargo.lock`, `package.json` and `package-lock.json`, and only in versions the guard counts as equivalent;
- the whole repository tree is compared, not just the Rust package root.

Suppose a sibling-language release commit lands between validation and the Rust push, for example:
- a JS release deletes `.changeset/*.md`;
- a C# release bumps `<Version>` in a `.csproj`;
- a Python release bumps `pyproject.toml`.

The Rust release then exits 1 with "Let the newer commit's own CI run release it". That newer commit was pushed with `GITHUB_TOKEN`, which starts no workflow, so no run ever releases the Rust change. The release stays lost until someone pushes again.

**Evidence**
- `scripts/version-and-commit/source-guard.rs:222-237` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/scripts/version-and-commit/source-guard.rs#L222))
  ```rust
  if file.components().any(|part| {
      let name = part.as_os_str().to_string_lossy();
      name.starts_with('.')
          || matches!(name.as_ref(), "tests" | "fixtures" | "examples" | "experiments" | "docs" | "scripts" | "vendor" | "node_modules")
  }) {
      return Err(format!("Path outside release metadata allowlist: {path}"));
  }
  ```
- `source-guard.rs:199` `_ => Ok(false),` and L256 `return Err(format!("Untested source/dependency change: {path}"));`
- `source-guard.rs:261-265` ([permalink](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/scripts/version-and-commit/source-guard.rs#L261)): `"... Let the newer commit's own CI run release it."`
- The guard is called from `scripts/version-and-commit.rs:566`, `:578`, `:767` and `:769`. The release commits are pushed with `token: ${{ secrets.GITHUB_TOKEN }}` at `.github/workflows/release.yml:806`.
- The monorepo layout is documented at `README.md:221`.

**MRE.** In a monorepo with `rust/` and `js/`, merge two PRs back to back, one releasing each language. The JS release job pushes `chore: release` and deletes `.changeset/foo.md`. The Rust release job then fetches, sees main moved, and fails with `Path outside release metadata allowlist: .changeset/foo.md`. No workflow runs for the JS release commit, so the Rust bump is never published.

**Workaround.** Re-run the Rust release manually after the sibling release lands, or push an empty commit.

**Suggested fix.** Treat commits whose changed paths all lie outside the Rust package root (and outside shared CI files) as unrelated to the validated Rust source. Alternatively, when the guard rejects, dispatch the workflow on the new head with a PAT or `workflow_dispatch` so the release is not lost. There is no link-cli counterpart; this follows from the guard introduced for closed issue #231.

---

## 13. Pull-request change detection diffs only the newest commit, so a docs-only follow-up commit skips every Rust check

**Where.** [`scripts/detect-code-changes.rs#L116-L131`](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/scripts/detect-code-changes.rs#L116-L131) compares `HEAD^2^` with `HEAD^2` on `pull_request` events: the last commit of the PR head, not the whole PR.

**Why it is a false negative.** The run for the previous head is not guaranteed to have passed, or even to have finished. Every job in `release.yml` uses `cancel-in-progress: true` (for example [L68](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/blob/88fd935f69e303ac20f02d414bf92ab32d99a104/.github/workflows/release.yml#L68)), so pushing a second commit cancels the run that was testing the first one. If the second commit only touches Markdown, the new run reports `rs-changed=false`, skips lint, tests and build, and the PR shows green. The Rust change from the first commit was never tested.

**Already reported.** This is the open template issue [#235](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/issues/235), so no new issue was filed; the link-cli evidence below was added to it as [a comment](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/issues/235#issuecomment-6779540622). The per-commit diff was introduced on purpose by closed issue [#34](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template/issues/34), to avoid running every job for a docs-only follow-up commit. Any fix has to keep that trade-off in view.

**Observed in link-cli.** PR #174 changes many Rust files. Its run for head `067cb2c3` (Rust CI/CD Pipeline run 42898138563) succeeded with Rust lint and test **skipped**, because that commit changed only a Python helper.

**Reproduction** (needs only git):

```sh
git init -q r && cd r && git commit -q --allow-empty -m base && git branch -M main
git switch -q -c pr && mkdir src && echo 'fn a(){}' > src/lib.rs && git add . && git commit -qm code
echo docs > README.md && git add . && git commit -qm docs
git switch -q main && git merge -q --no-ff --no-edit pr     # the synthetic merge GitHub checks out
git diff --name-only HEAD^2^ HEAD^2   # template: README.md  -> rs-changed=false
git diff --name-only HEAD^1 HEAD      # whole PR: README.md src/lib.rs
```

**Workaround.** Re-run the PR workflow after touching a Rust file, or push an empty commit after the earlier run has finished successfully.

**Suggested fix.** On `pull_request` merge commits, diff the base parent against the merge commit (`git diff --name-only --no-renames HEAD^1 HEAD`), which is exactly what the PR changes. link-cli did this in [`rust/scripts/detect-code-changes.rs`](https://github.com/link-foundation/link-cli/blob/38ef331a/rust/scripts/detect-code-changes.rs), with the regression test `main_merge_and_pr_both_check_every_change_of_the_branch`, which fails on the old per-commit diff. If the per-commit optimisation should stay, follow the JS template: it uses the push range only when the previous head has a successful run, and otherwise diffs the full PR (`getPullRequestDiffArgs` in [`scripts/detect-code-changes.mjs#L236-L263`](https://github.com/link-foundation/js-ai-driven-development-pipeline-template/blob/34415b00490262a8582028099139fe913e7e0cb3/scripts/detect-code-changes.mjs#L236-L263)).

---

## Checked and not affected
- **2:** compressed evidence is scanned. `.github/scripts/check-secrets.mjs` gunzips tracked `.gz` files. Minor note: `.tgz`, `.zip` and `.xz` files would be passed through raw, but none are tracked.
- **6:** the rerun workflow list is complete.
- **12:** lint, test and build do not depend on the release preflight.
- **7, 8, 9, 11:** not applicable.
