#!/usr/bin/env python3
"""Check retained Merkle results without dropping any production denominator."""
import hashlib
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[1]
PREFIX = "causal-merkle"
REPORTS = []


def log(suffix):
    return (BASE / f"{PREFIX}-{suffix}.log").read_text()


def tests(suffix, expected):
    output = log(suffix)
    totals = {key: int(re.search(r"(?:#|ℹ) " + key + r" (\d+)\b", output)[1])
              for key in ("tests", "pass", "fail", "skipped")}
    assert totals["tests"] == totals["pass"] == expected
    assert totals["fail"] == totals["skipped"] == 0
    print(f"{suffix}: {totals}.")
    if suffix != "repository":
        assert re.search(r"all files\s+\| 100\.00\s+\|\s+100\.00\s+\|\s+100\.00", output)
        print("100% production lines, branches and functions; unchanged thresholds and denominators.")


print("PR: https://github.com/link-foundation/link-cli/pull/139")
print("Reproduce: bash experiments/issue-138/validate-causal-merkle.sh")
for suffix, error in [("before", "ERR_MODULE_NOT_FOUND"), ("rust-before", "E0432"),
                      ("csharp-before", "CS0246"),
                      ("continuation-before", "finite retries must advance"),
                      ("leaf-retry-frozen-before", "leaf-only retries must advance")]:
    assert error in log(suffix), suffix
print("Test-first diagnostics: absent APIs and finite node-only/leaf-only traversal starvation.")
print("causal-merkle-before.txt preserves exact diagnostics, hashes and the reproducible frozen candidate.")
assert re.search(r"^ALL PASS \S+", log("validation"), re.M)
print(log("validation").strip())
fixture = json.loads((ROOT / "docs/protocol/causal-merkle-v1.json").read_text())
assert [len(fixture[key]) for key in ("snapshots", "leaves", "nodes")] == [8, 4, 3]
assert "64 snapshot pairs, 48 leaf/proof groups" in log("fixtures")
print("Independent bounded model: 64 snapshot pairs, 48 leaf/proof groups; eight roots, four leaf and three node wire vectors.")
for suffix in ("rust-focused", "wasm-focused"):
    assert "test result: ok. 9 passed; 0 failed; 0 ignored;" in log(suffix), suffix
assert re.search(r"Passed:\s+22, Skipped:\s+0, Total:\s+22", log("csharp-focused"))
assert "22 pass" in log("bun") and "0 fail" in log("bun")
assert "22 passed | 0 failed" in log("deno")
print("Nine Merkle contracts pass in native Rust, C#, Node, Bun, Deno and actual WASM, alongside thirteen causal version/local-owner contracts.")
physical = log("physical-handoffs")
assert "PASS 50 Merkle message handoffs; 20 reaped exits; 100 recovered cross-runtime handoffs" in physical
assert len(re.findall(r"^PASS Merkle (?:plain|mirrored) \w+ messages ->", physical, re.M)) == 50
assert len(re.findall(r"^PASS Merkle (?:plain|mirrored) \w+ exit-(?:before|after) ->", physical, re.M)) == 100
print("SQL-free physical matrix: 50 plain/mirrored wire handoffs across five writers/readers, concurrent siblings and following edits.")
print("Twenty reaped exit-73 children; 100 unchanged-byte stale-owner refusals and 100 recovered retry/reopen/following handoffs.")
print("Physical reads and duplicate pulls preserve file hashes. Real pulls carry four-node/one-leaf continuations, at most forty attempts.")
print("Ownership markers are quarantined only after reaping. Application exits do not certify hardware power loss.")
for mode in ("Debug", "Release"):
    output = log(f"csharp-{mode}-full")
    match = re.search(r"Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)", output)
    assert match and match[1] == match[2] == "2484", (mode, match)
    path = Path(re.findall(r"^\s+(\S+coverage\.cobertura\.xml)$", output, re.M)[-1])
    REPORTS.append(path)
    report = ET.parse(path).getroot()
    print(f"C# {mode}: 2484 passed; zero failed/skipped.")
    for metric in ("lines", "branches"):
        covered, count = [int(report.attrib[f"{metric}-{key}"]) for key in ("covered", "valid")]
        print(f"Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%).")
    for filename in ("CausalVersion.cs", "CausalReplica.cs", "CausalMerkle.cs"):
        classes = [item for item in report.findall(".//class") if item.attrib["filename"].endswith("/" + filename)]
        assert classes
        lines = {int(item.attrib["number"]): item for cls in classes for item in cls.findall("./lines/line")}
        branches = [tuple(map(int, re.search(r"\((\d+)/(\d+)\)", line.attrib["condition-coverage"]).groups()))
                    for line in lines.values() if "condition-coverage" in line.attrib]
        assert all(int(line.attrib["hits"]) > 0 for line in lines.values()), (mode, filename)
        assert all(covered == count for covered, count in branches), (mode, filename)
        print(f"{filename} executable lines: {sum(int(line.attrib['hits']) > 0 for line in lines.values())}/{len(lines)}.")
        print(f"{filename} branches: {sum(c for c, _ in branches)}/{sum(n for _, n in branches)}.")
