# CI gates: compressed logs skip the secrets scan, CodeQL messages can inject workflow commands, PR runs write caches that evict main's, and lychee is verified only against its own release

Repository: `link-foundation/js-ai-driven-development-pipeline-template`
Commit inspected: `34415b00490262a8582028099139fe913e7e0cb3`

Items 2, 3 and 4 were found and fixed downstream in link-foundation/link-cli (https://github.com/link-foundation/link-cli/pull/174). Item 8 is a hardening suggestion with no link-cli counterpart. No open duplicate exists. Related closed issues: #278 (schedule/cache), #286, #266, #138, #184, #165, #176, #280, #291.

---

## 2. Tracked `.gz` evidence logs are never scanned for secrets

**Summary.** The secrets step runs `secretlint "**/*"`. secretlint 13.0.7 reads files as text and has no decompression, so gzip-compressed files are effectively skipped. The repository tracks eight compressed CI logs, which are exactly the kind of file that captures tokens.

**Evidence**
- `.github/workflows/release.yml:256-257` ([permalink](https://github.com/link-foundation/js-ai-driven-development-pipeline-template/blob/34415b00490262a8582028099139fe913e7e0cb3/.github/workflows/release.yml#L256))
  ```yaml
  - name: Check for secrets
    run: npx --yes -p secretlint@13.0.7 -p @secretlint/secretlint-rule-preset-recommend@13.0.7 secretlint "**/*"
  ```
- Unscanned files: `git ls-files '*.gz'` lists 8 files under `docs/case-studies/issue-212/validation/`, for example `security-37555257997.log.gz` and `checks-and-release-37557125805.log.gz`.

**MRE.**
1. Run `printf 'token=ghp_%s\n' "$(head -c 36 /dev/zero | tr '\0' A)" | gzip > leak.log.gz && git add leak.log.gz`.
2. Run the step command. It passes.
3. Run the same content uncompressed. secretlint reports the token.

**Workaround.** Store evidence uncompressed, or scan it manually with `zcat file.gz | npx secretlint --stdinFileName file`.

**Suggested fix.** Scan the `git ls-files` inventory:
- pipe each `.gz` through `gunzip` into `secretlint --stdinFileName <name-without-.gz>`;
- expand `.tar.gz`/`.tgz` entries the same way;
- bound the inflated size;
- fail closed on undecodable archives.

See link-cli `.github/scripts/check-secrets.mjs` (`isCompressed`, `isTarball`, `readTarEntries`), https://github.com/link-foundation/link-cli/pull/174.

---

## 3. SARIF messages are printed unquoted, so a finding can inject workflow commands

**Summary.** CodeQL notification and result messages contain text derived from the analysed code. They are concatenated into failure lines and written with `console.error`. A message that contains a newline followed by `::error ...`, `::add-mask::` or `::stop-commands::` becomes a runner command.

**Evidence**
- `scripts/check-codeql-results.mjs:64-65` ([permalink](https://github.com/link-foundation/js-ai-driven-development-pipeline-template/blob/34415b00490262a8582028099139fe913e7e0cb3/scripts/check-codeql-results.mjs#L64))
  ```js
  failures.push(
    `${file} run ${runIndex}: ${key} ${describeNotification(notification)} [${level}] ${message}`
  ```
- `scripts/check-codeql-results.mjs:94` ([permalink](https://github.com/link-foundation/js-ai-driven-development-pipeline-template/blob/34415b00490262a8582028099139fe913e7e0cb3/scripts/check-codeql-results.mjs#L94)): `` `${file} run ${index}: ${finding?.ruleId ?? 'unknown rule'} [...] ${finding?.message?.text ?? 'CodeQL finding'}` ``
- `scripts/check-codeql-results.mjs:130-131` ([permalink](https://github.com/link-foundation/js-ai-driven-development-pipeline-template/blob/34415b00490262a8582028099139fe913e7e0cb3/scripts/check-codeql-results.mjs#L130)): `for (const failure of failures) { log(failure); }`
- The script is invoked from `.github/workflows/security.yml:62`.

**MRE.** Run `node scripts/check-codeql-results.mjs dir/` on a SARIF file in `dir/` that contains a result with `"message": {"text": "x\n::error title=spoof::forged"}`. The job shows a forged `spoof` annotation.

**Workaround.** None in-workflow; inspect SARIF artifacts directly.

**Suggested fix.** Quote every untrusted field with `const quote = (v) => JSON.stringify(String(v ?? '?'))` before logging. See link-cli `.github/scripts/check-codeql-results.mjs`, https://github.com/link-foundation/link-cli/pull/174.

---

## 4. PR-only jobs write caches that main can never read (minor)

**Summary.** Caches saved from a `pull_request` run are scoped to the merge ref. Main cannot read them, but they still count toward the 10 GB LRU and evict main's entries. Two jobs save such caches:
- `docker-build` runs only on pull requests yet exports `mode=max` layers;
- the link check saves a new lychee cache keyed per commit on every PR push.

**Evidence**
- `.github/workflows/release.yml:416` (`if: github.event_name == 'pull_request' && ...`) and L455 ([permalink](https://github.com/link-foundation/js-ai-driven-development-pipeline-template/blob/34415b00490262a8582028099139fe913e7e0cb3/.github/workflows/release.yml#L455)): `cache-to: type=gha,mode=max,scope=docker-image`
- `.github/workflows/links.yml:85-88` ([permalink](https://github.com/link-foundation/js-ai-driven-development-pipeline-template/blob/34415b00490262a8582028099139fe913e7e0cb3/.github/workflows/links.yml#L85)), on a workflow triggered by `pull_request` (L24):
  ```yaml
  uses: actions/cache@v5
  with:
    path: .lycheecache
    key: cache-lychee-${{ runner.os }}-${{ steps.link-cache.outputs.day }}-${{ github.sha }}
  ```

**MRE.** Push several commits to a PR touching Docker or link inputs, then run `gh cache list`. New `refs/pull/N/merge` entries accumulate, and older `refs/heads/main` entries are evicted.

**Workaround.** Prune PR caches with `gh cache delete`.

**Suggested fix.**
- Use `actions/cache/restore` on PRs, and save with `actions/cache/save` only on `push` to `refs/heads/main` when there was no exact hit.
- Drop `cache-to` on the PR-only Docker build, or use `mode=min`.

See link-cli `.github/workflows/rust.yml`, `crash.yml`, `docs.yml`, `js-client.yml` and `wasm.yml`, https://github.com/link-foundation/link-cli/pull/174.

---

## 8. The lychee binary is checked only against a checksum from the same release (hardening)

**Summary.** `install-lychee.sh` downloads the archive and its `.sha256` from the same GitHub release and compares them. This catches transfer corruption but not a replaced release asset. The version is pinned in `links.yml`, but its checksum is not.

**Evidence**
- `scripts/install-lychee.sh:42` and L47 ([permalink](https://github.com/link-foundation/js-ai-driven-development-pipeline-template/blob/34415b00490262a8582028099139fe913e7e0cb3/scripts/install-lychee.sh#L42))
  ```bash
  if ! fetch "${base_url}/${asset}" "$work/$asset" || ! fetch "${base_url}/${asset}.sha256" "$work/$asset.sha256"; then
  ...
  expected="$(awk '{print $1; exit}' "$work/$asset.sha256")"
  ```
- `.github/workflows/links.yml:94`: `LYCHEE_VERSION: v0.24.2`, with no pinned digest.

**MRE.** Serve a modified archive and a matching `.sha256` from `LYCHEE_DOWNLOAD_BASE_URL`. The install succeeds.

**Workaround.** None.

**Suggested fix.** Commit the expected SHA-256 for each supported architecture next to `LYCHEE_VERSION` (for example `LYCHEE_SHA256_X86_64`) and compare against it. Use the release `.sha256` only as a cross-check. There is no link-cli counterpart; link-cli uses `lycheeverse/lychee-action`.

---

## Checked and not affected
- **1:** `scripts/lychee-report.mjs` handles timeouts; `links.yml` fails on exit codes other than 0 and 2 (L120) and has a schedule.
- **5:** `scripts/check-file-line-limits.sh` covers all source types.
- **6:** the rerun workflow list is complete.
- **12:** lint and test do not depend on the release preflight.
- **7, 9, 10, 11:** not applicable. Note: `scripts/setup-preview-gh.sh:19` uses `curl --retry 3` without `--retry-all-errors`, the same pattern as the Java template's item 9c.
