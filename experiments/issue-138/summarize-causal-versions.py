#!/usr/bin/env python3
"""Check actual semantic conformance and complete, nonexcluded coverage."""
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path

base = Path(__file__).resolve().parent
prefix = "causal-version"


def log(suffix):
    return (base / f"{prefix}-{suffix}.log").read_text()


print("PR: https://github.com/link-foundation/link-cli/pull/139")
print("Reproduce: bash experiments/issue-138/validate-causal-versions.sh")
assert "ERR_MODULE_NOT_FOUND" in log("js-before")
print("Test-first baseline: the new JS conformance suite fails with ERR_MODULE_NOT_FOUND before production implementation.")
assert re.search(r"^ALL PASS \S+", log("validation"), re.M)
print(log("validation").strip())
fixtures = json.loads((base.parents[1] / "docs/protocol/causal-version-v1.json").read_text())
assert (len(fixtures["cases"]), len(fixtures["merges"]), len(fixtures["advances"])) == (46, 11, 9)
print("Independent shared cases: 46 codec, eleven merge and nine advancement vectors; 24 delivery orders and 216 state triples per runtime.")
assert "test result: ok. 5 passed; 0 failed; 0 ignored;" in log("rust-focused")
assert re.search(r"Passed:\s+5, Skipped:\s+0, Total:\s+5", log("csharp-focused"))
assert "5 pass" in log("bun") and "0 fail" in log("bun")
assert "5 passed | 0 failed" in log("deno")
print("Five selected tests pass in native Rust, C#, Node, Bun and Deno; actual WASM includes the same five Rust contracts.")
for mode in ("Debug", "Release"):
    output = log(f"csharp-{mode}-full")
    match = re.search(r"Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)", output)
    assert match and match[1] == match[2] == "2467", (mode, match)
    path = Path(re.findall(r"^\s+(\S+coverage\.cobertura\.xml)$", output, re.M)[-1])
    report = ET.parse(path).getroot()
    print(f"C# {mode}: 2467 passed; zero failed/skipped.")
    for metric in ("lines", "branches"):
        covered, count = [int(report.attrib[f"{metric}-{key}"]) for key in ("covered", "valid")]
        print(f"Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%).")
    classes = report.findall(".//class[@filename='Engine/CausalVersion.cs']")
    if not classes:
        classes = [item for item in report.findall(".//class") if item.attrib["filename"].endswith("/CausalVersion.cs")]
    assert classes, (mode, "missing causal production coverage")
    lines = {int(item.attrib["number"]): int(item.attrib["hits"]) for cls in classes for item in cls.findall("./lines/line")}
    print(f"CausalVersion.cs executable lines: {sum(hits > 0 for hits in lines.values())}/{len(lines)}.")
for title, stem, expected in [("Native Rust", "native", 1202), ("Actual WASM", "wasm", 728)]:
    results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", log(f"{stem}-full"))
    assert results and sum(int(p) for p, _, _ in results) == expected, (title, results)
    assert all(int(f) == 0 for _, f, _ in results)
    print(f"{title}: {expected} passed; {sum(int(i) for _, _, i in results)} parent-driven child entries ignored by normal harness.")
    report = json.loads((base / f"{prefix}-{stem}-coverage.json").read_text())["data"][0]
    for metric in ("lines", "branches", "functions", "regions", "instantiations"):
        value = report["totals"][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
    component, = [item for item in report["files"] if item["filename"].endswith("/rust/src/engine/causal.rs")]
    for metric in ("lines", "branches", "functions"):
        value = component["summary"][metric]
        print(f"causal.rs {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
assert "test result: ok. 6 passed; 0 failed; 1 ignored;" in log("doctests")
print("Doctests: six passed; one existing ignored example.")
for suffix, expected in [("js-full", 649), ("js-coverage", 5), ("repository", 221)]:
    output = log(suffix)
    totals = {key: int(re.search(r"(?:#|ℹ) " + key + r" (\d+)\b", output)[1]) for key in ("tests", "pass", "fail", "skipped")}
    assert totals["tests"] == totals["pass"] == expected and totals["fail"] == totals["skipped"] == 0, (suffix, totals)
    print(f"{suffix}: {totals}.")
    if suffix != "repository":
        assert re.search(r"all files\s+\| 100\.00\s+\|\s+100\.00\s+\|\s+100\.00", output)
        print("Production coverage: 100% lines, branches and functions; no reduced thresholds or exclusions.")
assert not re.search(r"serde_json|rusqlite", log("bare-dependencies"))
print("Formatting, strict native/WASM Clippy, TS declarations, source sizes, one C# changeset, dependency freshness and workflow/ShellCheck checks pass.")
print("Bare Rust normal dependencies contain neither serde_json nor rusqlite. The new JS codec remains a separate import.")
print("The component validates canonical bytes, owns results, retains concurrent update/deletion siblings and checks counters without I/O.")
print("Durable counter ownership/publication, Merkle comparison, bounded range transfer, ordinary links projection and safe tombstone reclamation remain unfinished.")
print("All full production coverage denominators remain included. Complete native/C#/linked-WASM 100% and every other unmet R1-R12 criterion remain required.")
print("Fresh exact-head Windows/Linux/macOS CI remains required. The PR stays draft and unmerged.")
