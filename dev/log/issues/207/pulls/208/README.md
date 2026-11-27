# Issue #207 and PR #208 evidence

The [case study](../../../../../../docs/case-studies/issue-207/README.md) describes the timeline, root causes and requirement-to-verification map.

- [Original issue](github/issue.json), [comments](github/issue-comments.json), [timeline](github/issue-timeline.json), [sub-issues](github/issue-sub-issues.json) and [blockers](github/issue-blocked-by.json).
- [Original pull request](github/pull.json), [conversation](github/pull-conversation-comments.json), [inline review comments](github/pull-inline-comments.json) and [reviews](github/pull-reviews.json). Paginated GitHub API responses use one array per page.
- [Initial run IDs](github/named-run-ids.txt), [baseline default-branch snapshot](github/main-runs-before-fix.json), [branch protections](github/main-rules.json) and [related changes](github/related-prs.json).
- `runs/<id>.json` records job results, timestamps and the tested SHA. `runs/<id>.log.gz` is a complete available run log. `*.unavailable.txt` records the provider response when a log is inaccessible. Use `gzip -dc runs/<id>.log.gz | sed -n '1,1500p'` to read a log in sections.
- Template recursive Git trees and the [CI path comparison](templates/ci-file-comparison.tsv) pin the inputs. The [adopted guidance](templates/CI-CD-BEST-PRACTICES.md) is the downloaded Hive Mind guide. The inventory matches identical content and similar filenames. The case study compares equivalent practices whose filenames differ.
- `verification/` contains local verification logs. `artifacts/46062568123/` contains the failed etcd run’s member logs.

| Evidence | Location |
| --- | --- |
| Draft’s root-layout assertion and failed JavaScript stage | [branch JavaScript log](github/branch-javascript-46067171135.failed.log#L864), lines 864–880 |
| JS client check cancelled in conformance | `runs/46059980457.log.gz`, lines 11863 and 12108 |
| Rust Windows test cancelled mid-suite | `runs/46059980682.log.gz`, lines 14386 and 25007 |
| etcd selected the former leader after SIGCONT | [failed conformance log](github/46062568123.failed.log#L120), lines 120–138 |
| npm server OIDC returned HTTP 404 | [failed preflight log](github/46059980580.failed.log#L165), lines 165–166 |
| Rust and C# file-size warnings | `runs/46059980682.log.gz`, lines 2345–2346. `runs/46062568158.log.gz`, lines 1820–1821 |
