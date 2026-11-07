# Native discovery effect boundaries

R1/R7/R8 require discovery to validate the selected native LSM state before any
initialization, repair or truncation. The complete C# coverage report still has
unexecuted rejection and cleanup paths in its read-only adapters. Extend the
contract evidence without changing production behavior or excluding those paths.

- [x] Trace the actual page, deferred-table and borrowed-WAL adapters and their
      native ownership boundary. Reuse existing independent page fixtures.
- [x] Verify every mutation rejects without changing physical file bytes or
      forwarding effects; reads return owned values and preserve capabilities.
- [x] Verify missing/malformed parts, index bounds, closed reads, acquisition
      errors, simultaneous cleanup errors and cached exact-once cleanup.
- [x] Run focused controls, complete Debug/Release coverage and local checks
      sequentially within the shared memory budget. Retain exact denominators.
- [ ] Publish this independent test step and inspect fresh exact-head CI.

These tests audit an existing guarantee; no newly reproduced production bug or
physical power-loss certification is claimed. Wider coverage and all other
unfinished acceptance criteria remain active.

The nonregular-handle control opens only the immediate OS null device; it never
opens a FIFO or waits for a pipe peer. .NET 10's
[Windows SafeFileHandle](https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Private.CoreLib/src/Microsoft/Win32/SafeHandles/SafeFileHandle.Windows.cs)
defines seek admission by disk handle type, and its
[OS stream strategy](https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Private.CoreLib/src/System/IO/Strategies/OSFileStreamStrategy.cs)
forwards that capability. The portable test must still pass actual Windows CI;
Linux execution alone cannot establish that platform result.

Local verification finishes 2026-10-27T10:37:00Z. All nine focused controls and
all 2,460 tests in each complete Debug/Release run pass. Debug covers
9,417/9,726 lines and 5,699/6,144 branches; Release covers 7,270/7,471 lines and
5,643/6,105 branches. Both configurations gain 17 lines/four branches with
unchanged denominators. discovery-guards.txt records the full evidence. The
tracking-dependent manifest policy failure was corrected independently in
mapped-grow-manifest-policy.txt; all 221 repository checks now pass.
