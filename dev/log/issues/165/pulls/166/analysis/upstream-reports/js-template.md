Found while auditing https://github.com/link-foundation/link-cli/pull/166 against the complete template tree at `7877a27980664bc6453e80262f8391d1c2562ab1`.

The lychee report parser in [`scripts/recheck-broken-links.mjs`](https://github.com/link-foundation/js-ai-driven-development-pipeline-template/blob/7877a27980664bc6453e80262f8391d1c2562ab1/scripts/recheck-broken-links.mjs) consumes an adjacent failure as part of the previous record. With this report:

```text
## Errors per input
* [404] https://example.com/missing
* [ERROR] https://example.com/reset
```

`parseLycheeFailures(report)` returns **one** record, whose trailing description contains `* [ERROR] https://example.com/reset`, instead of two. The record regex uses `\s*` after the URL; it consumes the newline, and `[^\n]*` then consumes the next record. That can bypass the bounded retry/archive decision for the second URL.

Reproduce by importing `parseLycheeFailures` from the linked script and passing the literal report above. The downstream offline reproducer below asserts the current one-record result against this exact snapshot and verifies the fixed parser returns both records.

Workaround/code fix: replace the separator's `\s*` with `[ \t]*`, keep the description on the same line, and restrict parsing to the Errors section. Add adjacent `[404]` / `[ERROR]` and no-description cases to the existing parser tests.

The SARIF gate in [`scripts/check-codeql-results.mjs`](https://github.com/link-foundation/js-ai-driven-development-pipeline-template/blob/7877a27980664bc6453e80262f8391d1c2562ab1/scripts/check-codeql-results.mjs) accepts an incomplete extraction. Put this into `repro/example.sarif` and run `node scripts/check-codeql-results.mjs repro`:

```json
{"version":"2.1.0","runs":[{"tool":{"driver":{"name":"CodeQL"}},"results":[],"invocations":[{"executionSuccessful":true,"toolExecutionNotifications":[{"level":"error","message":{"text":"Extraction failed"}}]}]}]}
```

Actual: exit 0. Expected: nonzero, because an empty findings list does not prove source extraction completed. Real CodeQL SARIF from downstream CodeQL 2.27.4 exhibits exactly this combination (`executionSuccessful: true` plus extraction error notifications).

Workaround/code fix: validate every invocation's `toolExecutionNotifications` and `toolConfigurationNotifications`; fail on error, warning, missing level (SARIF defaults to warning), malformed records and malformed notification arrays. Keep note/none informational notifications allowed, and print the diagnostic descriptor, location and message. Include regression tests for both notification arrays even when `results` is empty. This is separate from gating ordinary unresolved vulnerability findings; that gate already exists.

Reproducible downstream comparison: [offline reproducer](https://github.com/link-foundation/link-cli/blob/issue-165-a5206aa5b4c0/experiments/issue-165/reproduce-template-gaps.mjs), run from the link-cli checkout with `node experiments/issue-165/reproduce-template-gaps.mjs`. It uses the committed immutable source snapshots, performs no network requests, demonstrates the template failures, and verifies the downstream fixes. [Collected evidence and analysis](https://github.com/link-foundation/link-cli/tree/issue-165-a5206aa5b4c0/dev/log/issues/165/pulls/166).
