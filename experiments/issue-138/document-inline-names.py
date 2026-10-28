"""Preserve complete official denominators and the independent before evidence."""
import json
from pathlib import Path

root = Path.cwd()
s = json.loads((root / 'experiments/issue-138/inline-names-summary.json').read_text())
assert s['native'] == dict(passed=894, failed=0, ignored=15)
assert s['wasm'] == dict(passed=521, failed=0, ignored=0)
assert s['csharp_Debug']['passed'] == s['csharp_Release']['passed'] == 2027
assert s['js']['pass'] == 177 and s['repository']['pass'] == 221
rows = []
for label, key in [('Native Rust', 'native_coverage'), ('Actual WASM, complete linked source', 'wasm_coverage')]:
    def metric(kind):
        v = s[key][kind]
        return f"{v['covered']:,}/{v['count']:,} ({100*v['covered']/v['count']:.2f}%)"
    rows.append(f"| {label} | {metric('lines')} | {metric('branches')} | {metric('functions')} |")
for mode in ('Debug', 'Release'):
    def metric(kind):
        v = s['csharp_' + mode]['coverage']
        covered, count = v[kind + '-covered'], v[kind + '-valid']
        return f'{covered:,}/{count:,} ({100*covered/count:.2f}%)'
    rows.append(f"| C# {mode} | {metric('lines')} | {metric('branches')} | Not reported by this collector |")
table = '\n'.join(['| Scope | Covered lines | Covered branches | Covered functions |', '| --- | --- | --- | --- |'] + rows)
component = s['native_coverage']['text_storage_format']
assert all(value['count'] == value['covered'] for value in component.values())
body = '''Exact native text names and atomic reload: complete local validation
PR: https://github.com/link-foundation/link-cli/pull/139
Date: 2026-10-26 UTC
Before implementation: faf191d10f1dc6fab561dd46dafb0d67561e0963

Reproduce with experiments/issue-138/validate-inline-names.sh, summarize-inline-
names.py and document-inline-names.py after setup-resumed-validation.sh. Heavy
compilers run sequentially, Rust uses one build job and .NET heaps are bounded.

The seven initial automated tests all fail before the correction; see
inline-names-before.txt. Whitespace truncates, quote edges disappear, a line
break creates an additional row, absent-address names vanish and invalid UTF-8
after a valid row discards the old live state during reload. The complete-name
and failed-reload assertions now pass, using both plain and selected indexes.

Four additional native contracts verify independent exact metadata bytes,
eighteen malformed/future cases with both index settings, following rename,
name movement/removal, deletion/reuse, legacy optional delimiters and repeated
reopening of 64 finite mixed-Unicode names. Names include empty strings, NUL,
CR/LF/tab/DEL, quotes, backslashes, literal-looking syntax, BOM and Unicode.
A portable memory-reload control also runs in actual WASM. Existing Unicode
names, history, versioning, query and output contracts remain in the full suites.
The runnable rust/examples/text_names.rs passes separately after the suite
with --no-default-features, including exact multiline/Unicode/zero-address names.

A separate existing TCP race test failed during initial full C# Release
validation. tcp-shutdown-before.txt preserves the actual NullReferenceException
in TcpListener.AcceptTcpClient. Commit 7e882d7 handles that known runtime race
only after shutdown starts. Both complete C# configurations then pass the same
200-connection regression without extending timeouts; TCP-SHUTDOWN-PLAN.md traces
its primary runtime source and records the finite scheduling limit.

Complete local validation:
- Native Rust all-features LLVM coverage: 894 passed, zero failed; fifteen
  ignored child entries are exercised by bounded parents. Custom stdout passes.
- C# Debug and Release with Coverlet: 2027 each, zero failed/skipped.
- Actual WASM with LLVM coverage: 521 passed, zero failed/ignored; six executed
  modules declare the independently verified 256 MiB linear-memory cap.
- JS: 177 tests and all eighteen modules at enforced 100% lines/branches/functions;
  strict TypeScript and 221 repository checks pass.
- Native/WASM/C# formatting, strict native/WASM Clippy, release fragments,
  source sizes, ShellCheck, actionlint and whitespace checks pass.
- Node, Bun and Deno generated actual-WASM client conformance passes.

Official complete-run coverage, with the whole project denominator:

''' + table + '\n\nNative text format component: ' + ', '.join(
    f"{value['covered']}/{value['count']} {kind}" for kind, value in component.items()) + '''.
All three actual-WASM wrappers retain 100% measured lines/branches/functions.
The native text-file parser is not exercised through a WASM filesystem; its
linked-source coverage is retained in the complete WASM denominator.

The versioned comment namespace reuses the maintained UTF-8 literal codec;
legacy unquoted ~1{31} and raw backslashes retain literal meanings. Safe quoted
rows retain their readable representation. Older readers ignore the extension
and can lose its names when saving: updated readers are required. Metadata does
not allocate rows. Invalid/future metadata rejects without rewriting the file.
Reload holds the old and candidate states until complete load/index construction.
Parsing/encoding are linear in bytes; M metadata entries add O(M log M) sorting.
The same replacement/flush protocol and cooperating writer ownership remain;
no physical repair or stronger save() durability is claimed. TEXT-STORAGE.md
specifies compatibility, persistence assumptions and the exact example bytes.

Whole-project Rust/C#/linked-WASM coverage remains below 100%. Native physical
repair, page undo, LSM, finer composition, full journal replay/updates, persistent
object retention, balancing, multi-catalog transactions and the other inventoried
acceptance criteria remain unfinished. Keep the goal active and PR draft;
publication requires fresh exact-head CI. Native page prototype experiments are
separate future implementation evidence, not part of these production results.
'''
(root / 'dev/log/issues/138/pulls/139/inline-names.txt').write_text(body)
p = root / 'docs/case-studies/issue-138/VERIFICATION.md'
text = p.read_text(); heading = '## Exact native text names and atomic reload\n'
assert heading not in text
p.write_text(text + '\n\n' + heading + '\n' + body)
print(table)
