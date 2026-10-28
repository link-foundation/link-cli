#!/usr/bin/env python3
"""Verify batch receipts and preserve complete production coverage denominators."""
import hashlib
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[1]
REPORTS = []


def path(suffix, extension="log"):
    return BASE / f"causal-batch-{suffix}.{extension}"


def log(suffix):
    return path(suffix).read_text()


def counts(suffix, expected, coverage=False):
    text = log(suffix)
    totals = {key: int(re.search(r"(?:#|ℹ) " + key + r" (\d+)\b", text)[1])
              for key in ("tests", "pass", "fail", "skipped", "cancelled")}
    assert totals["tests"] == totals["pass"] == expected, (suffix, totals)
    assert totals["fail"] == totals["skipped"] == totals["cancelled"] == 0
    print(f"{suffix}: {totals}.")
    if coverage:
        assert re.search(r"all files\s+\| 100\.00\s+\|\s+100\.00\s+\|\s+100\.00", text)
        print("100% production lines, branches and functions; unchanged thresholds and complete denominators.")


print("PR: https://github.com/link-foundation/link-cli/pull/139")
print("Reproduce: bash experiments/issue-138/validate-causal-batch.sh")
print("The driver runs the unchanged complete-production-coverage Node command from .github/workflows/js-client.yml.")
for suffix, diagnostic in (("api-before", "ERR_MODULE_NOT_FOUND"), ("native-before", "E0432"),
                           ("csharp-before", "CS0246"), ("ownership-before", "changed after admission"),
                           ("physical-generation-before", "'3' !== '4'")):
    assert diagnostic in log(suffix), suffix
print("Test-first missing APIs: Node ERR_MODULE_NOT_FOUND, native E0432, C# CS0246.")
print("JS payload mutation across the first await reproduced before admission owned it.")
print("The physical exit matrix first exposed an incorrect example generation assertion.")
print("Physical LSM changes only changed keys: the empty receipt advances metadata, not the owner.")
print("All three examples now use that actual contract; a portable real-WAL/table unit control also passes.")
assert "30" in log("model") and "PASS" in log("model")
fixtures = json.loads((ROOT / "docs/protocol/causal-batch-v1.json").read_text())
assert len(fixtures["cases"]) == 53
assert (ROOT / "docs/protocol/causal-batch-v1.json").read_bytes() == path("vectors-regenerated", "json").read_bytes()
print("Independent finite model: 30 bounded scenarios; 53 independently generated shared journal-admission vectors.")
for suffix in ("native-focused", "wasm-focused"):
    assert "test result: ok. 7 passed; 0 failed; 0 ignored;" in log(suffix), suffix
assert re.search(r"Passed:\s+7, Skipped:\s+0, Total:\s+7", log("csharp-focused"))
assert "15 pass" in log("bun") and "0 fail" in log("bun")
assert "15 passed | 0 failed" in log("deno")
counts("node-focused", 15)
print("Seven focused native/C#/actual-WASM contracts and fifteen Node/Bun/Deno contracts pass.")
assert "PASS 50 batch complete-store handoffs with physical hash preservation" in log("physical-handoffs")
physical = log("physical-faults")
assert "PASS 20 reaped process exits; 100 cross-runtime retry/reopen/following checks" in physical
assert len(re.findall(r"^PASS batch (?:plain|mirrored) \w+ exit-(?:before|after) ->", physical, re.M)) == 100
print("SQL-free plain/mirrored files: 50 complete-store handoffs across Rust, C#, Node, Bun and Deno.")
print("Twenty exit-73 children are reaped before explicit marker quarantine; 100 blocked opens preserve evidence.")
print("All 100 cross-runtime recovered retries, following writes, empty receipts and return reads pass with file-hash guards.")
print("Application process exits do not certify hardware power loss.")
assert re.search(r"^ALL PASS \S+", log("validation"), re.M)
print(log("validation").strip())
print("Driver interruptions from editing active Bash scripts are retained. The completed format/focused and Debug evidence is preserved; overwritten Release output is rerun from an in-memory script snapshot before completing compiler/WASM validation.")
assert log("dotnet-sdk").strip() == "10.0.403"
print("C# format/build/test use SDK 10.0.403, matching CI. The ambient 10.0.112 formatter's whitespace-only diagnostic is retained separately.")
assert "87.5% (7/8)" in log("null-token-before")
assert re.search(r"^CSHARP NULL-TOKEN PASS \S+", log("validation"), re.M)
print("Initial Release coverage exposed one untested null retry-token path (7/8 outcomes). The existing admission contract now includes it; complete C# validation is repeated without changing production code.")
assert "true=0, false=334" in log("observation-limit-before")
assert re.search(r"^ADMISSION PASS \S+", log("validation"), re.M)
assert "test result: ok. 7 passed; 0 failed; 0 ignored;" in log("native-admission")
before_path = path("native-before-observation-limit", "json")
before = json.loads(before_path.read_text())["data"][0]
after = json.loads(path("native-coverage", "json").read_text())["data"][0]
for metric in ("lines", "branches", "functions", "regions", "instantiations"):
    assert before["totals"][metric]["count"] == after["totals"][metric]["count"], metric
provenance_path = path("admission-production", "json")
provenance = json.loads(provenance_path.read_text())
for source, expected in provenance.items():
    assert hashlib.sha256((ROOT / source).read_bytes()).hexdigest() == expected, source
