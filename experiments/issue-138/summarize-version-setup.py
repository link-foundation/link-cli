#!/usr/bin/env python3
"""Check the setup-boundary correction against actual logs and full coverage."""
import re
import xml.etree.ElementTree as ET
from pathlib import Path

base = Path(__file__).resolve().parent


def log(suffix):
    return (base / f"version-setup-{suffix}.log").read_text()


print("PR: https://github.com/link-foundation/link-cli/pull/139")
print("Reproduce: bash experiments/issue-138/run-version-setup-before.sh")
print("Reproduce: bash experiments/issue-138/validate-version-setup.sh")
assert "PASS immutable 950bcb34ed060674ed3896f1deb187706be62c09 combined CreateAndUpdate omits completed-creation progress" in log("before-validation")
print("Immutable 950bcb3: completed-creation boundary fails; independent duplicate-row behavior control passes.")
assert re.search(r"^ALL PASS \S+", log("validation"), re.M)
print(log("validation").strip())
assert re.search(r"Passed:\s+31, Skipped:\s+0, Total:\s+31", log("focused"))
print("31 focused controls pass: two setup boundaries, twelve actual version exits and seventeen finite supervision controls.")
print("Actual creation returns before its reported phase; update begins afterward. Duplicate-row effects/results match the unchanged combined helper on a separate real store.")
print("The child enables existing transaction/version tracing; production tracing remains off by default.")
print("The 15-second idle bound, 64 finite phases, bounded capture, actual operations and unconditional reaping are unchanged.")
for mode in ("Debug", "Release"):
    output = log(f"csharp-{mode}-full")
    match = re.search(r"Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)", output)
    assert match and match[1] == match[2] == "2462", (mode, match)

    def coverage(text):
        path = Path(re.findall(r"^\s+(\S+coverage\.cobertura\.xml)$", text, re.M)[-1])
        return ET.parse(path).getroot()

    current = coverage(output)
    previous = coverage((base / f"discovery-guards-csharp-{mode}-full.log").read_text())
    print(f"C# {mode}: {match[1]} passed; zero failed/skipped.")
    for metric in ("lines", "branches"):
        covered, count = [int(current.attrib[f"{metric}-{key}"]) for key in ("covered", "valid")]
        assert count == int(previous.attrib[f"{metric}-valid"]), (mode, metric, count)
        print(f"Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%).")
output = log("repository")
totals = {key: int(re.search(r"(?:#|ℹ) " + key + r" (\d+)\b", output)[1]) for key in ("tests", "pass", "fail", "skipped")}
assert totals["tests"] == totals["pass"] == 221 and totals["fail"] == totals["skipped"] == 0
print(f"Repository: {totals}")
print("Formatting, TS declarations, source-size, changeset, dependency freshness and workflow/ShellCheck checks pass.")
print("This establishes operation-boundary visibility, not the original Windows host's exact scheduling or physical I/O cause.")
print("No production coverage denominator is removed. Complete native/C#/linked-WASM 100% and all other unmet R1-R12 requirements remain unfinished.")
print("Fresh exact-head platform CI remains required; the PR stays draft and unmerged.")
