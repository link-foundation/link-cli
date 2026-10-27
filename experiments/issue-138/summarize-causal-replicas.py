#!/usr/bin/env python3
"""Verify actual local-owner tests and retain complete production denominators."""
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path

BASE = Path(__file__).resolve().parent
PREFIX = "causal-replica"


def log(suffix):
    return (BASE / f"{PREFIX}-{suffix}.log").read_text()


print("PR: https://github.com/link-foundation/link-cli/pull/139")
print("Reproduce: bash experiments/issue-138/validate-causal-replicas.sh")
assert "ERR_MODULE_NOT_FOUND" in log("before")
assert "E0432" in log("rust-before") and "CS0246" in log("csharp-before")
assert "inadmissible frame reached JSON.parse" in log("framing-before")
assert "Missing expected rejection" in log("observed-history-before")
print("Test-first evidence: absent public APIs, preparse frame admission and dominated historical observation rejection; causal-replica-before.txt retains exact diagnostics and log hashes.")
assert re.search(r"^ALL PASS \S+", log("validation"), re.M)
print(log("validation").strip())
fixtures = json.loads((BASE.parents[1] / "docs/protocol/causal-replica-v1.json").read_text())
assert len(fixtures["cases"]) == 29
print("Shared independent journal vectors: 29, including full dominated-observation evidence and historical contradiction rejection after reopening.")
assert "test result: ok. 8 passed; 0 failed; 0 ignored;" in log("rust-focused")
assert "test result: ok. 8 passed; 0 failed; 0 ignored;" in log("wasm-focused")
assert re.search(r"Passed:\s+13, Skipped:\s+0, Total:\s+13", log("csharp-focused"))
assert "13 pass" in log("bun") and "0 fail" in log("bun")
assert "13 passed | 0 failed" in log("deno")
print("Eight local-owner contracts pass in native Rust, C#, Node, Bun, Deno and actual WASM. C#/Bun/Deno also run the five semantic version contracts.")
assert "PASS 50 causal complete-store handoffs with physical hash preservation" in log("physical-handoffs")
assert "PASS 20 reaped process exits; 100 cross-runtime retry/reopen/following checks" in log("process-faults")
print("Actual SQL-free physical directory: all five writers/readers, plain and mirrored; 50 complete-store handoffs preserve read hashes.")
print("Actual process exits: 20 before/after publication cuts, 100 stale-owner refusals without changed bytes, and 100 recovered cross-runtime retry/following/reopen handoffs.")
print("The operator first reaps each child, then quarantines ownership markers outside the store. No live-owner stealing or hardware power-loss certification is claimed.")
for mode in ("Debug", "Release"):
    output = log(f"csharp-{mode}-full")
    match = re.search(r"Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)", output)
    assert match and match[1] == match[2] == "2475", (mode, match)
    path = Path(re.findall(r"^\s+(\S+coverage\.cobertura\.xml)$", output, re.M)[-1])
    report = ET.parse(path).getroot()
    print(f"C# {mode}: 2475 passed; zero failed/skipped.")
    for metric in ("lines", "branches"):
        covered, count = [int(report.attrib[f"{metric}-{key}"]) for key in ("covered", "valid")]
        print(f"Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%).")
    for filename in ("CausalVersion.cs", "CausalReplica.cs"):
        classes = [item for item in report.findall(".//class") if item.attrib["filename"].endswith("/" + filename)]
        assert classes, (mode, filename)
        lines = {int(item.attrib["number"]): item for cls in classes for item in cls.findall("./lines/line")}
        print(f"{filename} executable lines: {sum(int(line.attrib['hits']) > 0 for line in lines.values())}/{len(lines)}.")
        branches = [tuple(map(int, re.search(r"\((\d+)/(\d+)\)", line.attrib["condition-coverage"]).groups()))
                    for line in lines.values() if "condition-coverage" in line.attrib]
        print(f"{filename} branches: {sum(covered for covered, _ in branches)}/{sum(count for _, count in branches)}.")
for title, stem, expected in [("Native Rust", "native", 1210), ("Actual WASM", "wasm", 736)]:
    results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", log(f"{stem}-full"))
    assert results and sum(int(p) for p, _, _ in results) == expected, (title, results)
    assert all(int(f) == 0 for _, f, _ in results)
    print(f"{title}: {expected} passed; {sum(int(i) for _, _, i in results)} parent-driven child entries ignored by normal harness.")
    report = json.loads((BASE / f"{PREFIX}-{stem}-coverage.json").read_text())["data"][0]
    for metric in ("lines", "branches", "functions", "regions", "instantiations"):
        value = report["totals"][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
    component, = [item for item in report["files"] if item["filename"].endswith("/rust/src/engine/causal_replica.rs")]
    for metric in ("lines", "branches", "functions"):
        value = component["summary"][metric]
        print(f"causal_replica.rs {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
assert "test result: ok. 6 passed; 0 failed; 1 ignored;" in log("doctests")
print("Doctests: six passed; one existing ignored example.")
for suffix, expected in [("js-full", 657), ("js-focused", 8), ("repository", 221)]:
    output = log(suffix)
    totals = {key: int(re.search(r"(?:#|ℹ) " + key + r" (\d+)\b", output)[1]) for key in ("tests", "pass", "fail", "skipped")}
    assert totals["tests"] == totals["pass"] == expected and totals["fail"] == totals["skipped"] == 0, (suffix, totals)
    print(f"{suffix}: {totals}.")
    if suffix != "repository":
        assert re.search(r"all files\s+\| 100\.00\s+\|\s+100\.00\s+\|\s+100\.00", output)
        print("Production coverage: 100% lines, branches and functions; no reduced thresholds or exclusions.")
assert not re.search(r"serde_json|rusqlite", log("bare-dependencies"))
print("Formatting, strict native/WASM Clippy, TS declarations, source sizes, one C# changeset, dependency freshness and workflow/ShellCheck checks pass.")
print("Bare Rust normal dependencies contain neither serde_json nor rusqlite. Native physical composition uses no SQLite feature; JS remains a separate export.")
print("Admission: 4096 entries/8 MiB total; 1 MiB/five array levels/32768 unquoted separators per record before parsing. The maximum 64-version/64-author/4096-byte-name shape passes.")
print("Every operation replays the full bounded journal; historical comparisons cost O(H²W), retained state O(HW+R), journal publication copy O(RW). No safe receipt/history compaction is supplied.")
print("Author uniqueness across independent stores and backup rollback fencing remain deployment requirements. The caller owns and closes the provider.")
print("Ordinary links projection/session facade, bounded Merkle/range transport, reclamation, full project/linked-WASM 100% and every other unmet R1-R12 criterion remain unfinished.")
platforms = (BASE / "causal-replica-csharp-platform-coverage.txt").read_text()
assert platforms.count("2475 tests pass, platform summary present;") == 3
assert "d00e02232812346ace3525efb3525fc42fd6d17e" in platforms
print("Published d00e022 C# CI passes 2475 tests on each platform; all three real archives verify size and SHA256. See causal-replica-csharp-platform-coverage.txt for complete platform denominators.")
print("The remaining exact-head Rust platform run is still pending at this receipt. The PR stays draft and unmerged.")
