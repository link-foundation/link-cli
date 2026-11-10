# CI gates: the dependency-freshness check fails every PR during Dependabot's cooldown and forbids SHA pins; size check covers only .java with no warning band

Repository: `link-foundation/java-ai-driven-development-pipeline-template`
Commit inspected: `0d3a4e3e96d5e93d50a133b1f9eadd91241629cd`

Item 5 was found and fixed downstream in link-foundation/link-cli (https://github.com/link-foundation/link-cli/pull/174). Item 9 has no link-cli counterpart. No open duplicate exists. Related closed issues: #14 (freshness gate), #16.

---

## 9. The CI-dependency freshness check conflicts with Dependabot cooldown, forbids SHA pins, and has a non-retrying download

### 9a. An unrelated upstream release turns every pull request red for up to 7 days

**Summary.** `check-ci-dependencies.mjs` compares each pinned action and tool with the upstream `releases/latest`. Any difference fails the job on `pull_request`. Dependabot is configured with `cooldown: default-days: 7`, so it deliberately waits a week before proposing the bump. During that window every PR fails "Workflow Policy and Dependency Freshness", whatever it changes, and the only way out is a manual bump PR.

**Evidence**
- `scripts/check-ci-dependencies.mjs:109-110` ([permalink](https://github.com/link-foundation/java-ai-driven-development-pipeline-template/blob/0d3a4e3e96d5e93d50a133b1f9eadd91241629cd/scripts/check-ci-dependencies.mjs#L109))
  ```js
  if (version !== current) {
    problems.push({ kind: OUTDATED, message: `${path}: ${repo} ${version} is outdated; latest stable is ${current}` });
  ```
- `scripts/check-ci-dependencies.mjs:143` and L162 ([permalink](https://github.com/link-foundation/java-ai-driven-development-pipeline-template/blob/0d3a4e3e96d5e93d50a133b1f9eadd91241629cd/scripts/check-ci-dependencies.mjs#L143))
  ```js
  const gate = eventName === 'pull_request';
  ...
  const exitCode = broken.length || (gate && outdated.length) ? 1 : 0;
  ```
- `.github/workflows/workflows.yml:6` (`pull_request`) and L30-31 ([permalink](https://github.com/link-foundation/java-ai-driven-development-pipeline-template/blob/0d3a4e3e96d5e93d50a133b1f9eadd91241629cd/.github/workflows/workflows.yml#L30)): `run: node scripts/check-ci-dependencies.mjs`
- `.github/dependabot.yml:20-21` and L33-34 ([permalink](https://github.com/link-foundation/java-ai-driven-development-pipeline-template/blob/0d3a4e3e96d5e93d50a133b1f9eadd91241629cd/.github/dependabot.yml#L20))
  ```yaml
  cooldown:
    default-days: 7
  ```

**MRE.** The day after `actions/checkout` publishes v7.0.2, open any docs-only PR. The job fails with `actions/checkout v7.0.1 is outdated; latest stable is v7.0.2`. Dependabot proposes nothing until day 7.

**Workaround.** Hand-bump the pin in each affected PR, or re-run after Dependabot's PR merges.

**Suggested fix.** Choose one:
- Make "outdated" a warning on every event (keep "broken" fatal), and let the scheduled run plus Dependabot drive bumps.
- Only flag releases older than the cooldown (compare `published_at` with 7 days).
- Gate only PRs that touch `.github/workflows/**`.

### 9b. The checker rejects SHA-pinned actions

**Summary.** Only `owner/repo@vX.Y.Z` is accepted. Pinning to a full commit SHA, which GitHub recommends as the only immutable reference, makes the job throw. Adopting it, or enabling zizmor's `unpinned-uses` / `hash-pin` policy, is therefore impossible.

**Evidence**
- `scripts/check-ci-dependencies.mjs:41-42` ([permalink](https://github.com/link-foundation/java-ai-driven-development-pipeline-template/blob/0d3a4e3e96d5e93d50a133b1f9eadd91241629cd/scripts/check-ci-dependencies.mjs#L41))
  ```js
  const match = reference.match(/^([\w.-]+\/[\w.-]+)(?:\/[^@]+)?@(v\d+\.\d+\.\d+)$/);
  if (!match) throw new Error(`${name}: use an exact stable action version: ${reference}`);
  ```

**MRE.** Change `uses: actions/checkout@v7.0.1` to `uses: actions/checkout@<40-hex-sha> # v7.0.1`. The script exits with `use an exact stable action version`.

**Workaround.** None while the check is required.

**Suggested fix.** Also accept `@<40-hex>` with a trailing `# vX.Y.Z` comment. Take the version from the comment and verify that the tag resolves to that SHA (`gh api repos/<repo>/commits/<tag>`).

### 9c. The osv-scanner download does not retry transient network errors

**Summary.** `curl --retry 3` retries only timeouts and some HTTP codes. Connection resets, DNS failures and partial transfers fail the Security job at once.

**Evidence**
- `.github/workflows/security.yml:45` ([permalink](https://github.com/link-foundation/java-ai-driven-development-pipeline-template/blob/0d3a4e3e96d5e93d50a133b1f9eadd91241629cd/.github/workflows/security.yml#L45)): `curl -fsSL --retry 3 -o "$RUNNER_TEMP/osv-scanner/osv-scanner" \`

**MRE.** Drop the connection mid-transfer (for example via a proxy). curl exits 56 or 18 without retrying.

**Workaround.** Re-run the job.

**Suggested fix.** Add `--retry-all-errors --retry-delay 5 --connect-timeout 20`.

There is no link-cli counterpart for 9a-9c.

---

## 5. The file-size check covers only `.java` and has no warning band

**Summary.** The size check walks only `.java` files and fails at 1000 lines with no early warning. The `.mjs` CI and release scripts, which hold most of the pipeline logic, are never measured.

**Evidence**
- `scripts/check-file-size.mjs:16` ([permalink](https://github.com/link-foundation/java-ai-driven-development-pipeline-template/blob/0d3a4e3e96d5e93d50a133b1f9eadd91241629cd/scripts/check-file-size.mjs#L16)): `const MAX_LINES_DEFAULT = 1000;` There is no warning threshold anywhere in the file.
- `scripts/check-file-size.mjs:62` ([permalink](https://github.com/link-foundation/java-ai-driven-development-pipeline-template/blob/0d3a4e3e96d5e93d50a133b1f9eadd91241629cd/scripts/check-file-size.mjs#L62)): `} else if (extname(entry) === '.java') {`
- Unchecked today: `scripts/version-and-commit.mjs` (364 lines).

**MRE.** Grow `scripts/version-and-commit.mjs` past 1000 lines. The check passes. Grow a `.java` file to 999 lines. No warning is shown.

**Workaround.** Review sizes by hand.

**Suggested fix.**
- Add a 900-line warning band (`::warning` annotation).
- Include `.mjs`/`.js` files.

See link-cli `.github/scripts/check-javascript-quality.mjs` and `csharp/scripts/check-file-size.mjs` (900-line warning bands), https://github.com/link-foundation/link-cli/pull/174.

---

## Checked and not affected
- **4:** only `setup-java` `cache: maven` is used, keyed on `pom.xml`.
- **6:** `rerun-unacquired.yml` lists all three workflows.
- **1, 2, 3, 7, 8, 10, 11, 12:** not applicable; there is no links workflow, secrets scanner, CodeQL SARIF gate or release preflight.
