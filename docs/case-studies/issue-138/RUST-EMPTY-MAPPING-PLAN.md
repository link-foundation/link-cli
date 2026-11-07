# Empty-capacity mapping boundaries

Continue the R1/R6/R7/R8 ownership audit at zero and rejected shrink requests.
Do not confuse these boundaries with general failed partial-shrink recovery.

- [x] Trace upstream shrink: it updates the buffer, unmaps and truncates even
      for zero removal. This can erase persisted bytes of a not-yet-adopted
      owner. Excessive removal panics after the wrapper invalidates its barrier.
      memmap2 0.9.12 already handles a genuinely empty map on both OS paths;
      a zero-length Windows mapping syscall must not be inferred from the
      outer FileMapped call alone. Retain shrinking all items as a control.
- [x] Run six finite durable boundary tests against unchanged 2478d59 and
      preserve their actual failures before changing production code.
- [x] Make zero removal a true no-op and reject excessive removal before
      invalidation, preserving existing genuine-shrink and destructor behavior.
- [x] Verify exact-once destructor ownership, empty reopening, following writes
      and retained flush admission with independent controls. Do not claim to
      repair actual truncation errors or failed partial-shrink ownership.
- [x] Run complete native and actual-WASM suites with all production coverage
      denominators. Validation finishes 2026-10-27T11:01:26Z: 1,190 native tests,
      six doctests, 723 actual-WASM tests and 221 repository checks pass.
- [ ] Publish the atomic correction and inspect fresh exact-head platform CI.

Keep each allocation finite. Full hardware power-loss verification, general
failed partial shrink and failed destructor-bearing growth stay unfinished
unless independently implemented and verified. Preserve every wider R1-R12 gap.

Actual immutable baseline 2478d59ef43f7766748317be53ca928164d43274 fails four
regressions and passes the two genuine-shrink controls. mapped-empty-before.txt
preserves the exact physical lengths, lost values and barrier failure. The
minimal wrapper correction returns immediately for zero removal and validates
excessive removal before descriptor invalidation. Both default/all-feature
focused suites now pass six empty/shrink cases, six flush cases and six growth
cases. Zero removal adds no allocation, lock, remap or file I/O; valid real
shrink keeps its previous ownership path and failure-state limitations.
