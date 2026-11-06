# Code scanning comparison remains neutral with pending configurations after successful uploads

Observed on public PR https://github.com/link-foundation/link-cli/pull/166 at head `5db14dddd56caa5e533d35aaf4a7361cd16e0706`, synthetic merge `d949114c51f757e602cd0d337fcf326d82a542e1`, on 2026-11-06 at 01:08 UTC. This may belong to the code-scanning backend rather than the action; please transfer or redirect if appropriate.

## Reproduction using the retained public run

1. View successful Security run https://github.com/link-foundation/link-cli/actions/runs/42140455675. All five CodeQL analyses and our independent SARIF completeness/finding gate passed. The workflow uses `github/codeql-action@v4`, explicit build mode `none`, and full databases (`CODEQL_OVERLAY_DATABASE_MODE: none`).
2. Inspect the GitHub Advanced Security check, which completed at 00:57:51 UTC, before the longer analyses finished:

   `gh api repos/link-foundation/link-cli/check-runs/127639504317`

   At 01:08 UTC it still reported `neutral`, title `4 configurations not found`, with pending/hourglass entries for C#, JavaScript and Rust in build mode `none`. It also names an obsolete C# `autobuild` category. The warning also existed on the initial, unchanged PR head (`a20e071`, check `127599283191`).
3. Compare the actual successful uploads using:

   `gh api repos/link-foundation/link-cli/code-scanning/analyses --paginate`

   These exact pending categories have analyses on the PR merge ref above: Rust `2108643387`, C# `2108634525`, JavaScript `2108630646`. Each has an empty `error` and `results_count: 0`. Python `2108629477` and Actions `2108629491` also succeeded. The Rust analysis was created at 01:00:53 UTC, after the neutral check completed.
4. Re-read the check after every workflow is complete. Its earlier pending summary remains unchanged. A supported REST check re-request attempt returned HTTP 404.

## Expected and actual behavior

The comparison should refresh as uploads finish, distinguish successful current categories from pending categories, and identify only genuinely missing baseline configurations. The obsolete C# autobuild category is a separate baseline-migration concern: it last ran on main at `4bfa2527471bf94ee1871650a6c88c99c3a52052`, while recent main analyses already use C# build mode `none`.

The evidence establishes the stale summary, not the internal reason it is retained. One possible explanation is that a genuinely obsolete category prevents the entire summary from being recomputed; that is a hypothesis, not a confirmed root cause.

## Workarounds and suggestions

- Check current analysis records and validate the generated SARIF independently. Our gate fails missing/invalid output, unsuccessful invocations, any unresolved finding, and warning/error extraction diagnostics; it does not rely on this neutral UI result.
- GitHub documents removing genuinely stale baseline configurations at https://docs.github.com/en/code-security/how-tos/manage-security-alerts/manage-code-scanning-alerts/resolve-alerts#removing-stale-configurations-and-alerts-from-a-branch. That is a deliberate security-history operation; we have not deleted historical analyses merely to clear the summary. It is not yet verified to refresh the remaining pending entries.
- Suggested backend fix: recompute each category's state after SARIF processing, even when another baseline category is missing, and retain the timestamp/commit/ref of the latest upload in the summary. Test a retired baseline category alongside a current category that finishes later.
- Suggested action diagnostics: when upload processing finishes, optionally log the analysis identifier, ref, category and matching comparison-check state under debug mode. This would make it easier to distinguish incomplete extraction, failed upload, configuration migration and a stale comparison result.