for title, stem, expected in [("Native Rust", "native", 1219), ("Actual WASM", "wasm", 745)]:
    results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", log(f"{stem}-full"))
    assert results and sum(int(p) for p, _, _ in results) == expected, (title, results)
    assert all(int(f) == 0 for _, f, _ in results)
    print(f"{title}: {expected} passed; {sum(int(i) for _, _, i in results)} parent-driven child entries ignored by normal harness.")
    report_path = BASE / f"{PREFIX}-{stem}-coverage.json"
    REPORTS.append(report_path)
    report = json.loads(report_path.read_text())["data"][0]
    for metric in ("lines", "branches", "functions", "regions", "instantiations"):
        value = report["totals"][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
    component, = [item for item in report["files"] if item["filename"].endswith("/rust/src/engine/causal_merkle.rs")]
    for metric in ("lines", "branches", "functions"):
        value = component["summary"][metric]
        print(f"causal_merkle.rs {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).")
assert "test result: ok. 6 passed; 0 failed; 1 ignored;" in log("doctests")
print("Doctests: six passed; one existing ignored example.")
for suffix, expected in [("js-full", 666), ("js-focused", 9), ("repository", 221)]:
    tests(suffix, expected)
assert "ci-logs should live under the package-specific project tree or case-study evidence" in log("repository-before")
print("Repository-layout before control: one failure at log line 250 because retained CI logs were at the root.")
print("Moved the three logs into experiments/issue-138/ci-logs and corrected the receipt paths; all 221 checks then pass without changing a check.")
assert not re.search(r"serde_json|rusqlite|sha2", log("bare-dependencies"))
assert "Checked 127 declared versions and the Cargo.lock files." in log("dependency-freshness")
print("Formatting, strict native/WASM Clippy, TS declarations, source sizes, one C# changeset, fresh dependencies and workflow/ShellCheck checks pass.")
print("Bare Rust normal dependencies contain no serde_json, rusqlite or sha2. Native physical composition selects no SQLite feature; JS is a separate export.")
print("Admission: 4096 versions/8 MiB total, at most 64 per address before pruning. Fixed depth/fanout 16; opaque cursor retains at most 241 paths of at most 16 digits.")
print("Per-frame preparse bounds: nodes 32768 bytes, leaves 1081344 bytes, five array levels, 32768 unquoted separators, ASCII and no escapes.")
print("Positive per-attempt node/leaf/accepted-byte defaults 4096/64/8388608; maxima 65536/4096/8388608. A rejected fetched leaf remains pending.")
print("Worst-case response-frame budgets are 194 MiB by default and 6272 MiB at maximum counts. Accepted bytes do not cap proofs or rejected frames; processing does not retain all frames.")
print("Tree retained data O(16R+VW), plus path/container overhead; local journal historical replay/publication costs remain. Construction is not an incremental persistent index.")
print("SHA256 equality assumes collision resistance. Content roots omit pruned history, retry receipts and membership; absence never authorizes deletion.")
print("Complete coverage gaps, ordinary links projection, global allocation, safe membership/history reclamation, automatic balancing and wider R1-R12 requirements remain unfinished.")
print("Fresh exact-head platform CI is still required; PR139 remains draft and unmerged.")
for path in [*REPORTS, *(BASE / f"{PREFIX}-{suffix}.log" for suffix in (
        "validation", "js-full", "rust-focused", "wasm-focused", "physical-handoffs",
        "csharp-Debug-full", "csharp-Release-full", "native-full", "wasm-full",
        "repository-before", "repository", "dependency-freshness", "workflows"))]:
    data = path.read_bytes()
    print(f"{path.relative_to(ROOT)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.")
