"""Keep complete final results, including partial component/project coverage."""
import json
from pathlib import Path

root=Path.cwd()
s=json.loads((root/'experiments/issue-138/structure-depth-summary.json').read_text())
assert s['native']=={'passed':882,'failed':0,'ignored':15}
assert s['wasm']=={'passed':520,'failed':0,'ignored':0}
assert s['csharp_Debug']['passed']==s['csharp_Release']['passed']==2027
assert s['js']['pass']==177 and s['repository']['pass']==221
rows=[]
components=[]
for label,key in [('Native Rust','native_coverage'),('Actual WASM, complete linked source','wasm_coverage')]:
 def metric(kind):
  v=s[key][kind]
  return f"{v['covered']:,}/{v['count']:,} ({100*v['covered']/v['count']:.2f}%)"
 rows.append(f"| {label} | {metric('lines')} | {metric('branches')} | {metric('functions')} |")
 components.append(label+': '+', '.join(f"{v['covered']}/{v['count']} {kind}" for kind,v in s[key]['structure_formatter'].items()))
for mode in ['Debug','Release']:
 def metric(kind):
  v=s['csharp_'+mode]['coverage']; covered,count=v[kind+'-covered'],v[kind+'-valid']
  return f'{covered:,}/{count:,} ({100*covered/count:.2f}%)'
 rows.append(f"| C# {mode} | {metric('lines')} | {metric('branches')} | Not reported by this collector |")
table='\n'.join(['| Scope | Covered lines | Covered branches | Covered functions |','| --- | --- | --- | --- |']+rows)
body='''Deep structure output: complete local validation
PR: https://github.com/link-foundation/link-cli/pull/139
Date: 2026-10-26 UTC
Recovered implementation: 500ec8f951b4e7ebf5904488400b11acecacca29

Reproduce with experiments/issue-138/validate-structure-depth.sh and summarize-
structure-depth.py after the documented LLVM/wasm-bindgen tooling setup. Heavy
compilers run sequentially with one Rust build job and bounded .NET heaps.

structure-depth-before.txt preserves real automated failures: a finite 2048-node
source chain overflows isolated native stacks in concrete Rust, generic Rust
and C#. The child processes have finite memory/CPU limits, parent deadlines and
dumps disabled; all are reaped. Corrected entry points use explicit frames and
one growable output buffer. Target references remain unexpanded. Both Rust
entry points reuse one formatter, and C# uses its standard StringBuilder.

Independent shallow oracles cover names/quoting, self/two-node source cycles,
shared children, missing roots, exact deepest-first name reads and failures at
every reference position. Rust also verifies caller-provided visited sets and
error path progress. Native bounded tests verify exact output at depth 2048;
portable Rust cases do the same in actual WASM. Ordinary Unicode named stores
reopen twice in both native languages. The independent existing output suites
also pass. A broader initial C# test fixture aborted in usage repointing when
merging an existing cyclic pair during setup; creating a separate self-cycle
corrects the fixture. That execution did not reach the output formatter.

Complete local validation:
- Native Rust all-features LLVM coverage: 882 passes, zero failed; fifteen
  ignored child entries are exercised by bounded parents. Custom stdout passes.
- C# Debug and Release with Coverlet: 2027 each, zero failed/skipped.
- Actual WASM with LLVM coverage: 520 passes, zero failed/ignored; all six
  executed modules declare the independently verified 256 MiB linear-memory cap.
- JS: 177 tests and all eighteen modules at enforced 100% lines/branches/functions.
  Strict TypeScript and 221 repository checks pass.
- Native/WASM/C# formatting, strict native/WASM Clippy, release fragments,
  source sizes, ShellCheck, actionlint and whitespace checks pass.
- Node, Bun and Deno generated actual-WASM client conformance passes.

Official complete-run coverage (no filtered project denominator):

'''+table+'\n\nShared Rust formatter component: '+'; '.join(components)+'''.
All three actual-WASM wrappers retain 100% lines/reported branches/functions.
The C# output-file component includes its other existing output methods; inspect
its complete Coverlet report rather than treating file coverage as method scope.

Metadata costs O(depth) plus resolved reference text and output. Assembly is
linear in output bytes; store lookups and name decoding retain their own costs.
No decoded-output limit, new stored representation, repair or stronger physical
durability is added. A separate observed legacy Rust inline text-name whitespace
truncation remains for the next atomic audit in this same PR; this traversal's
persistent controls use the ordinary Unicode named store.

Whole-project native Rust, C# and linked WASM coverage remain below 100%. Finer
composition, legacy physical repair, LSM, real power loss, full journal replay,
persistent object retention, balancing, multi-catalog transactions and wider
competitor requirements remain unfinished. Keep the goal active and PR draft;
publication requires fresh exact-head CI.
'''
(root/'dev/log/issues/138/pulls/139/structure-depth.txt').write_text(body)
p=root/'docs/case-studies/issue-138/VERIFICATION.md'; text=p.read_text()
heading='## Deep structure output\n'; assert heading not in text
p.write_text(text+'\n\n'+heading+'\nReproduce with '+body.split('\n\nReproduce with ',1)[1])
print(table)
print('\n'.join(components))
