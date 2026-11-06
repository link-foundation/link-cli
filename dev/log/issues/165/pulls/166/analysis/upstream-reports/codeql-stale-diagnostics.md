CodeQL Action 4.39.3 / CodeQL 2.27.4 reuses a JavaScript extraction error from the overlay-base database after the PR fixes the corresponding source. Discovered in https://github.com/link-foundation/link-cli/pull/166 while enforcing complete SARIF analysis.

Measured sequence:

1. Main run https://github.com/link-foundation/link-cli/actions/runs/42054737983 analyzes commit `717cf18`, caches `codeql-overlay-base-database-1-2c0214e11a876d0e-javascript-2.27.4-717cf18a4ff43be2890f98e12485d97f4b9241be-42054737983-1`, and reports a parser diagnostic for the valid awaited function call `await using(...)` (separate parser report: https://github.com/github/codeql/issues/23036).
2. PR commit `34330979f69590887beaa4044c13ac6e84253281` renames that helper and every call to `withClient`.
3. PR run https://github.com/link-foundation/link-cli/actions/runs/42133108866 restores that base database and uploads SARIF still reporting the old diagnostic at `js/client/test/conformance/run.mjs:164:18`. Its `timeUtc` is `2026-11-05T15:25:47.384528128Z`; this PR extraction ran after `2026-11-05T23:59:45Z`. The repaired source has no `await using` call.

Expected: source-local diagnostics for changed/re-extracted files describe the current checkout. Actual: the old diagnostic remains in `runs[].invocations[].toolExecutionNotifications`, making a complete-analysis gate fail on repaired code.

Reproduce using the two downstream commits/runs above and the uploaded `codeql-javascript-typescript` artifact. Collected source, SARIF and logs are under https://github.com/link-foundation/link-cli/tree/issue-165-a5206aa5b4c0/dev/log/issues/165/pulls/166 . Relevant log lines: 623–637 (overlay mode/cache), 1343–1347 (old location), 1417 (gate fails). The original SARIF notification preserves the old timestamp.

Workaround: set job environment `CODEQL_OVERLAY_DATABASE_MODE: none`, supported by `src/config-utils.ts:checkOverlayEnablement`, to generate fresh full analysis. This costs additional CI extraction but retains complete coverage. Do not suppress all extraction diagnostics or exclude the repaired conformance test.

Suggested code fix: invalidate source-local extraction diagnostics belonging to modified/deleted files when constructing the overlay invocation, and merge only current diagnostics for files extracted in the overlay. Keep base diagnostics for genuinely unchanged files. Add a regression fixture whose base has a parser error and whose overlay repairs that file, asserting both query results and invocation notifications are updated. The exact layer responsible (CLI diagnostic merging versus action overlay postprocessing) still needs upstream tracing; the stale notification itself is directly observed.
