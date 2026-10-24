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
