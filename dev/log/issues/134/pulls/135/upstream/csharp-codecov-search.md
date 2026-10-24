## Summary

The Codecov step of `release.yml` lets the uploader search the whole checkout and run its default plugins:

https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/blob/4cc372ebf2075a0c948b46881143f7d66f0f0e7e/.github/workflows/release.yml#L294-L304

```yaml
- name: Upload coverage to Codecov
  timeout-minutes: 5
  if: matrix.os == 'ubuntu-24.04' && env.CODECOV_TOKEN != ''
  uses: codecov/codecov-action@v7
  with:
    token: ${{ env.CODECOV_TOKEN }}
    fail_ci_if_error: true
```

1. **False positive coverage reports.** The search treats any file whose name matches its patterns (for example `*coverage*`) as a report. In link-foundation/link-cli, which uses the same step, it uploaded the Rust test source `rust/tests/issue62_review_coverage_tests.rs` next to the real `coverage.cobertura.xml` on every run:

   ```
   info - Found 2 coverage files to report
   info - > /home/runner/work/link-cli/link-cli/rust/tests/issue62_review_coverage_tests.rs
   info - > /home/runner/work/link-cli/link-cli/csharp/Foundation.Data.Doublets.Cli.Tests/TestResults/<guid>/coverage.cobertura.xml
   ```

2. **Warning noise.** The default `xcode`, `gcov` and `pycoverage` plugins log on every run of a .NET project:

   ```
   warning - xcrun is not installed or can't be found.
   warning - No gcov data found.
   warning - coverage.py is not installed or can't be found.
   ```

3. **Silent false negative on the default branch.** When `CODECOV_TOKEN` is absent, the step is skipped with a `::notice::` on every event, so `main` stays green while coverage is never uploaded. In link-cli the earlier `fail_ci_if_error: false` variant failed with `Token required - not valid tokenless upload` on `main` (run 40076673386) and on pull requests while every job stayed green; Codecov's API still reports the repository as `"activated": false, "totals": null`.

Issue #34 introduced the token gate and `fail_ci_if_error: true`; the points above are what is left after it. The Rust template already restricts its upload with `files: lcov.info` and `disable_search: true`.

## Reproduction

Add any tracked file named like `tests/foo_coverage_tests.cs` to a repository generated from the template, configure `CODECOV_TOKEN` and look at the "Upload coverage to Codecov" log: the file is listed under "Found N coverage files to report".

## Workaround

Rename such files, or restrict the search in the workflow.

## Suggested fix

```yaml
with:
  token: ${{ env.CODECOV_TOKEN }}
  directory: tests/<Project>.Tests/TestResults   # only the dotnet test reports
  plugins: noop
  fail_ci_if_error: true
```

and, to keep a missing secret from passing unnoticed, a job that runs only on `push` to the default branch, needed by no release job, and fails with what to do when `CODECOV_TOKEN` is empty. Pull requests stay green without the secret. link-cli implements this in `.github/workflows/csharp.yml` (the `Check Codecov token` step and the `codecov-token` job), guarded by the policy test "every Codecov upload uses a token, reads only its reports and fails loudly".
