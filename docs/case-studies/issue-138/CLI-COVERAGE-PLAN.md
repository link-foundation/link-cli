# Public command recovery and version names

Requirement mapping: R1/R3/R7/R8/R9/R12. The complete measured denominator
remains the target. Focused success or a higher percentage does not close R8.

- [x] Rank complete-run Rust/C# coverage without excluding production files.
- [x] Read the uncovered C# command paths and existing integration fixtures.
- [x] Preserve finite before tests for any discovered behavior defect.
- [x] Verify delimiter-containing version names through both native commands,
      exact listing, checkout, repeated reopening and following commits.
- [x] Exercise real command dispatch in the coverage process, with serialized
      console capture, automatic restoration and exclusive-handle release checks.
- [x] Check invalid configuration diagnostics, version errors, import/export
      failures, trigger removal and subsequent successful database commands.
- [x] Compare complete Debug/Release and native coverage, preserving the full
      denominator and all remaining gaps. Run appropriate full local checks.
- [x] Preserve atomic changes, update public examples/documentation and obtain
      fresh exact-head CI before treating the command batch as verified.

The initial complete C# Release mirror report leaves 108 uncovered command
lines. Native Rust also has CLI and transaction/recovery gaps. The audit script
in experiments/issue-138/coverage-audit.py distinguishes LLVM zero-hit region
starts from Cobertura wholly unhit lines; it never changes a report denominator.

Initial source tracing found both command parsers split --tag at the first
equals sign, while the version libraries preserve delimiter-containing names.
Both native executables reproduce this defect before the correction. Test the
public name=sequence grammar with a name containing equals and a numeric/tag
point after its final delimiter. Preserve ordinary tag syntax and errors.
In particular, an existing delimiter-containing tag must remain a valid point
in alias=existing-tag. Splitting only at the final delimiter would regress that
previously accepted command. Prefer the earliest delimiter whose complete suffix
resolves, retaining the original interpretation whenever it succeeds; test this
ambiguity and preserve the original unknown-point diagnostic when none resolves.

Before evidence: cli-rust-tags-before.log fails at the real executable's unknown
point `雪=part=3`. cli-csharp-before-clean.log has exactly two failures at
`0=4` and `雪=part=4`, with 28 unrelated public recovery/configuration contracts
passing. The parsers now try delimiters left to right without changing any
previously successful resolution. Trailing/empty points still reject. The native
libraries and stored metadata grammar remain unchanged.

Existing subprocess tests remain necessary for actual executable integration.
Additional direct entry-point contracts should exercise distinct recovery and
configuration guarantees, rather than replace those process checks or merely
assert implementation branches. Test input and output must remain finite;
network rejection cases must avoid unbounded external connections.

The C# contracts use synchronous entry-point calls in the existing serialized
console collection. xUnit 2 rejects Timeout on synchronous tests before executing
them: an initial complete Debug run passed all 2,060 existing cases and rejected
31 new cases for this framework constraint. Those unsupported attributes were
removed; executable subprocess controls retain their existing finite deadlines,
and the CI test jobs have finite job deadlines. No production timeout changed.

A separate finite nesting control exercises depths 2/32/62/63/128/2,048 through
both native commands with capped child stack/CPU/heap and a kill/reap deadline.
The existing maintained parser already rejects the two excessive inputs with
its declared maximum depth of 64. Reopening and adding a third row afterwards
preserves exactly the two original rows and the new row. Accepted inputs have
the independently expected row count and final source-spine edge. No parser
behavior correction or new nesting guarantee is claimed.

Complete measured runs pass 923 native Rust cases (17 bounded child entries),
and 2,091 C# cases in each configuration. C# command unique-line coverage rises
from 597/759 to 736/767 in Debug and from 626/734 to 714/742 in Release. The
changed tag parser adds eight command lines in each configuration. Whole-project
totals retain every measured production file: Rust 12,815/13,646 lines and
2,381/2,816 branches; C# Debug 7,901/8,259 and 4,240/4,632; C# Release
6,072/6,296 and 4,194/4,605. These remain below the requested 100% target.
The 539-case actual-WASM mirror run remains current for its unchanged portable
sources; the changed native command parser does not run inside WASM.

All nine workflows pass on command head 63ccbc7, including Linux/macOS/Windows
native tests and actual WASM coverage. cli-ci.txt preserves the exact SHA,
creation timestamps and links. Later code still requires fresh exact-head CI.
