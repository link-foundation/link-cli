# Fallible allocation through composed decorators

Requirement mapping: R1/R6/R7/R8/R9/R12. Wider acceptance criteria stay active.

- [x] Trace named-query allocation, transaction/version bridges and trigger
      bookkeeping. Existing transaction/version fallible bridges are bypassed
      by trigger defaults and the planner's temporary allocation probe.
- [x] Preserve failing real-filesystem regressions before changing production.
- [x] Add portable allocation-fault contracts and execute them on actual WASM.
- [x] Propagate allocation errors through trigger/query composition, including
      schema relationships, while preserving existing infallible interfaces.
- [x] Verify successful/no-op paths, trigger application, invalid addresses,
      caller ownership, cleanup/retry and native persistent reopening.
- [x] Verify the C# exception-propagation control without claiming its API has
      the Rust Result/infallible-bridge defect.
- [x] Run focused then complete local suites, coverage and required checks.
- [ ] Publish an atomic change, update PR 139 and inspect fresh exact-head CI.

All fixtures use finite small addresses and dedicated temporary files. No
process-wide corruption, unbounded allocation or host stress is necessary.
Returning an error does not itself undo an operation whose durable outcome is
uncertain; existing transaction cleanup/recovery remains authoritative.

Complete local evidence: `dev/log/issues/138/pulls/139/fallible-composition.txt`.
Fresh CI must be verified after publishing this atomic change.
