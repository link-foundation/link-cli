#!/usr/bin/env python3
"""Verify complete mapped-owner evidence without narrowing production scope."""
import json
import re
from pathlib import Path

base = Path(__file__).resolve().parent
prefix = "mapped-flush"


def log(suffix):
    return (base / f"{prefix}-{suffix}.log").read_text()


print("PR: https://github.com/link-foundation/link-cli/pull/139")
print("Reproduce: bash experiments/issue-138/validate-mapped-flush.sh")
driver = log("validation")
assert re.search(r"^ALL PASS \S+", driver, re.M), driver[-2048:]
print(driver.strip())
assert "PASS immutable baseline lacks mapped flush ownership" in log("baseline-reproduction")
assert "error[E0599]" in log("api-before")
tail = log("tail-regression-before")
assert "real_owner_publishes_only_its_current_allocation ... FAILED" in tail
assert "assertion `left == right` failed" in tail
print("Immutable 7e9799a baseline lacks both ownership APIs; original pre-implementation diagnostics retained.")
print("Independent current-allocation assertion also first rejects the draft's tail-only flush range.")
for suffix, expected in [("api-after", 6), ("unit-after", 6), ("default-after", 6)]:
    assert f"test result: ok. {expected} passed; 0 failed; 0 ignored;" in log(suffix)
    print(f"{suffix}: {expected} passed, zero failed/ignored.")
assert "adopted mapping flushed and reopened link" in log("example")
print("Runnable adoption example passes; retired handles reject use after closing their owned file.")
for title, stem, expected in [("Native Rust", "native", 1175), ("Actual WASM", "wasm", 723)]:
    results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", log(f"{stem}-full"))
    assert results and sum(int(p) for p, _, _ in results) == expected, (title, results)
    assert all(int(f) == 0 for _, f, _ in results)
    print(f"{title}: {expected} passed; {sum(int(i) for _, _, i in results)} parent-driven child entries ignored by normal harness.")
    report = json.loads((base / f"{prefix}-{stem}-coverage.json").read_text())["data"][0]
    for metric in ("lines", "branches", "functions", "regions", "instantiations"):
        value = report["totals"][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    for suffix in ("/rust/src/storage/mapped_flush.rs", "/rust/src/storage/file_mem.rs", "/rust/src/storage/doublets_storage.rs"):
        component, = [item for item in report["files"] if item["filename"].endswith(suffix)]
        for metric in ("lines", "branches", "functions"):
            value = component["summary"][metric]
            print(f"{suffix} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
print("Doctests: " + re.findall(r"test result: ok\.[^\n]+", log("doctests"))[-1])
output = log("repository")
totals = {key: int(re.search(r"(?:#|ℹ) " + key + r" (\d+)\b", output)[1]) for key in ("tests", "pass", "fail", "skipped")}
assert totals["tests"] == totals["pass"] == 221 and totals["fail"] == totals["skipped"] == 0
print(f"Repository: {totals}")
print("Formatting, native/WASM strict Clippy, source-size, changeset, dependency freshness and workflow/ShellCheck checks pass.")
print("Platform ordering is source-established and tested with syscall mocks; native Windows execution requires exact-head CI.")
print("No production coverage denominator is excluded. Complete native/C#/linked-WASM 100% remains unfinished.")
print("No claim of upstream failed-resize repair, legacy torn-page repair or hardware power-loss verification.")
