# CI gates: credential preflight skips lint/test/build, NuGet 403 passes preflight, lychee timeouts ignored, Security not rerun, plus smaller gate gaps

Repository: `link-foundation/csharp-ai-driven-development-pipeline-template`
Commit inspected: `1f3b92f71c22717102a21cc5576a514cf11e674f`

Items 1, 3, 5, 6, 7 and 12 were found and fixed downstream in link-foundation/link-cli (https://github.com/link-foundation/link-cli/pull/174). Item 11 has no link-cli counterpart. No open duplicate exists. Related closed issues: #47 and #96 (rerun), #51 (preflight before build), #66 and #80 (NuGet auth), #74, #82.

---

## 12. A failed release-credential preflight skips lint, test and build on main

**Summary.** `lint` and `test` both `need` `release-preflight` and require `needs.release-preflight.result == 'success'`. `build` requires lint and test to succeed. So on main, an expired or under-scoped NuGet key means no code check runs at all, and the commit is never verified. Only `release` and `instant-release` should depend on the preflight.

**Evidence**
- `.github/workflows/release.yml:129-135` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/.github/workflows/release.yml#L129))
  ```yaml
  needs: [detect-changes, release-preflight]
  ...
  if: |
    always() && !cancelled() && needs.release-preflight.result == 'success' && (
  ```
- `.github/workflows/release.yml:190` and L200-201 ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/.github/workflows/release.yml#L190)): the `test` job has the same `needs` and the same `if`.
- `.github/workflows/release.yml:300-301` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/.github/workflows/release.yml#L300)): `build` has `needs: [lint, test]` and `if: always() && needs.lint.result == 'success' && needs.test.result == 'success'`.
- `release` (L403) and `instant-release` (L625) already list `release-preflight` in `needs` themselves.

**MRE.** Set `NUGET_API_KEY` to a revoked key and push to main. `Release Preflight` fails, and `Lint and Format Check`, `Test (*)` and `Build Package` are all skipped.

**Workaround.** Fix the secret and re-run; there is no way to get test results for that commit meanwhile.

**Suggested fix.**
- `lint`: `needs: [detect-changes]`, with no preflight in its `if`.
- `test`: needs `[detect-changes, lint]`.
- `release` and `instant-release`: keep `needs: [release-preflight, lint, test, build]` with `needs.release-preflight.result == 'success'`.
- Pipeline Status keeps reporting the preflight.

See link-cli commit "run lint and tests on main even when release credentials fail" (`.github/workflows/csharp.yml`, `.github/workflows/rust.yml`, plus a regression test in `.github/scripts/workflow-policy.test.mjs`) in https://github.com/link-foundation/link-cli/pull/174.

---

## 7. A NuGet key without push scope passes the preflight

**Summary.** The fallback probe POSTs to `create-verification-key`. NuGetGallery guards that route with `[ApiScopeRequired(PackagePush, PackagePushVersion)]`, so:
- 401 means the key is not accepted;
- 403 means the key authenticates but cannot push.

The script maps 403 to `unknown` and blames a "PackageVerify scope". The preflight then marks the check as not required, because `required: keyCheck.status !== 'unknown' || ...`. The release proceeds and fails only at `dotnet nuget push`.

**Evidence**
- `scripts/nuget-auth.mjs:73-79` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/scripts/nuget-auth.mjs#L73))
  ```js
  if (!response.ok) {
    await response.body?.cancel?.();
    return {
      ...result,
      status: response.status === 401 ? 'failed' : 'unknown',
      detail: `${selected.reason} Fallback verification HTTP ${response.status}. This probe may require PackageVerify scope; unsupported verification is not evidence of push rejection. ${NUGET_AUTH_HINT}`,
  ```
- `scripts/preflight-credentials.mjs:304-305` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/scripts/preflight-credentials.mjs#L304))
  ```js
  name: 'NuGet authentication',
  required: keyCheck.status !== 'unknown' || keyCheck.auth === 'none',
  ```
- `scripts/preflight-credentials.mjs:203` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/scripts/preflight-credentials.mjs#L203)): only `required` checks with a non-`ok` status fail the release.

**MRE.** Create a nuget.org API key whose only scope is "Unlist" for the package glob, and store it as `NUGET_API_KEY`. The preflight reports `unknown` and passes. The publish step later fails with a 403 from push.

**Workaround.** Verify key scopes on nuget.org manually whenever the key is rotated.

**Suggested fix.**
- Map 401 to `{status: 'failed', authenticated: false}`, with the message "NUGET_API_KEY is not accepted (invalid, expired or revoked)".
- Map 403 to `{status: 'failed', authenticated: true, authorized: false}`, with the message "authenticates but has no push scope".
- Also log the `x-nuget-warning` response header.

See `describeVerificationDenial` in link-cli `.github/scripts/nuget-auth.mjs`, https://github.com/link-foundation/link-cli/pull/174.

---

## 1. The link check ignores lychee's "Timeouts per input" section and does not distinguish exit codes 1 and 3 from 2

**Summary.** The recheck reads only the `Errors per input` section. With a recoverable error plus a timeout, every listed error recovers and `all_recovered=true`, so the timeout passes silently. A report that contains only timeouts gives an empty section. The recheck receives no exit code, so runtime and config failures (exit 1 or 3) go through the same "recover" path. There is no `schedule:`.

**Evidence**
- `scripts/check-web-archive.mjs:64-69` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/scripts/check-web-archive.mjs#L64))
  ```js
  function extractErrorsSection(content) {
    const lines = content.split('\n');
    const start = lines.findIndex((line) => /^#+\s+Errors per input\s*$/.test(line));
    if (start === -1) {
      return '';
    }
  ```
- `scripts/recheck-broken-links.mjs:84` `const section = extractErrorsSection(content);` and L509 `setOutput('all_recovered', remaining === 0 ? 'true' : 'false');` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/scripts/recheck-broken-links.mjs#L84))
- `.github/workflows/links.yml:91-96` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/.github/workflows/links.yml#L91)): `if: steps.lychee.outputs.exit_code != 0`. The env holds only `LYCHEE_OUTPUT` and `RECOVERED_OUTPUT`, with no exit code. The triggers (L3-25) are push, pull_request and workflow_dispatch, with no schedule.

**MRE.** Give the recheck a report with one recoverable `[503]` under `## Errors per input` and one `[TIMEOUT]` under `## Timeouts per input`. The output is `all_recovered=true`, and the job is green.

**Workaround.** Inspect the lychee report artifact manually, and add a cron trigger.

**Suggested fix.** Port link-cli's `extractFailureSections`/`validateLycheeReport` (`.github/scripts/check-web-archive.mjs`, `.github/scripts/recheck-broken-links.mjs`), pass `LYCHEE_EXIT_CODE` and fail on any exit code other than 2. Add `schedule:` to `links.yml`. See https://github.com/link-foundation/link-cli/pull/174.

---

## 6. A Security workflow failure is never retried by the infrastructure rerun workflow

**Summary.** `rerun-infrastructure-failures.yml` listens only to `CI/CD Pipeline`. `Security` ends in the terminal `Security Status` gate (`if: always()`), so an unacquired or cancelled runner there leaves a red required check that nothing retries.

**Evidence**
- `.github/workflows/rerun-infrastructure-failures.yml:20-21` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/.github/workflows/rerun-infrastructure-failures.yml#L20))
  ```yaml
  workflow_run:
    workflows: ['CI/CD Pipeline']
  ```
- `.github/workflows/security.yml:187-190` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/.github/workflows/security.yml#L187)): `security-status:` / `name: Security Status` / `needs: [codeql, audit, helper-checks, artifact-upload, artifact-download, dependency-review]` / `if: always()`.

**MRE.** A Security run whose `codeql` job is never acquired by a runner fails `Security Status`. No rerun is attempted, whereas the same failure in `CI/CD Pipeline` is retried.

**Workaround.** Re-run failed jobs manually.

**Suggested fix.** Add `'Security'` (and any other workflow with a terminal gate) to `workflow_run.workflows`. link-cli made the same change for its crash-test workflow in `.github/workflows/rerun-infrastructure-failures.yml`, https://github.com/link-foundation/link-cli/pull/174.

---

## 3. The CodeQL gate stops at the first SARIF problem and never lists findings

**Summary.** `validateSarif` throws a fixed message as soon as a run has any result, and files are validated one at a time. The log never names a rule, file or line, and later files are not examined. Maintainers must download SARIF artifacts to learn what failed. The script does not echo SARIF text, so there is no workflow-command injection here.

**Evidence**
- `scripts/security-gates.mjs:15-16` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/scripts/security-gates.mjs#L15))
  ```js
  if (array(run.results, 'SARIF results').length)
    throw Error('CodeQL findings detected (all levels are fatal)');
  ```
- `scripts/security-gates.mjs:125` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/scripts/security-gates.mjs#L125)): `for (const file of files) validateSarif(read(join(directory, file)));`
- The script is invoked at `.github/workflows/security.yml:47`.

**MRE.** Introduce two CodeQL findings in two languages. The step prints only `CodeQL findings detected (all levels are fatal)`.

**Workaround.** Download the `sarif` artifact and inspect it.

**Suggested fix.** Collect every malformed item, diagnostic and finding across all files. Print each one with its rule, location and message, quoting untrusted values with `JSON.stringify`. Fail once at the end. See link-cli `.github/scripts/check-codeql-results.mjs`, https://github.com/link-foundation/link-cli/pull/174.

---

## 5. The file-size limit covers only `.cs`; the JavaScript CI helpers are unchecked

**Summary.** The 1000-line limit with its 900-line warning applies only to `.cs` files. ESLint has no `max-lines` rule, so the `.mjs` CI helpers have no bound.

**Evidence**
- `scripts/check-file-size.mjs:12-14` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/scripts/check-file-size.mjs#L12))
  ```js
  export const MAX_LINES = 1000;
  export const WARN_LINES = 900;
  export const FILE_EXTENSIONS = ['.cs'];
  ```
- Unchecked today: `scripts/recheck-broken-links.mjs` (535 lines), `scripts/recheck-broken-links.test.mjs` (564 lines).

**MRE.** Grow any `scripts/*.mjs` past 1000 lines. `node scripts/check-file-size.mjs` passes.

**Workaround.** Review helper size by hand.

**Suggested fix.** Include `.mjs`/`.js` (and `.md` with a doc budget) in the checked extensions. See link-cli `csharp/scripts/check-file-size.mjs` (C# warning band) and `.github/scripts/check-javascript-quality.mjs` (JS 900-line band), https://github.com/link-foundation/link-cli/pull/174.

---

## 11. 22 SHA pins carry major-only version comments

**Summary.** zizmor enforces `hash-pin`, and `isShaPinned` accepts any non-empty comment. Twenty-two pins are annotated only with a major version (`# v7`), while 36 others carry the exact tag. A major-only comment does not identify the pinned release. Several major tags have since moved, so the comment no longer matches what `vN` resolves to, and Dependabot cannot rewrite the comment precisely.

**Evidence**
- `scripts/action-pins.mjs:10` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/scripts/action-pins.mjs#L10)): `const USES = /^\s*-?\s*uses:\s*([\w.-]+\/[\w./-]+)@(\S+)(?:\s+#\s*(\S+))?/gm;`. Lines 37-42 accept `Boolean(ref.comment)`.
- `.github/workflows/security.yml:50` ([permalink](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/1f3b92f71c22717102a21cc5576a514cf11e674f/.github/workflows/security.yml#L50)): `uses: actions/upload-artifact@cf430e030ddbb5b0abf93d22962f4752f3646cd9 # v7`. That SHA is v7.0.2.
- Other occurrences:
  - `security.yml` L68, 88, 102, 117, 139, 159, 181
  - `docs.yml` L87, 93, 118
  - `release.yml` L68, 103, 154, 267, 334, 365, 434, 666, 822, 851
  - `links.yml` L57
- Exact tags for the pinned SHAs:

  | Action | Comment | Exact tag |
  |---|---|---|
  | upload-artifact | `# v7` | v7.0.2 |
  | download-artifact | `# v8` | v8.0.2 |
  | dependency-review-action | `# v5` | v5.0.0 (v5 has moved) |
  | configure-pages | `# v6` | v6.0.0 |
  | upload-pages-artifact | `# v5` | v5.0.0 |
  | deploy-pages | `# v5` | v5.0.1 |
  | codecov-action | `# v7` | v7.1.1 |
  | lychee-action | `# v2` | v2.9.0 |
  | setup-bun | `# v2` | v2.2.0 |
  | create-pull-request | `# v8` | v8.1.1 (v8 has moved) |

**MRE.** Run `grep -nE '@[0-9a-f]{40} # v[0-9]+$' .github/workflows/*.yml`. It lists 22 lines.

**Workaround.** None needed for safety; this affects auditability.

**Suggested fix.** Annotate every pin with the exact tag (`# v7.0.2`), and make `isShaPinned` require `/^v\d+\.\d+\.\d+$/` for the comment. There is no link-cli counterpart.

---

## Checked and not affected
- **4:** no `actions/cache` or GHA Docker cache is used.
- **2, 8, 9, 10:** not applicable; for 2, no secrets scanner is configured.
