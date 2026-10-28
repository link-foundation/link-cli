"""Preserve complete Unicode graph results after sequential validation finishes."""
import json
from pathlib import Path

root = Path.cwd()
summary = json.loads((root / 'experiments/issue-138/unicode-graph-summary.json').read_text())
assert summary['native'] == {'passed': 875, 'failed': 0, 'ignored': 14}
assert summary['wasm'] == {'passed': 516, 'failed': 0, 'ignored': 0}
assert summary['csharp_Debug']['passed'] == summary['csharp_Release']['passed'] == 2022
assert summary['js']['pass'] == 177 and summary['repository']['pass'] == 221
rows = []
components = []
for label, key in [('Native Rust', 'native_coverage'), ('Actual WASM, complete linked source', 'wasm_coverage')]:
    def metric(kind):
        v = summary[key][kind]
        return f"{v['covered']:,}/{v['count']:,} ({100 * v['covered'] / v['count']:.2f}%)"
    rows.append(f"| {label} | {metric('lines')} | {metric('branches')} | {metric('functions')} |")
    components.append(label + ': ' + ', '.join(
        f"{v['covered']}/{v['count']} {kind}" for kind, v in summary[key]['checked_walker'].items()))
for mode in ['Debug', 'Release']:
    def metric(kind):
        v = summary['csharp_' + mode]['coverage']
        covered, count = v[kind + '-covered'], v[kind + '-valid']
        return f'{covered:,}/{count:,} ({100 * covered / count:.2f}%)'
    rows.append(f"| C# {mode} | {metric('lines')} | {metric('branches')} | Not reported by this collector |")
    assert all(float(v['line-rate']) == float(v['branch-rate']) == 1 for v in summary['csharp_' + mode]['checked_walker'])
table = '\n'.join(['| Scope | Covered lines | Covered branches | Covered functions |', '| --- | --- | --- | --- |'] + rows)
body = '''Unicode sequence graph integrity: complete local validation
PR: https://github.com/link-foundation/link-cli/pull/139
Date: 2026-10-25 UTC
Base: 4729b539ddd9db9167895fed654282c95789e026

Reproduce with experiments/issue-138/validate-unicode-graph.sh and summarize-
unicode-graph.py after the documented LLVM/wasm-bindgen tooling setup. C#
Debug/Release completed before strict WASM Clippy rejected a nested test module
with the same name as its parent. Renaming only the helper to graph_contract
fixes that composition lint. The resumed driver reuses unchanged completed C#
reports and runs every remaining check; final results use successful reports.
Compilers run sequentially within the 2.92 GiB container budget.

unicode-graph-before.txt preserves genuine before failures: both native
languages hang on a finite self-cycle; Rust silently returns Z for (missing,Z).
Bounded children use memory/CPU limits and parent deadlines and are reaped.
The corrected walkers track the current path with enter/exit frames. Missing
nodes and cycles reject; shared subtrees expand once per occurrence. No traversal
state survives an error. C# reuses its installed Unicode converter through
ISequenceWalker; Rust keeps the Vec-returning public compatibility wrapper and
adds try_walk. Existing UTF-16 and converter memoization policies are preserved.

Portable contracts cover both child directions, self/two-node cycles, repeated
errors, following healthy reads, names, Unicode order, shared DAGs, 2048-level
finite graphs, missing nodes, invalid symbols and UTF-16 policies. Native tests
also verify persistent reopening twice; Rust preserves file bytes on rejected
reads. The C# bounded cases exercise GetString and its public sequence converter.
Actual WASM executes the same portable Rust cases with the maintained test
runner and an explicit 256 MiB test-module linear-memory cap. The independent verify-wasm-memory-cap.py parser
checks the actual memory section of all six executed modules: each declares
a maximum of 4096 64-KiB pages (256 MiB).

Complete local validation:

- Native Rust all-features LLVM coverage: 875 passes, zero failed; fourteen
  ignored child entries are exercised by bounded parent tests. The custom
  stdout harness passes.
- C# Debug and Release with Coverlet: 2022 each, zero failed/skipped.
- Actual WASM with LLVM coverage: 516 passes, zero failed/ignored.
- JS: 177 tests; all eighteen production modules enforce 100% lines, branches
  and functions. Strict TypeScript and 221 repository checks pass.
- Native/WASM formatting and strict Clippy, C# formatting, release fragments,
  source sizes, actionlint, ShellCheck and whitespace checks pass.
- Generated actual-WASM client conformance passes Node, Bun and Deno. The
  universal JS API has no direct typed sequence-graph decoder; no new API is
  added merely for this integrity check.

Official complete-run totals, without filtered denominators or diagnostic unions:

''' + table + '\n\nChecked walker component: ' + '; '.join(components) + '''.
C# Debug/Release class summaries report 100% lines and branches. All three
actual-WASM wrappers retain 100% lines, reported branches and functions.

Whole native Rust, linked WASM and C# remain below the required 100%. This fix
adds O(depth) traversal metadata and does not limit decoded output from valid
DAGs. Cached converter results do not retraverse modified links. No stored-format
change, repair or new power-loss guarantee is supplied. Finer composition,
legacy physical repair, LSM, physical power loss, full journal transfer/replay,
persistent object retention, automatic balancing, multi-catalog transactions
and wider competitor requirements remain unfinished. The goal remains active
and PR 139 remains draft; publication requires fresh exact-head CI.
'''
(root / 'dev/log/issues/138/pulls/139/unicode-graph.txt').write_text(body)
p = root / 'docs/case-studies/issue-138/VERIFICATION.md'
s = p.read_text()
heading = '## Unicode sequence graph integrity\n'
assert heading not in s
section = body.split('\n\nReproduce with ', 1)[1]
p.write_text(s + '\n\n' + heading + '\nReproduce with ' + section)
print(table)
print('\n'.join(components))
