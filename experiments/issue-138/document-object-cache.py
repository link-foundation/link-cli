"""Preserve verified complete cache results; run after both validation scripts."""
import json
from pathlib import Path

root = Path.cwd()
summary = json.loads((root / "experiments/issue-138/object-cache-summary.json").read_text())
assert summary["native"] == {"passed": 866, "failed": 0, "ignored": 13}
assert summary["wasm"] == {"passed": 509, "failed": 0, "ignored": 0}
assert summary["csharp_Debug"]["passed"] == summary["csharp_Release"]["passed"] == 2016
assert summary["js"]["pass"] == 177 and summary["repository"]["pass"] == 221
api_progress = (root / "experiments/issue-138/object-cache-csharp-api-validation.log").read_text()
assert api_progress.endswith("Final C# Release with original constructor ABI and coverage: passed\n")
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
components = []
for label, key in [("Native Rust", "native_coverage"), ("Actual WASM", "wasm_coverage")]:
    component = summary[key]["object_cache"]
    components.append(label + ": " + ", ".join(
        f"{value['covered']}/{value['count']} {kind}" for kind, value in component.items()))
for mode in ("Debug", "Release"):
    assert all(float(entry["line-rate"]) == float(entry["branch-rate"]) == 1
        for entry in summary["csharp_" + mode]["object_cache"])
body = """Optional immutable journal object cache: complete local validation
PR: https://github.com/link-foundation/link-cli/pull/139
Date: 2026-10-25 UTC
Base: 864839b3e3ec0e92d8783dc82ca3bff4996b1be0

Reproduce with experiments/issue-138/validate-object-cache.sh after the
documented tools/etcd setup. The final constructor compatibility correction
was made after initial C# builds began: validate-object-cache-csharp-api.sh
reruns formatting and both full configurations with the corrected source.
summarize-object-cache.py and this evidence use only final successful reports.
Compilers run sequentially within the 2.92 GiB container limit.

The independently selected zero-default budget retains checked immutable
objects by partition/address/object generation. Every catalog snapshot remains
fresh. Returned arrays are independent; publication/staging still rereads and
compares complete data. Failed lookups never enter the cache; oversized objects
do not evict retained entries. Positive object/record overhead bounds empty
payloads. The existing C# three-argument constructor retains its binary signature.
Rust reuses audited lru 0.19.0 with default features disabled; C#/JS use standard
containers. No cache container or retained payload copy is allocated at zero.

object-cache-before.txt preserves absent-API Rust/C# failures and JS download
counter failures before implementation. These establish missing optional costs,
not an isolation defect. The first LRU test sequence accidentally reread a before
probing eviction; the corrected a,b,a,c,b,c,a sequence measures promotion and
eviction without that extra promotion. Final tests use the corrected oracle.
Each language independently compares 768 reads across six budgets with explicit
UTF-8 payload weights. Tests additionally cover empty objects, independent data
partitions, caller mutation, fresh catalog generations/removal/failure, wrong
object generations, corrupt fetch/repair, cached-data outages, staging collision
checks and rejected/failed/lost-response publication. C#/JS force overlapping
misses and verify one retained copy and correct eviction accounting.

Complete local validation:

- Native Rust all-features coverage: 866 standard passes, zero failed;
  thirteen ignored child entries are exercised by bounded parent tests.
  The custom stdout harness passes.
- C# Debug and Release with Coverlet: 2,016 each, zero failed/skipped.
- Actual WASM with LLVM coverage: 509 pass, zero failed/ignored, including
  every portable cache contract.
- JS: 177 tests; all eighteen production modules enforce 100% lines,
  branches and functions. Strict TypeScript and 221 repository checks pass.
- Formatting, strict native/WASM Clippy, source sizes, release fragments,
  actionlint/ShellCheck, whitespace and 118 latest declared dependencies pass.
- Generated actual-WASM conformance passes Node, Bun and Deno.
- Real etcd 3.7.3 interoperability passes with default zero and 65,536 budgets:
  Rust/C#/WASM use the same catalog and separate east/west data groups. A
  repeated cached WASM begin issues one catalog request and zero data requests.
  Atomic conflicts, lost responses, unavailable west/no publication, independent
  east commits, cached west reads during its data outage, catalog leader failover
  and majority-loss rejection even for cached shards all pass.
  Five finite etcd children are stopped and reaped after each run.

Official complete-run totals, without filtered denominators or diagnostic unions:

""" + table + "\n\nObjectCache component: " + "; ".join(components) + """.
C# Debug/Release class summaries report 100% lines and branches. All three
actual-WASM wrappers report 100% lines, branches and functions.

Whole native Rust, linked WASM and C# remain below the required 100%. Cache
payload weights do not measure process heap. Complete begin replay, updated
journal transfers and persistent old/orphan objects are unchanged. Immutable
namespaces must not be rewritten/deleted; cached hits cannot detect later
out-of-protocol tampering. Catalog availability remains required. Finer
composition, legacy physical repair, LSM, real power loss, automatic balancing,
multi-catalog transactions and wider competitor requirements remain unfinished.
The issue goal stays active and PR 139 stays draft; new code needs exact-head CI.
"""
(root / "dev/log/issues/138/pulls/139/object-cache.txt").write_text(body)
path = root / "docs/case-studies/issue-138/VERIFICATION.md"
text = path.read_text()
heading = "## Optional immutable journal object cache"
assert heading not in text
path.write_text(text + "\n\n" + heading + "\n\n" + body[body.index("Reproduce with"):])
print(table)
