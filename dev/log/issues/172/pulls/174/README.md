# Issue #172 / PR #174 evidence

Start with [the audit](analysis/AUDIT.md). It maps every requirement and every observed warning, error, false positive and false negative to:

- its root cause
- the alternatives considered
- the implemented fix
- the regression test

[The timeline](analysis/timeline.md) reconstructs the `main` runs at `199a07b` that the issue lists. [The upstream reports](analysis/upstream-reports/SUMMARY.md) record each template defect, its evidence and the issue filed for it.

## Contents

- `github/` holds the original issue and its comment, the PR comment/review API responses, and the metadata, jobs and per-job annotations of every run the issue references.
  - It also has the check runs of `199a07b`, the run history of `main` and of the issue #138 branch, and the Actions cache inventory on 2026-11-10.
  - Only secret **names** were collected, never values. Commit author e-mail addresses in the run metadata are replaced with `<redacted>`.
- `ci-logs/` holds the complete workflow logs, losslessly gzip-compressed. For a run whose combined log was unavailable, the per-job logs are there instead (`run-<id>-job-<job>.log.gz`). Read one with `gzip -cd ci-logs/run-<id>.log.gz`.
- `templates/` holds, for each of the five pipeline templates:
  - the recursive file tree
  - the source commit
  - for the four templates compared in PR #166, the diff and diffstat since that comparison
- `research/` holds primary sources:
  - the CI/CD best-practices document
  - lychee's markdown report writer, which defines the `Errors per input` and `Timeouts per input` sections
  - NuGetGallery's API key authorization attributes, which define what a 401 and a 403 mean
- `analysis/` holds the audit, the timeline, the flattened annotation list of `199a07b` and the upstream reports.

The collection script is `experiments/issue-172/collect-runs.sh <evidence-dir> <run-id>...`. It also fetches per-job annotations, because warnings never fail a job and are invisible in the run conclusion.
