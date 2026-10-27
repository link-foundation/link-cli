#!/usr/bin/env python3
"""Verify generic growth admission without reducing production scope."""
import json
import re
from pathlib import Path

base = Path(__file__).resolve().parent
prefix = "mapped-growth-admission"


def log(suffix):
    return (base / f"{prefix}-{suffix}.log").read_text()


print("PR: https://github.com/link-foundation/link-cli/pull/139")
print("Reproduce: bash experiments/issue-138/run-mapped-growth-admission-before.sh")
print("Reproduce: bash experiments/issue-138/validate-mapped-growth-admission.sh")
assert "PASS immutable 9bb985f602c4cad882703722c7a496e5b8b3b8de reproduces three generic growth admission failures" in log("before-validation")
print("Immutable 9bb985f: arithmetic overflow, layout overflow and zero-growth panic fail; ordinary zero growth passes.")
print("All three failures reject valid checked flushes despite a still-valid owned mapping. No impossible-size allocation is attempted.")
assert re.search(r"^ALL PASS \S+", log("validation"), re.M)
print(log("validation").strip())
for suffix in ("api-after", "default-after"):
    output = log(suffix)
    assert "test result: ok. 6 passed; 0 failed; 0 ignored;" in output
    assert "test result: ok. 6 passed; 0 failed; 1 ignored;" in output
    assert output.count("test result: ok. 6 passed; 0 failed; 0 ignored;") == 2
    assert "test result: ok. 4 passed; 0 failed; 0 ignored;" in output
    print(f"{suffix}: four generic admission, six flush, six growth and six empty/shrink controls pass; one parent-driven child entry ignored.")
assert "test result: ok. 9 passed; 0 failed; 0 ignored;" in log("unit-after")
print("Nine ownership/growth unit controls pass, including injected mapping-after-extension and clone failures.")
assert "PASS retained mapping after rejected growth" in log("quota-after")
assert "adopted mapping flushed and reopened link" in log("example")
print("Finite quota probe retains pointer, prefix and checked flush after EFBIG; owned-flush example passes.")
for title, stem, expected in [("Native Rust", "native", 1194), ("Actual WASM", "wasm", 723)]:
    results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", log(f"{stem}-full"))
    assert results and sum(int(p) for p, _, _ in results) == expected, (title, results)
    assert all(int(f) == 0 for _, f, _ in results)
    print(f"{title}: {expected} passed; {sum(int(i) for _, _, i in results)} parent-driven child entries ignored by normal harness.")
    report = json.loads((base / f"{prefix}-{stem}-coverage.json").read_text())["data"][0]
    for metric in ("lines", "branches", "functions", "regions", "instantiations"):
        value = report["totals"][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    for suffix in ("/rust/src/storage/file_mem_grow.rs", "/rust/src/storage/file_mem.rs", "/rust/src/storage/mapped_flush.rs"):
        component, = [item for item in report["files"] if item["filename"].endswith(suffix)]
        for metric in ("lines", "branches", "functions"):
            value = component["summary"][metric]
            print(f"{suffix} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
print("Doctests: " + re.findall(r"test result: ok\.[^\n]+", log("doctests"))[-1])
output = log("repository")
totals = {key: int(re.search(r"(?:#|ℹ) " + key + r" (\d+)\b", output)[1]) for key in ("tests", "pass", "fail", "skipped")}
assert totals["tests"] == totals["pass"] == 221 and totals["fail"] == totals["skipped"] == 0
print(f"Repository: {totals}")
print("Formatting, native/WASM strict Clippy, source sizes, changeset, dependency freshness and workflow/ShellCheck checks pass.")
print("Generic zero growth performs no I/O, mapping replacement, lock or allocation, calls fill once with zero initialized/new items and keeps deliberate prefix edits if fill panics.")
print("Arithmetic/layout overflow rejects before invalidation. Each finite generic control preserves pointer, physical length, prefix and checked flush; following growth and exact retirement of forty values pass.")
print("Genuine partial-shrink/generic-growth I/O failure, nonzero callback panic and shrink destructor panic remain unfinished; no physical repair or hardware power-loss claim.")
print("All production coverage denominators remain included. Complete native/C#/linked-WASM 100% and other unmet R1-R12 criteria remain required.")
print("Fresh exact-head Windows/Linux/macOS CI remains required. The PR stays draft and unmerged.")
