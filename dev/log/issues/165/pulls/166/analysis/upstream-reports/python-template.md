Found while auditing https://github.com/link-foundation/link-cli/pull/166 against the complete template tree at `78f2fdf5e085871387555bc066071f0aedd24479`.

The SARIF gate in [`scripts/check_codeql_results.py`](https://github.com/link-foundation/python-ai-driven-development-pipeline-template/blob/78f2fdf5e085871387555bc066071f0aedd24479/scripts/check_codeql_results.py) accepts an incomplete extraction. Put this into `repro/example.sarif` and run `python3 scripts/check_codeql_results.py repro`:

```json
{"version":"2.1.0","runs":[{"tool":{"driver":{"name":"CodeQL"}},"results":[],"invocations":[{"executionSuccessful":true,"toolExecutionNotifications":[{"level":"error","message":{"text":"Extraction failed"}}]}]}]}
```

Actual: exit 0. Expected: nonzero, because an empty findings list does not prove source extraction completed. Real CodeQL SARIF from downstream CodeQL 2.27.4 exhibits exactly this combination (`executionSuccessful: true` plus extraction error notifications).

Workaround/code fix: validate every invocation's `toolExecutionNotifications` and `toolConfigurationNotifications`; fail on error, warning, missing level (SARIF defaults to warning), malformed records and malformed notification arrays. Keep note/none informational notifications allowed, and print the diagnostic descriptor, location and message. Include regression tests for both notification arrays even when `results` is empty. This is separate from gating ordinary unresolved vulnerability findings; that gate already exists.

Reproducible downstream comparison: [offline reproducer](https://github.com/link-foundation/link-cli/blob/issue-165-a5206aa5b4c0/experiments/issue-165/reproduce-template-gaps.mjs), run from the link-cli checkout with `node experiments/issue-165/reproduce-template-gaps.mjs`. It uses the committed immutable source snapshots, performs no network requests, demonstrates the template failures, and verifies the downstream fixes. [Collected evidence and analysis](https://github.com/link-foundation/link-cli/tree/issue-165-a5206aa5b4c0/dev/log/issues/165/pulls/166).
