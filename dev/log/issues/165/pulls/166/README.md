# Issue #165 / PR #166 evidence

Start with [the audit](analysis/AUDIT.md), which maps the complete requirements to root causes, alternatives, implemented fixes and verification. [The chronology](analysis/timeline.md) distinguishes the eleven issue-referenced runs from subsequent implementation runs by timestamp and SHA.

- `github/`: original issue/PR/API responses, jobs, paginated annotations, run metadata and submitted upstream report URLs. Only secret **names**, never values, were collected.
- `ci-logs/`: complete available workflow logs, focused debug logs and SARIF artifacts. Raw logs are losslessly gzip-compressed. The failed runner-acquisition job has no executed-step log; its API metadata and annotation are the evidence.
- `templates/`: four immutable recursive trees, source metadata, selected-file inventories and `snapshot.tar.gz` source archives. The 1,063-row [comparison](analysis/template-file-matrix.csv) records every blob's disposition and downloaded content hashes; the [content inventory](analysis/template-content-inventory.md) records inspected workflows and helper declarations.
- `research/`: primary-source documentation, upstream code, releases and existing issue records.
- `verification/`: actual local checks, collection commands and earlier failed experiments retained for reproducibility. Historical failed checks are superseded only by a later check of the relevant changed source, never silently rewritten as successful.
- `analysis/upstream-reports/`: exact report text with reproductions, workarounds and suggested code changes.

Read a raw workflow log with `gzip -cd ci-logs/run-<id>.log.gz`. Extract a template using `tar -xzf templates/rust/snapshot.tar.gz -C /tmp`; the archive has a `rust/` prefix (likewise for the other languages). Keep extracted evidence outside the source tree so build and lint inventories remain accurate.

The reusable collection, archive, comparison and before/after regression scripts are in `experiments/issue-165/`. Collection is bounded at an explicit audit cutoff; final-commit live check URLs are recorded in PR #166, because committing a run's own logs necessarily creates a new commit and a new run.

All ten workflows passed on `fd17aa4ecc799435c5e70b16e5b0021489e20d2a`, `8f24ffd509a7c3c5b77b4521e1cb8151bbd44e0e` and `5db14dddd56caa5e533d35aaf4a7361cd16e0706`; their complete logs and conclusions are included among 93 collected runs. The last of those includes the additional change-detection repairs. [Final source validation and external diagnostics](analysis/final-verification.md) records every successful workflow, the subsequent full-query optimization correction, the pre-existing neutral GitHub CodeQL comparison summary and all retained OpenSSF advisories. Current final-head check results are recorded on the PR before it is marked ready.