REPORTS.extend([before_path, provenance_path, path("wasm-before-observation-limit", "json")])
print("Native full-suite profiles are supplemented with seven focused contracts using --no-clean after adding the 65-observation control. All complete production denominators and recorded production source hashes are unchanged.")
print("The actual-WASM full suite is repeated after the same portable test addition.")
for mode in ("Debug", "Release"):
    output = log(f"csharp-{mode}-full")
    match = re.search(r"Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)", output)
    assert match and match[1] == match[2] == "2491", (mode, match)
    report_path = Path(re.findall(r"^\s+(\S+coverage\.cobertura\.xml)$", output, re.M)[-1])
    REPORTS.append(report_path)
    report = ET.parse(report_path).getroot()
    print(f"C# {mode}: 2491 passed, zero failed/skipped.")
    for metric in ("lines", "branches"):
        covered, count = [int(report.attrib[f"{metric}-{key}"]) for key in ("covered", "valid")]
        print(f"Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%).")
    for filename in ("CausalVersion.cs", "CausalReplica.cs", "CausalMerkle.cs", "CausalBatch.cs"):
        classes = [c for c in report.findall(".//class") if c.attrib["filename"].endswith("/" + filename)]
        assert classes
        line_items = [item for cls in classes for item in cls.findall("./lines/line")]
        lines = {int(item.attrib["number"]) for item in line_items}
        branches = [tuple(map(int, re.search(r"\((\d+)/(\d+)\)", line.attrib["condition-coverage"]).groups()))
                    for line in line_items if "condition-coverage" in line.attrib]
        assert all(int(line.attrib["hits"]) > 0 for line in line_items), (mode, filename)
        assert all(covered == count for covered, count in branches), (mode, filename)
        print(f"{filename} lines: {len(lines)}/{len(lines)}; branches: {sum(c for c, _ in branches)}/{sum(n for _, n in branches)}.")
for title, stem, expected in (("Native Rust", "native", 1226), ("Actual WASM", "wasm", 752)):
    results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", log(f"{stem}-full"))
    assert results and sum(int(p) for p, _, _ in results) == expected, (title, results)
    assert all(int(f) == 0 for _, f, _ in results)
    print(f"{title}: {expected} passed, zero failed; {sum(int(i) for _, _, i in results)} existing parent-driven entries ignored by normal harness.")
    report_path = path(f"{stem}-coverage", "json")
    REPORTS.append(report_path)
    report = json.loads(report_path.read_text())["data"][0]
    for metric in ("lines", "branches", "functions", "regions", "instantiations"):
        value = report["totals"][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
    component, = [item for item in report["files"] if item["filename"].endswith("/rust/src/engine/causal_batch.rs")]
    branch, = [value for value in component["branches"] if value[0] == 64 and value[1] == 44
               and (value[4] or value[5])]
    assert branch[4] > 0 and branch[5] > 0, (title, branch)
    for metric in ("lines", "branches", "functions", "regions", "instantiations"):
        value = component["summary"][metric]
        print(f"causal_batch.rs {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
assert "test result: ok. 6 passed; 0 failed; 1 ignored;" in log("doctests")
print("Separate doctests: six passed, one existing ignored example.")
counts("js-full", 681, True)
counts("repository", 221)
assert not re.search(r"serde_json|rusqlite|sha2", log("bare-dependencies"))
assert not re.search(r"rusqlite|sha2", log("selected-dependencies"))
assert "Checked 127 declared versions and the Cargo.lock files." in log("dependency-freshness")
print("Strict native/WASM Clippy, formats, type declarations, workflow/ShellCheck, source sizes, one changeset and 127 fresh dependency declarations/locks pass.")
print("Bare Rust normal dependencies omit JSON, SQLite and SHA; selecting causal-batches adds no SQLite or SHA. JS uses a separate export.")
print("Both journals remain independently bounded at 4096 entries/8 MiB; records 1 MiB, 64 edits, 64 observations per edit, seven receipt array levels/32768 separators.")
print("One coherent caller-owned store publishes complete owner and receipt journals at one CAS. All mutations stage privately before it.")
print("Retry identity includes original owner generation and normalized edits; complete supplied historical evidence is still validated.")
print("Complete retained replay/copying costs O((E+1)H²W); this is not an incremental scalable index.")
print("Local batch atomicity does not survive independent per-row remote transfer as a global atomic group.")
print("Ordinary links/name projection and sessions, global allocation, safe membership/history reclamation, generic mapped failures, full project 100% and all wider unmet R1-R12 criteria remain unfinished.")
print("Fresh publication/platform CI remains required; PR139 stays draft and unmerged.")
suffixes = ("api-before", "native-before", "csharp-before", "ownership-before", "physical-generation-before",
            "model", "validation", "node-focused", "js-full", "native-focused", "wasm-focused", "csharp-focused",
            "bun", "deno", "physical-handoffs", "physical-faults", "csharp-Debug-full", "csharp-Release-full",
            "native-full", "doctests", "wasm-full", "repository", "dependency-freshness", "workflows",
            "dotnet-sdk", "csharp-format-sdk112-before", "null-token-before",
            "observation-limit-before", "native-admission", "admission-format", "csharp-driver-before")
sources = ("js/client/src/engine-causal-batch.js", "js/client/test/engine-causal-batch.test.mjs",
           "rust/src/engine/causal_batch.rs", "rust/tests/engine_causal_batch_tests.rs",
           "csharp/Foundation.Data.Doublets.Cli.Library/Engine/CausalBatch.cs",
           "csharp/Foundation.Data.Doublets.Cli.Tests/CausalBatchTests.cs", "docs/protocol/causal-batch-v1.json")
for report_path in [*REPORTS, *(path(suffix) for suffix in suffixes), *(ROOT / source for source in sources)]:
    data = report_path.read_bytes()
    print(f"{report_path.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.")
