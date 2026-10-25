# Public replay admission

Requirement mapping: R1/R3/R7/R8. Public apply/revert methods change data without
logging another transition. A live transaction owns pending writes or cleanup
after a commit/rollback error; these helpers must not overwrite its rows.

- [x] Reproduce apply and revert over pending, failed-checkpoint and failed-undo
      owners: six finite cases per native language, with one-shot flush faults.
      Establish behavioral failures before modifying production code.
- [x] Reject public replay before changing the replay flag or touching data
      whenever an owner remains live. Preserve internal recovery/undo helpers.
- [x] Verify unchanged rows and journal sequence, retained admission, retry of
      the original decision, following edits and unlogged replay outside owners.
- [x] Execute the same Rust contracts in actual WASM and run focused version
      controls, full local suites, coverage, formatting and strict checks.
- [x] Preserve before logs and measured evidence; update recipes/release notes.
- [x] Commit independently and publish only the prepared branch.
- [x] Verify new exact-head CI and review all feedback and the final diff.

Tests use three rows/updates and a six-case matrix, with no stress loop. Native
storage/checkpoint behavior and portable logical contracts are measured
separately. Atomic multi-transition checkout and ordinary legacy name logging
remain separate unfinished audits.

All six cases per native language first fail at the behavioral assertion: no
exception/error is returned by replay. The initial Rust fixture required the
borrowed-read extension trait; that compile diagnostic is retained separately
and is not counted as a reproduced behavioral failure.

All nine workflows pass on 31a50a0. See replay-owner-ci.txt for exact
SHA/timestamps, run URLs and the transient Bun installation retry.
