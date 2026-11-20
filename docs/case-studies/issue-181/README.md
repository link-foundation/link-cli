# NuGet packaging and publication (#181)

## Requirements and verification plan

The issue and all issue/PR #182 conversation comments, inline comments and reviews
were read on 2026-11-15. There were no comments, reviews or required sub-issues.

| Requirement | Implementation | Verification |
| --- | --- | --- |
| Exclude every example and experiment | Explicit `IsPackable=false` in all 19 previously packable projects; the twentieth already had it | Regression test fails before the fix; `dotnet pack --no-restore` on all 20 projects produces no files |
| Produce only `clink` and `Foundation.Data.Doublets.Cli` | Exact artifact allowlist after all three workflow pack commands, before upload/push | Unit tests reject extra, missing, stale, symbol and mismatched-version packages; Build Package job packs the solution and checks the output |
| Publish both packages at 5.1.0 | Keep both project versions at 5.1.0; `publish-current` dispatch resumes the existing release path without consuming changesets or bumping versions | Mocked regressions cover either package missing after a partial publication; live publication remains blocked below |
| Future releases use trusted publishing | Existing pinned `NuGet/login` steps and `id-token: write` in both release paths select OIDC when `NUGET_USER` is set | Live authentication failed; external account policy configuration remains unfinished |
| No example package on NuGet | Check both owner spellings against NuGet search, including the actual organization `link.foundation` | Search returned no packages for `link-foundation`; `link.foundation` returned six packages, none of the example/experiment IDs |

The existing release script checked only the CLI feed. It now checks both feeds
in the workflow so a partial upload cannot be mistaken for a complete release.
All future example/experiment project changes trigger the C# packaging checks.

Both package versions stay at 5.1.0. The release gate recovers an unpublished
current version before consuming new changesets, including the patch metadata
for this packaging fix and coverage-collector update. Bumping first would skip
the requested release. No production C# source or public API changes are needed.

## Reproduction and local checks

Before the exclusions, `node --test csharp/scripts/verify-packages.test.mjs`
failed on `examples/cluster-csharp/ClusterExample.csproj`. After the changes,
the exclusion and artifact allowlist tests pass. The existing C# script and
workflow test suites also pass.

```sh
node --test csharp/scripts/*.test.mjs .github/scripts/*.test.mjs
node csharp/scripts/verify-packages.mjs --pack-excluded
# Run on a built solution (also performed in CI):
dotnet pack csharp --no-build --configuration Release --output csharp/artifacts
node csharp/scripts/verify-packages.mjs
```

## External publication blocker: unfinished

[Fresh authentication-only run 43773497911](https://github.com/link-foundation/link-cli/actions/runs/43773497911)
failed on 2026-11-15. Its NuGet login reports HTTP 401:
`No matching trust policy owned by user 'konard' was found.`
The downloaded log has this error on line 60. No `NUGET_USER` repository variable
or publishing secret is configured. Testing this account does not prove that
another personal account has no policy. The available GitHub authentication
cannot administer NuGet account policies; there is no authenticated NuGet web
session available in this workspace.

Both public flat-container feeds still end at 4.0.6, so publication of **both
5.1.0 packages** and **verified future trusted publishing** remain unfinished.
Keep the PR unmerged until those criteria are met.

A NuGet account with access must create/identify a trusted publisher policy
covering repository owner `link-foundation`, repository `link-cli`, workflow
`csharp.yml`, and both package IDs. Use the personal **policy creator** as the
NuGet login user, per the [official documentation](https://learn.microsoft.com/en-us/nuget/nuget-org/trusted-publishing).
Verify before enabling the repository variable:

```sh
gh workflow run csharp.yml --repo link-foundation/link-cli \
  --ref issue-181-b34d51fa134d \
  -f release_mode=verify-trusted-publishing -f nuget_user=POLICY_CREATOR
# After successful verification, set NUGET_USER to POLICY_CREATOR, then:
gh workflow run csharp.yml --repo link-foundation/link-cli \
  --ref issue-181-b34d51fa134d -f release_mode=publish-current
```

The latter mode runs the normal build/test/package gates, publishes the current
version with duplicate-safe retries, waits for both NuGet feeds and creates the
GitHub release. It performs no version commit. Existing optional API-key release
support is preserved until trusted publishing can actually be verified.

## Required CI repairs

The initial Dependencies run [43773295255](https://github.com/link-foundation/link-cli/actions/runs/43773295255)
failed on outdated declarations (downloaded log lines 210–232), including the
closed cargo-audit holdback. A fresh upstream probe now confirms that released
install-action v2.90.5 contains cargo-audit 0.23.0, so both action pins and the
audit version are updated without an exception. Coverage collector, npm tools
and both Rust lockfiles are refreshed for the same required latest-dependency
check. These maintenance changes preserve existing features and are validated
by the existing CI suites. The task-preparation `.gitkeep` is removed because
the repository layout test forbids root placeholder files.
