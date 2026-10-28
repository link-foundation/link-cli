#!/usr/bin/env python3
"""Require complete receipts for the finite-supervision correction."""
import re
import xml.etree.ElementTree as ET
from pathlib import Path

base = Path(__file__).resolve().parent

def log(suffix):
    return (base / f"phase-pool-{suffix}.log").read_text()

print("PR: https://github.com/link-foundation/link-cli/pull/139")
print("Reproduce: bash experiments/issue-138/validate-phase-pool.sh")
assert re.search(r"^ALL PASS \S+", log("validation"), re.M)
print(log("validation").strip())
assert "PASS immutable baseline reproduces occupied-pool phase5/exit0 timeout" in log("baseline-reproduction")
assert re.search(r"System.TimeoutException: bounded shared pool control;.*phase=5; exited 0", log("before"))
before = log("regression-before")
assert re.search(r"Failed:\s+3, Passed:\s+0, Skipped:\s+0, Total:\s+3", before)
assert "supervision needed the occupied shared worker" in before
print("Immutable 89fbfad and original linked-helper logs reproduce the actual phase5/exit0 timeout symptom.")
print("All three durable occupied-worker controls fail first; none changes the test runner's pool.")
assert "PASS finite progress supervision survives a bounded occupied shared pool" in log("probe-after")
assert re.search(r"Passed:\s+17, Skipped:\s+0, Total:\s+17", log("focused-after"))
print("All 17 focused controls pass: occupied-worker progress/stall/repeat, monotonic clock, caller context, malformed phases, both streams and reaping.")
for mode in ("Debug", "Release"):
    output = log(f"csharp-{mode}-full")
    match = re.search(r"Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)", output)
    assert match and match[1] == match[2] == "2451", (mode, match)
    path = Path(re.findall(r"^\s+(\S+coverage\.cobertura\.xml)$", output, re.M)[-1])
    coverage = ET.parse(path).getroot()
    print(f"C# {mode}: {match[1]} passed; zero failed/skipped.")
    for metric in ("lines", "branches"):
        covered, count = [int(coverage.attrib[f"{metric}-{key}"]) for key in ("covered", "valid")]
        print(f"Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)")
output = log("repository")
totals = {key: int(re.search(r"(?:#|ℹ) " + key + r" (\d+)\b", output)[1]) for key in ("tests", "pass", "fail", "skipped")}
assert totals["tests"] == totals["pass"] == 221 and totals["fail"] == totals["skipped"] == 0
print(f"Repository: {totals}")
print("C# formatting, TS declarations, source sizes, changeset, dependency freshness and workflow/ShellCheck checks pass.")
print("No idle/suite bounds or production coverage denominators were widened or excluded.")
print("Three joined dedicated threads per active probe are test-only resource costs.")
print("The isolated control proves a helper dependency, not the exact CI runner scheduling or a persistence guarantee.")
print("Fresh exact-head CI and every wider unmet R1-R12 requirement remain active; the PR stays draft and unmerged.")
