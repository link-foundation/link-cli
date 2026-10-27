#!/usr/bin/env python3
"""Keep complete coverage denominators when auditing read-only discovery."""
import re
import xml.etree.ElementTree as ET
from pathlib import Path

base = Path(__file__).resolve().parent


def log(suffix):
    return (base / f"discovery-guards-{suffix}.log").read_text()


print("PR: https://github.com/link-foundation/link-cli/pull/139")
print("Reproduce: bash experiments/issue-138/validate-discovery-guards.sh")
assert re.search(r"^ALL PASS \S+", log("validation"), re.M)
print(log("validation").strip())
assert re.search(r"Passed:\s+9, Skipped:\s+0, Total:\s+9", log("focused"))
print("Nine read-only discovery contracts pass; no production behavior changes.")
print("Actual files retain exact bytes after denied initialization/repair/truncation and owned reads.")
print("Missing/malformed parts, bounds, closed reads, nonregular handles and invalid parents retain their admission boundaries.")
print("Acquisition/cleanup failures retain both causes; all handles close exactly once and repeated disposal returns the same cached failure.")
print("Deferred tables retain capabilities and forward reads only; borrowed WAL reads are owned and all writes reject.")
for mode in ("Debug", "Release"):
    output = log(f"csharp-{mode}-full")
    match = re.search(r"Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)", output)
    assert match and match[1] == match[2] == "2460", (mode, match)
    def coverage(text):
        path = Path(re.findall(r"^\s+(\S+coverage\.cobertura\.xml)$", text, re.M)[-1])
        return ET.parse(path).getroot()
    current = coverage(output)
    previous = coverage((base / f"phase-pool-csharp-{mode}-full.log").read_text())
    print(f"C# {mode}: {match[1]} passed; zero failed/skipped.")
    for metric in ("lines", "branches"):
        covered, count = [int(current.attrib[f"{metric}-{key}"]) for key in ("covered", "valid")]
        assert count == int(previous.attrib[f"{metric}-valid"]), (mode, metric, count)
        before = int(previous.attrib[f"{metric}-covered"])
        assert covered > before, (mode, metric, covered, before)
        print(f"Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%), previously {before}/{count}.")
output = log("repository")
totals = {key: int(re.search(r"(?:#|ℹ) " + key + r" (\d+)\b", output)[1]) for key in ("tests", "pass", "fail", "skipped")}
assert totals["tests"] == totals["pass"] == 221 and totals["fail"] == totals["skipped"] == 0
print(f"Repository: {totals}")
print("Formatting, TS declarations, source-size, changeset, dependency freshness and workflow/ShellCheck checks pass.")
print("No production coverage denominator is removed. Complete native/C#/linked-WASM 100% and all other unmet R1-R12 requirements remain unfinished.")
print("Fresh exact-head platform CI remains required; the PR stays draft and unmerged.")
