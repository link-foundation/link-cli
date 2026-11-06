# Issue #165 / PR #166 work plan

Objective: address every requirement in issue #165 in this single unmerged PR.

- [x] Read complete issue, issue comments, PR conversation/inline comments and reviews (initially all empty).
- [x] Inventory every workflow and CI/CD helper; snapshot full trees and relevant files from all four language templates and the requested best-practices document.
- [x] Download all issue-referenced runs, current failing PR runs, job annotations and related timeline evidence; distinguish old SHAs from current runs.
- [x] Reconstruct timeline and map each prose requirement to implementation and verification evidence.
- [x] Reproduce actual failures and audit false positives, false negatives, warnings and errors across all workflows and helpers.
- [x] Implement all applicable template/best-practice improvements; add minimal regression tests and default-off diagnostics wherever evidence is insufficient.
- [x] Report reproducible shared template/upstream defects with workaround and code-level fixes when found.
- [x] Commit each independently useful change; push only issue-165-a5206aa5b4c0 (final detection/evidence commit prepared at this cutoff).
- [x] Run focused inexpensive local checks; use CI for full builds and OS matrices (ten workflows green on fd17aa4 and 8f24ffd; final detection-commit confirmation pending).
- [x] Review complete PR source diff, latest comments, release trigger and main ancestry; final clean-status confirmation pending.
- [ ] Update title/body with reproduction, tests, complete evidence and Fixes #165; mark ready after all final-commit checks pass.

No required sub-issues are referenced in the initial issue or comments. Template-specific application/product features will be assessed for applicability and recorded, rather than introducing unrelated products into link-cli.
