# Restore the scheduled dependency audit (#191)

The enforcement audit on `main` failed on six outdated entries. The
registry lookups and Cargo dry runs completed, and the tracking job opened
issue #191 as intended. There were no issue comments, PR reviews, or
required sub-issues at the time of investigation. [Scheduled run](https://github.com/link-foundation/link-cli/actions/runs/45026148237) · [Issue #191](https://github.com/link-foundation/link-cli/issues/191)

## Timeline

| Time (UTC) | Event |
| --- | --- |
| Nov 21, 20:50 | The push audit for `fbc90908` passed in report mode. That mode surfaces newer releases without failing on version drift. [Run](https://github.com/link-foundation/link-cli/actions/runs/44948417817) |
| Nov 22, 13:12 | The scheduled enforcement audit for the same commit reported six outdated entries and exited 1. The tracking job opened issue #191. The downloaded run log records the versions on lines 1054–1059. [Run](https://github.com/link-foundation/link-cli/actions/runs/45026148237) |
| Nov 22, 17:52 and 22:36 | The zizmor action and the standalone scanner released new versions. A fresh local audit detected both updates alongside the six scheduled findings. [Action v0.9.0](https://github.com/zizmorcore/zizmor-action/releases/tag/v0.9.0) · [Scanner v1.33.1](https://github.com/zizmorcore/zizmor/releases/tag/v1.33.1) |
| Nov 22, 22:47 | The zizmor v0.9.0 action rejected scanner 1.33.1 on PR #190. The downloaded job log records `Unknown version 1.33.1` on line 195. The action's release contains a lock for 1.33.0. [Failed job](https://github.com/link-foundation/link-cli/actions/runs/45079504717/job/137384237703) · [Action locks](https://github.com/zizmorcore/zizmor-action/tree/v0.9.0/support/locks) |
| Nov 22, 23:54 | The branch for #191 passed the hosted enforcement audit for 184 declared versions and the Cargo lockfiles. [Audit job](https://github.com/link-foundation/link-cli/actions/runs/45084556111/job/137402070984) |
| Nov 23, 01:13 | PR #190 merged the same dependency pins, hash-verified zizmor installation, and parser regression test into `main`. PR #192 incorporated that commit and carries the Rust patch fragment and this case study. [PR #190](https://github.com/link-foundation/link-cli/pull/190) · [PR #192](https://github.com/link-foundation/link-cli/pull/192) |

## Fix and verification

| Audit finding | Change | Verification |
| --- | --- | --- |
| `taiki-e/install-action` v2.90.6 in Security and WASM | Pin both uses to the commit for v2.90.8. [Released commit](https://github.com/taiki-e/install-action/commit/fe3acb53a98b62c772db35de75ce0e4595e7ca8c) | Enforced dependency audit and existing Security and WASM jobs |
| `secretlint` 14.0.1 in `js/package.json` and `js/package-lock.json` | Bump to 14.1.0 and regenerate its npm resolution. | Enforced audit, `npm ci --prefix js`, and the existing tracked-file secrets scan |
| `hashbrown` 0.17.1 in `rust/Cargo.lock` and `walkdir` 2.5.0 in `rust/wasm/Cargo.lock` | Resolve 0.17.2 and 2.5.1 with targeted `cargo update` commands. The Rust fragment triggers a patch release. | Enforced audit, locked native and WASM metadata resolution, and existing Rust and WASM checks |
| zizmor 1.32.0 and zizmor-action v0.8.1 | Share one hash-verified PyPI wheel for zizmor 1.33.1 across both audit passes. Reuse the wheel installation and requirements-parser changes from [PR #190](https://github.com/link-foundation/link-cli/pull/190). | Both audit passes and the `requirements with wheel hashes still participate in freshness checks` regression test |

The updated zizmor manifest stays covered by the freshness audit and the
existing Dependabot pip group. The regular audit continues to emit GitHub
annotations and use the GitHub token for online checks. The pedantic audit
continues to enforce high-severity, high-confidence findings.

Reproduce the audit failure on `fbc90908` and verify the PR with the same
command. Registry results can change as new versions ship.

```sh
GITHUB_TOKEN="$(gh auth token)" CHECK_DEPENDENCIES_MODE=enforce \
  node .github/scripts/check-latest-dependencies.mjs
node --test .github/scripts/check-latest-dependencies.test.mjs \
  js/test/repositoryLayout.test.mjs
```

The prepared PR's root `.gitkeep` also failed the repository-layout test.
Removing the placeholder restores that check. Once the fix reaches `main`,
the next passing scheduled or manual enforcement audit can close the tracking
issue. [PR #192](https://github.com/link-foundation/link-cli/pull/192)

## JS server release on main

The separate JS server workflow on `main` failed at npm trusted-publisher
verification with HTTP 404 on line 169 of its failed-job log. The public npm
lookup for `@link-foundation/link-cli-server` also returned 404. An npm
namespace maintainer must publish the initial version and configure
`link-foundation/link-cli` with `js-server.yml` as the trusted publisher.
The pull-request preflight already treats publishing credentials as advisory
and validates the package with a dry run. [Main run](https://github.com/link-foundation/link-cli/actions/runs/44948417807) · [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/)
