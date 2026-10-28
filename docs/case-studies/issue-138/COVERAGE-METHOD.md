# Coverage measurement and generic instantiations

The acceptance target remains 100% coverage. Published Rust totals come from
LLVM's exported file summaries, including its branch counts; diagnostic source
location unions do not replace them. C# Debug and Release have different
sequence points, so both totals are retained. Actual WASM wrapper and complete
linked-core reports are reported separately.

## A bounded measurement experiment

Run both scripts under `experiments/issue-138/coverage-aggregation/` from a
Rust toolchain with `llvm-tools-preview`. The probes compile one 13-line file
at a time, execute two calls, then export raw JSON, instantiation detail and
LLVM's official summary. They need no repository build or network service.
`RUSTC_BOOTSTRAP=1` enables the same experimental branch instrumentation used
by the native coverage run. Neither probe stresses memory or recursion.

| Probe | Calls | Lines | Branches | Regions | Functions |
| --- | --- | --- | --- | --- | --- |
| `bash run.sh` | `select::<u32>(true)`, `select::<u64>(false)` | 9/10 (90%) | 1/2 (50%) | 12/13 (92.31%) | 2/2 |
| `bash run-control.sh` | `select::<u32>(true)`, `select::<u32>(false)` | 10/10 (100%) | 2/2 (100%) | 13/13 (100%) | 2/2 |

Measured with Rust 1.99.0 (`b940084d7`, LLVM 23.1.1). Both outcomes execute in
the first probe, but each generic instantiation takes only one outcome. The
control executes both outcomes within one instantiation. Assertions verify the
returned values, independently of the coverage result. Toolchain changes may
change these reports; the scripts retain version information and full outputs.

LLVM's [instantiation-group summary implementation](https://github.com/llvm/llvm-project/blob/a21f13bde2564a691a2da49adb773816f6c4e06b/llvm/tools/llvm-cov/CoverageSummaryInfo.cpp#L115)
merges the summaries of generic instantiations. Its [line and branch merge methods](https://github.com/llvm/llvm-project/blob/a21f13bde2564a691a2da49adb773816f6c4e06b/llvm/tools/llvm-cov/CoverageSummaryInfo.h#L85)
use the maximum covered count and maximum total count, rather than a union of
covered source coordinates. That implementation explains this experiment's
reported totals; this is not evidence of a compiler defect.

Consequently, `coverage-gaps.py` is only a bounded diagnostic. It coalesces
branch counts at the same source coordinates across instantiations, and its
line listing examines segment starts rather than reconstructing every continued
source region. An empty diagnostic listing does not establish complete LLVM
coverage. Read the official file summary and instantiation report when they
differ; add meaningful outcome tests within the appropriate instantiation.
Do not exclude production sources or restructure equivalent code just to alter
the measurement. Coverage also does not establish power-loss safety, exhaustive
scheduling, or equivalence to another database's guarantees.

## C# compiled branches

Retain the complete Debug and Release Cobertura reports and the collector's raw
JSON. Some generated methods have branch records but no source lines; an XML
source-line summary can omit those methods. The causal coverage gate therefore
checks every raw compiled branch, including methods with an empty line map.
Do not replace this check with an aggregate source-line percentage.

`summarize-csharp-causal-query.py` additionally reports each checked-query source
and its uncovered raw branches. The six existing causal components retain their
strict complete-branch gate. Passing that gate does not imply that new query
sources or the complete project have reached 100%; their remaining lines and
branches stay in the full report and the evidence receipt.

`check-author-fence-coverage.py` independently applies the same strict rule to
the optional author guard. The first focused Release report fails at 62/63
source lines and 106/124 raw branches. Additional public-contract cases cover
every recorded outcome in both Debug (87/87 lines, 124/124 branches) and Release
(63/63, 124/124). Both focused runs still instrument the complete production
assembly; their whole-project denominators and report hashes are retained in
[author-fence-coverage.txt](../../../dev/log/issues/138/pulls/139/author-fence-coverage.txt).
CI checks the component against each complete platform test report. This
component result does not complete the whole-project 100% criterion.

Native CI now retains official LLVM per-file/global JSON summaries alongside
its complete LCOV export. `check-author-fence-rust-coverage.py` prints their
complete totals and checks the author guard's source lines, branches, functions
and regions. It reports instantiations separately: the final focused audit
still has 22/38, despite complete source counters. Those entries stay in the
report and remain an unfinished acceptance criterion. Actual-WASM CI uses the
same gate against its existing complete LLVM JSON artifact. A passing source
counter gate does not establish complete instantiation coverage.

## Library placeholder control

`coverage-aggregation/library-instantiations/run.sh` separately tests a tiny
library and two consumers. Every source line, branch, function and region in
the library executes. With one concrete generic type, instantiations are 2/3;
with two concrete types taking both branches, they are 3/4. Both reports retain
the zero-count `select::<_>` placeholder. The test asserts returned values,
all source counters and the exact placeholder record independently.

The [pinned Rust 1.99 compiler source](https://github.com/rust-lang/rust/blob/b940084d7eb6a299eb4bfeb8e34901bc051e7ac4/compiler/rustc_codegen_llvm/src/coverageinfo/mapgen/unused.rs#L79)
documents that unused generic mappings from a library can survive even when a
downstream crate uses the function. The
[v0 symbol specification](https://doc.rust-lang.org/rustc/symbol-mangling/v0.html#type)
defines the `p` type code as a placeholder. All sixteen zero-count entries in
the final focused author-guard report use placeholder provider types; every
executed entry covers its regions. This matches the independently reproduced
compiler behavior. The [receipt](../../../dev/log/issues/138/pulls/139/library-instantiations.txt)
retains versions, report/source digests and every zero-count name. The raw
22/38 counter stays reported. This evidence neither excludes production code
nor resolves the separate uncovered source counters elsewhere in the project.
