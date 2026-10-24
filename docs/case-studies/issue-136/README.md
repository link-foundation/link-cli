# Coverage and release authentication (issue #136)

## Failures reproduced

The reported main runs used commit `7c7bd51` on 2026-10-24:

- [C# run 40087795597](https://github.com/link-foundation/link-cli/actions/runs/40087795597)
  failed its mandatory Codecov token gate before coverage could upload.
- [Rust run 40087795520](https://github.com/link-foundation/link-cli/actions/runs/40087795520)
  failed to compile `version-and-commit.rs`: `PathBuf` was unused outside tests.
  Running `cargo build --release` on the script's generated manifest with
  `RUSTFLAGS='-D warnings'` reproduced the same failure locally. Script unit
  tests alone did not expose it because their `PathBuf` use enabled the import.
- After the separately managed npm bootstrap published version `0.1.0`,
  [JS client run 40094288228](https://github.com/link-foundation/link-cli/actions/runs/40094288228)
  failed with `You cannot publish over the previously published versions`.
  Even `npm publish --dry-run` checks the registry version. A mocked regression
  test reproduced the failure before the workflow was changed.

There were no required sub-issues or additional issue/PR review requests when
the implementation was investigated.

## Changes and verification

| Requirement | Implementation and evidence |
| --- | --- |
| Coverage requires no upload secret | The token gate is removed. The test job grants `id-token: write` and uses `codecov/codecov-action@v7` with `use_oidc: true`. [C# run 40094288203](https://github.com/link-foundation/link-cli/actions/runs/40094288203) successfully obtained an OIDC token and uploaded the report. No `CODECOV_TOKEN` secret was added. |
| Only an unactivated Codecov repository tolerates upload failure | `coverage.mjs` checks the public repository activation API. An explicit inactive response or HTTP 404 prints a warning explaining `fail_ci_if_error=false`. An activated repository requires success; API/network errors and malformed responses fail. Mocked tests cover these cases. The live API was inactive during initial investigation and activated before the successful CI upload. |
| Coverage remains visible independently | The Ubuntu test job writes Cobertura line/branch totals to the Actions summary and uploads `csharp-coverage` before contacting Codecov. The successful run retained the artifact. Recursive report discovery and summary generation have an integration test. All workflow coverage uploads follow the OIDC policy. |
| Production release helpers compile before merge | `check-scripts.sh` generates manifests with `rust-script --package` and builds all 16 executable helpers using `cargo build --release` and `-D warnings`, without executing them. Included modules are compiled through their callers. Rust lint runs this check on PRs. The `PathBuf` import now lives in the tests module; the standalone path helper also declares its regex dependency. |
| Preserve release behavior | All 29 existing release-helper tests pass. The existing issue-134 throwaway-repository experiment successfully ran versioning, collected 49 fragments, committed, tagged `rust-v1.0.9`, and pushed to its local bare origin with warnings denied. It left no uncommitted changes. No real registry publication or main-branch push was performed. A patch fragment triggers the next Rust release. |
| Verify npm before retiring its token fallback | [Authentication-only run 40094153480](https://github.com/link-foundation/link-cli/actions/runs/40094153480) successfully exchanged a GitHub OIDC token for a temporary npm credential without uploading a package. The npm token fallback has been removed. Actual publication uses OIDC and provenance; it still needs the package policy's permission to publish directly. |
| Validate published npm versions on PRs | Published versions use `npm pack --dry-run`; new versions retain `npm publish --dry-run --access public`. The regression test executes the workflow commands with a mocked registry client for both states. |

Local checks passed: 553 C# tests with Cobertura coverage, 370 Rust tests across
41 suites (two existing tests ignored), 29 release-helper tests, 217 JavaScript/
workflow tests, 37 client unit tests, client type declarations, seven WebAssembly
tests, the browser app production build, and Node/Bun/Deno conformance (32 passed
and four expected skips per runtime). The cross-backend synthetic check returned
identical data from Rust, C#, and WebAssembly. Rust formatting, both Clippy
workspaces, actionlint with ShellCheck, and zizmor passed. CI additionally passed
the Rust and C# OS matrices, optimized builds, and security/dependency checks.

## External blockers: still unfinished

The crates.io and NuGet migrations have **not** been verified. Their existing
token fallbacks remain because the issue explicitly requires removing each
fallback only after verification. Authentication-only dispatches make the
remaining checks reproducible without committing versions or publishing:

```sh
gh workflow run rust.yml --ref issue-136-fb310c545174 \
  -f release_mode=verify-trusted-publishing
gh workflow run csharp.yml --ref issue-136-fb310c545174 \
  -f release_mode=verify-trusted-publishing -f nuget_user=POLICY_CREATOR
gh workflow run js-client.yml --ref issue-136-fb310c545174 \
  -f verify_trusted_publishing=true
```

These modes skip normal build/test/release jobs and use their own concurrency
groups, so they do not interrupt regular CI. The initial Rust and NuGet
verification runs also started regular checks and were cancelled after their
authentication jobs finished; the isolated modes fix that behavior.

| Registry | Live result | Required external state |
| --- | --- | --- |
| crates.io | [Run 40095765020](https://github.com/link-foundation/link-cli/actions/runs/40095765020), authentication job: HTTP 400, `No Trusted Publishing config found for repository link-foundation/link-cli`. A separate read-only probe using the existing publishing token received HTTP 403 from the trusted-publisher configuration API: `this token does not have the required permissions to perform this action`. | A crate owner with account access or a token authorized to manage trusted publishing must configure this repository and `rust.yml` in the `link-cli` trusted publisher settings. After a successful verification, enable `CRATES_IO_TRUSTED_PUBLISHING=true` and remove both Rust release paths' secret fallbacks. |
| NuGet | [Run 40094151877](https://github.com/link-foundation/link-cli/actions/runs/40094151877), authentication job: HTTP 401, `No matching trust policy owned by user 'konard' was found`. No `NUGET_USER` repository or inherited organization variable is configured. | Identify the personal account that created a policy covering this repository, `csharp.yml`, and both packages (`clink` and `Foundation.Data.Doublets.Cli`), or create the policies using an authorized NuGet account. Verify that account, set `NUGET_USER`, then remove both release paths' API-key fallbacks. The tested GitHub account name does not establish that no other account owns a policy. |

The read-only crates.io configuration probe was temporary and has been removed;
it did not change registry configuration or disclose the token. The existing
publishing token cannot administer trust, and the GitHub credential available in
this workspace does not grant registry account access. NuGet policy creation
requires an authenticated account session and anti-forgery token, as shown by
the registry's [policy-creation endpoint](https://github.com/NuGet/NuGetGallery/blob/4f91f0b69278ef98292dfbd4ed637a846f5864cd/src/NuGetGallery/Controllers/UsersController.cs#L1319).
Its publishing API key does not provide that session. Package organization owners
on NuGet do not identify the personal policy creator. The required account is the
[NuGet policy creator](https://learn.microsoft.com/en-us/nuget/nuget-org/trusted-publishing),
not necessarily a package owner.

The issue's final acceptance criterion also requires a green **main** run and
the next real Rust Auto Release. PR builds and the local release experiment
verify the code paths but cannot prove a future post-merge publication. Those
criteria remain unfinished; keep the PR unmerged while the registry blockers
are unresolved.
