# CI evidence for issue #111

Run IDs refer to https://github.com/link-foundation/link-cli/actions/runs/<id>.
Each section was recorded when its runs completed.

## First run of the gate (commit 4e540213)

| Workflow     | Run         | Result  | Cause                                                                                                                         |
| ------------ | ----------- | ------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Workflows    | —           | failure | zizmor 1.32.0 exited 12, raising `self-repository` on every `uses: ./.github/workflows/javascript-stage.yml`. Fixed in eb278c5f (see root cause 5). |
| Dependencies | 43579423907 | failure | Issue #176 closed at 08:27 and three new releases appeared: `cargo-audit` 0.23.0, `taiki-e/install-action` v2.90.4 and `lucide-react` 1.73.0. Fixed in f3639fbe. |
| Rust CI/CD   | 43579423628 | failure | "Changelog Fragment Check": the fragment was not committed yet. Every build and test job passed, including the golden fixture receipt. |
| C# CI/CD     | 43579423919 | failure | "Changeset Validation": the changeset was not committed yet. Every build and test job passed, including the golden fixture receipt.      |
| JavaScript, IPC, Docs, Security, Crash tests, DeepClient, Broken Link Checker | — | success | — |

In every one of these workflows, the `javascript / JavaScript stage` job ran
first, and the Rust and .NET jobs started only after it succeeded.

The JavaScript stage did not change in the later commits, and the pending
sections are filled in from the next runs.
