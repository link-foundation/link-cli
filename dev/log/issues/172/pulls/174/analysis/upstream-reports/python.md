# CI gates: the release push's rebase retry is unreachable, PR-only jobs write caches that evict main's, non-Python helpers skip the size check, and lychee is verified only against its own release

Repository: `link-foundation/python-ai-driven-development-pipeline-template`
Commit inspected: `f28c9d836cd18426adbc2f17629ac58202e04a83`

Items 4 and 5 were found and fixed downstream in link-foundation/link-cli (https://github.com/link-foundation/link-cli/pull/174). Item 8 has no link-cli counterpart. No open duplicate exists. Related closed issues: #73 (rebase-retry), #131, #66, #74, #122, #141.

---

## 8a. `push_branch_with_rebase_retry` never rebases, so a lost push race fails the release

**Summary.** The docstring promises to retry "a lost race with a rebase (issue #73)". After a non-fast-forward rejection, the function:
1. fetches;
2. calls `attest_release(validated_sha, origin/<branch>)`, which raises unless the remote head *is* the validated commit or a release commit whose sole parent is it;
3. if the remote moved to an equivalent release, resets to it, or raises if the version differs.

The `git pull --rebase` block runs only when the remote still equals `validated_sha`. In that state the original push was a fast-forward and could not have been rejected as non-fast-forward, so the rebase path is unreachable.

A real race fails the release job with "Release must retain the original validated commit as its sole parent; rerun CI". A real race here means another PR merged into main between validation and the push. The newer commit's own run normally picks the change up. But if that commit was pushed with `GITHUB_TOKEN`, no run starts, and the release is lost until the next human push.

**Evidence**
- `scripts/version_and_commit.py:167` ([permalink](https://github.com/link-foundation/python-ai-driven-development-pipeline-template/blob/f28c9d836cd18426adbc2f17629ac58202e04a83/scripts/version_and_commit.py#L167)): `"""Push to a branch, retrying a lost race with a rebase (issue #73).`
- `scripts/version_and_commit.py:227-249` ([permalink](https://github.com/link-foundation/python-ai-driven-development-pipeline-template/blob/f28c9d836cd18426adbc2f17629ac58202e04a83/scripts/version_and_commit.py#L227))
  ```python
  run_command(["git", "fetch", "origin", branch])
  remote = f"origin/{branch}"
  attest_release(validated_sha, remote, pyproject_git_path)
  if git_text(["rev-parse", remote]) != validated_sha:
      ...
      run_command(["git", "reset", "--hard", remote])
      return
  ...
  rebase = run_git_capturing(["git", "pull", "--rebase", "origin", branch])
  ```
- `scripts/version_and_commit.py:339-343` ([permalink](https://github.com/link-foundation/python-ai-driven-development-pipeline-template/blob/f28c9d836cd18426adbc2f17629ac58202e04a83/scripts/version_and_commit.py#L339))
  ```python
  parents = git_text(["show", "-s", "--format=%P", head]).split()
  if parents != [validated]:
      raise ValueError(
          "Release must retain the original validated commit as its sole parent; rerun CI"
      )
  ```
- `main()` turns this into `Error: ...` and returns 1 (L575-577).

**MRE.**
1. In a scratch clone, commit `A` (validated) and a release commit `R` on top of it.
2. On the remote, push an unrelated commit `B` on top of `A`.
3. Call `push_branch_with_rebase_retry("main", validated_sha=A)`.

The first push is rejected (non-fast-forward). Then `attest_release(A, origin/main)` raises because `B`'s parent is `A` but `B` is not a release commit with only metadata changes. No rebase is attempted.

**Workaround.** Re-run the release workflow on the new head.

**Suggested fix.** Choose one:
- Remove the dead rebase block and the docstring claim, and on a lost race dispatch the workflow on the new head so the release is not dropped.
- Implement the intended rebase: rebase `R` onto the new remote head and re-attest that only `pyproject.toml`'s version and changelog files differ from the new head.

There is no link-cli counterpart.

---

## 8b. The lychee binary is checked only against a checksum from the same release (hardening)

**Summary.** `install-lychee.sh` downloads the archive and its `.sha256` from the same GitHub release. This detects corruption but not a replaced asset. The version is pinned, but its digest is not.

**Evidence**
- `scripts/install-lychee.sh:42` and L47 ([permalink](https://github.com/link-foundation/python-ai-driven-development-pipeline-template/blob/f28c9d836cd18426adbc2f17629ac58202e04a83/scripts/install-lychee.sh#L42))
  ```bash
  if ! fetch "${base_url}/${asset}" "$work/$asset" || ! fetch "${base_url}/${asset}.sha256" "$work/$asset.sha256"; then
  ...
  expected="$(awk '{print $1; exit}' "$work/$asset.sha256")"
  ```
- `.github/workflows/links.yml:77`: `LYCHEE_VERSION: v0.24.2`, with no pinned digest.

**MRE.** Point `LYCHEE_DOWNLOAD_BASE_URL` at a server that returns a modified archive and its matching `.sha256`. The install succeeds.

**Workaround.** None.

**Suggested fix.** Commit per-architecture SHA-256 values next to `LYCHEE_VERSION` and compare against them. There is no link-cli counterpart; link-cli uses `lycheeverse/lychee-action`.

---

## 4. PR-only jobs write caches that main can never read (minor)

**Summary.** Caches saved from `pull_request` runs are scoped to the merge ref. Main cannot read them, but they count toward the 10 GB LRU and evict main's entries. Two jobs save such caches:
- the PR-only Docker check exports `mode=max` layers;
- the link check saves a new per-commit lychee cache on every PR push.

**Evidence**
- `.github/workflows/release.yml:506` (`if: github.event_name == 'pull_request' && ...`) and L539 ([permalink](https://github.com/link-foundation/python-ai-driven-development-pipeline-template/blob/f28c9d836cd18426adbc2f17629ac58202e04a83/.github/workflows/release.yml#L539)): `cache-to: type=gha,mode=max,scope=docker-pr-check`
- `.github/workflows/links.yml:68-71` ([permalink](https://github.com/link-foundation/python-ai-driven-development-pipeline-template/blob/f28c9d836cd18426adbc2f17629ac58202e04a83/.github/workflows/links.yml#L68)), in a workflow triggered by `pull_request` (L17):
  ```yaml
  uses: actions/cache@v5
  with:
    path: .lycheecache
    key: cache-lychee-${{ steps.link-cache.outputs.day }}-${{ github.sha }}
  ```

**MRE.** Push several commits to a PR. `gh cache list` shows accumulating `refs/pull/N/merge` entries, and `refs/heads/main` entries are evicted.

**Workaround.** Prune PR caches with `gh cache delete`.

**Suggested fix.**
- Restore with `actions/cache/restore`, and save with `actions/cache/save` only on `push` to `refs/heads/main` without an exact hit.
- Drop `cache-to` on the PR-only Docker build, or use `mode=min`.

See link-cli `.github/workflows/rust.yml`, `crash.yml`, `docs.yml`, `js-client.yml` and `wasm.yml`, https://github.com/link-foundation/link-cli/pull/174.

---

## 5. The file-size limit covers only `.py` and `.md`; shell and JavaScript helpers are unchecked

**Summary.** The 1000/900 band applies to `.py` and the 2500/2250 band to `.md`. Shell and `.mjs` CI helpers are never measured.

**Evidence**
- `scripts/check_file_size.py:20-23` ([permalink](https://github.com/link-foundation/python-ai-driven-development-pipeline-template/blob/f28c9d836cd18426adbc2f17629ac58202e04a83/scripts/check_file_size.py#L20))
  ```python
  LIMITS = {
      ".py": (MAX_LINES, WARN_LINES),
      ".md": (MAX_DOC_LINES, WARN_DOC_LINES),
  }
  ```
- L92: `if path.is_file() and path.suffix in LIMITS:`
- Unchecked today: `scripts/preflight-credentials.sh` (347 lines), `tests/deploy-pages.test.mjs` (315 lines).

**MRE.** Grow `scripts/preflight-credentials.sh` past 1000 lines. `python scripts/check_file_size.py` passes.

**Workaround.** Review sizes by hand.

**Suggested fix.** Add `.sh`, `.mjs` and `.js` to `LIMITS` with the 1000/900 band. See link-cli `.github/scripts/check-javascript-quality.mjs` (900-line warning band), https://github.com/link-foundation/link-cli/pull/174.

---

## Checked and not affected
- **1:** `scripts/check-links.sh` treats only exit codes 0 and 2 as expected, and `links.yml` has a schedule.
- **3:** not affected.
- **12:** lint and test do not depend on the release preflight.
- **2 (latent):** the secrets step is `secretlint "**/*"` (`.github/workflows/release.yml:200`), which does not decompress `.gz` files. No compressed files are tracked today. If gzip evidence is ever committed, adopt the gunzip-and-stdin scan from link-cli `.github/scripts/check-secrets.mjs`.
- **6:** there is no rerun-infrastructure workflow, and all jobs run on Ubuntu runners. Consider one if unacquired-runner failures appear.
- **7, 9, 10, 11:** not applicable.
