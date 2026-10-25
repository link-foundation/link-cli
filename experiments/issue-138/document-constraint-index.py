"""Preserve final complete-run results after sequential validation succeeds."""
import json
from pathlib import Path

root = Path.cwd()
summary = json.loads((root / "experiments/issue-138/constraint-index-summary.json").read_text())
assert summary["native"] == {"passed": 859, "failed": 0, "ignored": 13}
assert summary["wasm"] == {"passed": 502, "failed": 0, "ignored": 0}
assert summary["csharp_Debug"]["passed"] == summary["csharp_Release"]["passed"] == 2003
assert summary["js"]["pass"] == 168 and summary["repository"]["pass"] == 217
rows = []
for label, key in [("Native Rust", "native_coverage"), ("Actual WASM, complete linked source", "wasm_coverage")]:
    def metric(kind):
        value = summary[key][kind]
        return f"{value['covered']:,}/{value['count']:,} ({100 * value['covered'] / value['count']:.2f}%)"
    rows.append(f"| {label} | {metric('lines')} | {metric('branches')} | {metric('functions')} |")
for mode in ("Debug", "Release"):
    def metric(kind):
        values = summary["csharp_" + mode]["coverage"]
        covered, count = values[kind + "-covered"], values[kind + "-valid"]
        return f"{covered:,}/{count:,} ({100 * covered / count:.2f}%)"
    rows.append(f"| C# {mode} | {metric('lines')} | {metric('branches')} | Not reported by this collector |")
table = "\n".join(["| Scope | Covered lines | Covered branches | Covered functions |", "| --- | --- | --- | --- |"] + rows)
component_rows = []
for label, key in [("Native Rust", "native_coverage"), ("Actual WASM", "wasm_coverage")]:
    component = summary[key]["constraint_index"]
    component_rows.append(label + ": " + ", ".join(
        f"{value['covered']}/{value['count']} {kind}" for kind, value in component.items()))
for mode in ("Debug", "Release"):
    components = summary["csharp_" + mode]["constraint_index"]
    assert all(float(entry["line-rate"]) == float(entry["branch-rate"]) == 1 for entry in components)
component_text = "; ".join(component_rows) + ". C# Debug/Release class summaries report 100% lines and branches."
body = """Optional indexed constraints: complete local validation
PR: https://github.com/link-foundation/link-cli/pull/139
Date: 2026-10-25 UTC
Base: 3322f36aa838c7a6c01d1b7e0d7d30adb78f56dd

Reproduce with experiments/issue-138/validate-constraint-index.sh after the
documented validation setup, then summarize-constraint-index.py. Compilers run
sequentially within the 2.92 GiB limit. An unnecessary Vec in the new Rust test
first failed strict Clippy; a fixed array corrects it. The --resume-rust path
retains the completed C# reports after that test-only correction and reruns all
later validation. Only completed successful suites supply the final totals below.

Scan remains the default. Independently selected Indexed retains persistent
pair/name-owner and incoming-reference-count maps only for selected constraints.
Private candidates remove every old changed key before adding new keys, allowing
atomic swaps. Publication follows journal append and flush; rejected or uncertain
candidates retain committed roots. Recovery validates every historical record
before building the final indexes. The journal format is unchanged.

Before-implementation native/C# contracts fail because the configuration API is
absent; constraint-index-before.txt preserves that evidence without claiming a
scan semantic defect. Each native language independently checks 6,400 finite
candidate edits against a simple full-state oracle across all constraint subsets,
both maps and both logical encodings. Tests cover zero/absent/self/cyclic refs,
both incoming-reference positions, unchanged incoming rows, Unicode/NUL/empty
names, swaps/transfers, transient rows, rejected/following writes, mixed replay,
historical invalidity, pins, groups, branches and independent admission/read
validation. The six portable Rust contracts also execute in actual WASM.
Existing all-prefix durable group fault matrices now run with scan and indexed
unique-name validation. C# additionally checks before/after append and flush
uncertainty with exact committed root identity and recovered following writes.

Finite cost controls verify only selected map allocation and retained root
sharing. C# forbids all candidate enumeration and bounds one-row preparation to
seven row lookups at 8 and 512 rows. The reusable native allocation example uses
shared rows, changed-row encoding and all constraints for eight name edits.
Scan versus indexed requests 54,805 versus 17,845 bytes at 128 rows, and 329,269
versus 20,117 at 1,024 rows in this Debug run. These count allocation requests,
including reallocations, excluding setup/recovery; they are neither live-memory
nor latency/durability measurements. Exact totals can vary with toolchains.

Complete local validation:

- Native Rust all-features branch coverage: 859 standard passes, zero failed;
  thirteen ignored child entries are exercised by bounded parent tests. The
  custom stdout harness passes.
- C# Debug and Release with Coverlet: 2,003 each, zero failed/skipped.
- Actual WASM with LLVM coverage: 502 pass, zero failed/ignored.
- JS: 168 tests; all eighteen production modules enforce 100% line/branch/function
  coverage. Strict TypeScript and 217 repository checks pass.
- Formatting, strict native/WASM Clippy, source sizes, release fragments,
  actionlint/ShellCheck, whitespace and 117 latest declared dependencies pass.
- Generated actual-WASM conformance passes Node, Bun and Deno, including the
  public scan/indexed option, invalid types, swaps, groups, branches and replay.
- All sixteen SQLite scan/indexed x flat/shared x snapshot/changes x plain/delta
  compositions reopen the same database through Rust -> C# -> Node -> Bun ->
  Deno. Both strategies select References only: repeated provider pairs/names
  intentionally remain valid. All three runtime SQLite adapter suites pass.
- Existing persistent strategy/group examples and both ordinary undo policies
  pass. The finite allocation example passes. Container OOM/OOM-kill remain zero.

Official complete-run totals, without filtered denominators or diagnostic unions:

""" + table + "\n\nConstraintIndex component: " + component_text + """

All three actual-WASM wrapper files measure 100% lines, reported branches and
functions. Whole native Rust, linked WASM and C# remain below the required 100%.
Selected indexes do not remove flat-map copies, complete snapshot encoding,
historical recovery scans, branch comparisons or physical I/O. Finer independent
composition, legacy physical page repair, LSM, real power-loss verification,
automatic balancing/multi-catalog transactions and wider competitor criteria
remain unfinished. The native issue goal stays active and PR 139 remains draft;
fresh exact-head CI is required after publication.
"""
(root / "dev/log/issues/138/pulls/139/constraint-index.txt").write_text(body)
path = root / "docs/case-studies/issue-138/VERIFICATION.md"
text = path.read_text()
heading = "## Independently selected constraint indexes"
assert heading not in text
path.write_text(text + "\n\n" + heading + "\n\n" + body[body.index("Reproduce with"):])
print(table)
