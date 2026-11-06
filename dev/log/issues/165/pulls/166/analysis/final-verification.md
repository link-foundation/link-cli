# Complete source validation and retained external diagnostics

All ten workflows passed on `5db14dddd56caa5e533d35aaf4a7361cd16e0706`, after every implementation change through the change-detection repair. The final follow-up disables only the diff-informed optimization, retaining full CodeQL extraction/queries and the strict SARIF gate; its own current-head checks are recorded on PR #166.

| Workflow | Successful source-validation run |
|---|---|
| Broken Link Checker | [42140455282](https://github.com/link-foundation/link-cli/actions/runs/42140455282) |
| C# CI/CD Pipeline | [42140455753](https://github.com/link-foundation/link-cli/actions/runs/42140455753) |
| DeepClient CI | [42140456113](https://github.com/link-foundation/link-cli/actions/runs/42140456113) |
| Dependencies | [42140455328](https://github.com/link-foundation/link-cli/actions/runs/42140455328) |
| Docs | [42140455891](https://github.com/link-foundation/link-cli/actions/runs/42140455891) |
| JS client CI | [42140455830](https://github.com/link-foundation/link-cli/actions/runs/42140455830) |
| Rust CI/CD Pipeline | [42140455615](https://github.com/link-foundation/link-cli/actions/runs/42140455615) |
| Security | [42140455675](https://github.com/link-foundation/link-cli/actions/runs/42140455675) |
| WebAssembly CI | [42140456230](https://github.com/link-foundation/link-cli/actions/runs/42140456230) |
| Workflows | [42140456052](https://github.com/link-foundation/link-cli/actions/runs/42140456052) |

## CodeQL diff limit

The five CodeQL initializations warned that the compare API truncates at 300 changed files (Security raw log lines 1265, 3576, 5955, 8180 and 9827). The action fell back to full analysis; this was not incomplete SARIF. The retained official `src/diff-informed-analysis-utils.ts` shows the exact limit and feature-flag guard. We now set `CODEQL_ACTION_DIFF_INFORMED_QUERIES=false` with full databases, so all queries run directly without requesting a truncated optimization input. A workflow regression fails before this setting and passes afterward. This increases analysis scope rather than suppressing a diagnostic or removing findings.

## Pre-existing GitHub comparison summary

GitHub Advanced Security check `127639504317` remains neutral with an early pending summary although the three named current analyses have successfully uploaded on the correct merge ref. Its obsolete C# autobuild category last ran on main on 2026-10-24; main already changed to build mode none before this PR. The same warning exists on initial head a20e071 (check 127599283191). API responses, configuration history, timestamps and the unsuccessful supported re-request are preserved under verification. The full reproduction, workaround and proposed backend/action diagnostics are in [the submitted report](upstream-reports/codeql-stale-comparison.md): https://github.com/github/codeql-action/issues/4308. The internal reason for retaining the pending summary is unknown; the retired-category hypothesis is explicitly unconfirmed. No historical analyses were deleted, and no alerts were dismissed. The five current language jobs and strict completeness/finding gate pass independently of that external UI summary.

## OpenSSF advisories retained

Dependency Review intentionally emits advisory project-practice scores below its default threshold 3. These are not vulnerability severities or failed dependency audits. Its official README distinguishes this advisory setting from fail-on-severity and warn-only; the latter remains false. The lockfiles are reproducible, the vulnerability audits pass, and we do not lower the Scorecard warning threshold. There is no demonstrated package-code defect to report from a low aggregate project-practice score alone. Replacing ESLint/secretlint transitive dependencies solely to alter upstream project metadata would not establish a code fix. All 23 current advisories are inventoried below (Security raw log lines 4882–4904).

| Package | Reported score |
|---|---:|
| acorn-jsx | 2.6 |
| astral-regex | 2.3 |
| color-name | 2.3 |
| debug | 2.6 |
| deep-is | 1.9 |
| esrecurse | 2.4 |
| estraverse | 2.8 |
| esutils | 2.3 |
| fast-deep-equal | 2.3 |
| fast-json-stable-stringify | 2.2 |
| imurmurhash | 2 |
| js-tokens | 2.9 |
| json-stable-stringify-without-jsonify | 1.9 |
| levn | 2 |
| optionator | 2.1 |
| prelude-ls | 2.1 |
| require-from-string | 2.3 |
| spdx-exceptions | 2.7 |
| spdx-expression-parse | 2.8 |
| spdx-license-ids | 2.5 |
| type-check | 2.3 |
| uri-js | 2.4 |
| validate-npm-package-license | 2.8 |

## Other external notices

The .NET coverage installer verifies its GPG signature, then reports that its imported key has no local web-of-trust certification (C# raw log line 11260). Global GPG trust is not relaxed. GitHub also emits an advisory macOS ARM capacity notice; the Rust ARM job successfully ran, while bounded recovery handles a confirmed future unacquired runner. Conditional release/deployment skips are deliberate on a PR and terminal gates enumerate them. These messages do not indicate failed compiler/tests, skipped security coverage or missing release configuration proof.

The PR is unmerged. Recovery trigger activation and actual registry publication retain the documented default-branch semantics. The neutral GitHub summary and external advisory metadata are disclosed; this record makes no claim of a warning-free external service.
