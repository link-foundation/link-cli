# Issue #207 and PR #208 evidence

The [case study](../../../../../../docs/case-studies/issue-207/README.md) describes the timeline, root causes and requirement-to-verification map.

- [Original issue](github/issue.json), [comments](github/issue-comments.json), [timeline](github/issue-timeline.json), [sub-issues](github/issue-sub-issues.json) and [blockers](github/issue-blocked-by.json).
- [Original pull request](github/pull.json), [conversation](github/pull-conversation-comments.json), [inline review comments](github/pull-inline-comments.json) and [reviews](github/pull-reviews.json). Paginated GitHub API responses use one array per page.
- [Initial run IDs](github/named-run-ids.txt), [baseline default-branch snapshot](github/main-runs-before-fix.json), [default branch after both release commits](github/main-runs-after-releases.json), [first PR revision’s workflow runs](github/branch-runs-first-run.json), [individual checks](github/branch-checks-first-run.json), [branch protections](github/main-rules.json) and [related changes](github/related-prs.json).
- `runs/<id>.json` records job results, timestamps and the tested SHA. `runs/<id>.log.gz` is a complete available run log. `*.unavailable.txt` records the provider response when a log is inaccessible. Use `gzip -dc runs/<id>.log.gz | sed -n '1,1500p'` to read a log in sections.
- Template recursive Git trees and the [CI path comparison](templates/ci-file-comparison.tsv) pin the inputs. The [adopted guidance](templates/CI-CD-BEST-PRACTICES.md) is the downloaded Hive Mind guide. The inventory matches identical content and similar filenames. The case study compares equivalent practices whose filenames differ.
- `verification/` contains local verification logs. `artifacts/46062568123/` and `artifacts/46069793278/` contain the failed and passing etcd runs’ member logs.
- [Initial Codecov report](github/pull-conversation-first-run.json), [integration installation inventory](github/codecov-installations.json) and [requested permissions](github/codecov-app.json) record the optional GitHub App advisory.

| Evidence | Location |
| --- | --- |
| Draft’s root-layout assertion and failed JavaScript stage | [branch JavaScript log](github/branch-javascript-46067171135.failed.log#L864), lines 864–880 |
| JS client check cancelled in conformance | `runs/46059980457.log.gz`, lines 11863 and 12108 |
| Rust Windows test cancelled mid-suite | `runs/46059980682.log.gz`, lines 14386 and 25007 |
| etcd selected the former leader after SIGCONT | [failed conformance log](github/46062568123.failed.log#L120), lines 120–138 |
| Real etcd passed the revised paused-leader scenario in both journal modes | `runs/46069793278.log.gz`, lines 10651 and 10670. [Hosted conformance job](https://github.com/link-foundation/link-cli/actions/runs/46069793278/job/140744356887) |
| Windows Rust unit tests completed in 1,228.20 seconds on the first PR revision | `runs/46069793342.log.gz`, line 35760. [Hosted Windows job](https://github.com/link-foundation/link-cli/actions/runs/46069793342/job/140747171857) |
| Windows IPC returned `EPERM` in `RingStream._destroy` while closing a mapped ring | [Failed job log](github/46075606690.failed.log#L119), lines 119–138. [Reproduced before](verification/windows-ring-before.log), [passing after](verification/windows-ring-after.log), [focused IPC suite](verification/windows-ring-ipc-tests.log), [complete client coverage](verification/windows-ring-client-coverage.log.gz) and [Linux interop](verification/windows-ring-interop-linux.log) |
| npm server OIDC returned HTTP 404 | [failed preflight log](github/46059980580.failed.log#L165), lines 165–166 |
| Anonymous npm download succeeded for the client and returned HTTP 404 for the server | [client download](verification/npm-public-client.log) and [server download](verification/npm-public-server.log) |
| Rust and C# file-size warnings | `runs/46059980682.log.gz`, lines 2345–2346. `runs/46062568158.log.gz`, lines 1820–1821 |
