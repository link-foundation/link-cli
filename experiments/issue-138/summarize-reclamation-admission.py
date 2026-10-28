#!/usr/bin/env python3
"""Check complete admission/control receipts without filtering coverage scope."""
import json
import re
from pathlib import Path
import xml.etree.ElementTree as ET

base = Path(__file__).resolve().parent
prefix = "reclamation-admission"


def log(suffix):
    return (base / f"{prefix}-{suffix}.log").read_text()


print("PR: https://github.com/link-foundation/link-cli/pull/139")
print("Reproduce: bash experiments/issue-138/validate-reclamation-admission.sh")
driver = log("validation")
assert re.search(r"^ALL PASS \S+", driver, re.M), driver[-2048:]
print(driver.strip())
final_driver = log("final-validation")
assert re.search(r"^ALL PASS \S+", final_driver, re.M), final_driver[-2048:]
print(final_driver.strip())
assert "50 passed; 0 failed" in log("final-native-focused")
print("Final eligible-orphan fixture: all 50 native maintenance controls, all 723 actual-WASM and all 2,447 C# Debug tests rebuilt/rerun; initial driver runs Release and JS after the change.")
for title, stem, expected in [("Native Rust", "native", 1163), ("Actual WASM", "wasm", 723)]:
    results = re.findall(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", log(f"{stem}-full"))
    assert results and sum(int(p) for p, _, _ in results) == expected, (title, results)
    assert all(int(f) == 0 for _, f, _ in results)
    print(f"{title}: {expected} passed; {sum(int(i) for _, _, i in results)} ignored child entries.")
    report = json.loads((base / f"{prefix}-{stem}-coverage.json").read_text())["data"][0]
    for metric in ("lines", "branches", "functions", "regions", "instantiations"):
        value = report["totals"][metric]
        print(f"Complete {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
    for suffix in ("/rust/src/engine/reclaiming.rs", "/rust/src/engine/reclaiming/codec.rs",
                   "/rust/src/engine/reclaiming/collector.rs", "/rust/src/engine/reclaiming/objects.rs"):
        component, = [item for item in report["files"] if item["filename"].endswith(suffix)]
        for metric in ("lines", "branches", "functions"):
            value = component["summary"][metric]
            print(f"{suffix} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%)")
print("Doctests: " + re.findall(r"test result: ok\.[^\n]+", log("doctests"))[-1])
for mode in ("Debug", "Release"):
    output = log(f"csharp-{mode}-full")
    match = re.search(r"Passed!\s*- Failed:\s+0, Passed:\s+(\d+), Skipped:\s+0, Total:\s+(\d+)", output)
    assert match and match[1] == match[2] == "2447", (mode, match)
    coverage = ET.parse(Path(re.findall(r"^\s+(\S+coverage\.cobertura\.xml)$", output, re.M)[-1])).getroot()
    print(f"C# {mode}: {match[1]} passed; zero failed/skipped.")
    for metric in ("lines", "branches"):
        covered, count = [int(coverage.attrib[f"{metric}-{key}"]) for key in ("covered", "valid")]
        print(f"Complete {metric}: {covered}/{count} ({100 * covered / count:.5f}%)")
for title, suffix, expected in [("Node", "js-full", 644), ("Repository", "repository", 221)]:
    output = log(suffix)
    totals = {key: int(re.search(r"(?:#|ℹ) " + key + r" (\d+)\b", output)[1]) for key in ("tests", "pass", "fail", "skipped")}
    assert totals["tests"] == totals["pass"] == expected and totals["fail"] == totals["skipped"] == 0
    if title == "Node":
        assert re.search(r"all files\s+\|\s+100\.00\s+\|\s+100\.00\s+\|\s+100\.00\s+\|", output)
    print(f"{title}: {totals}")
assert re.search(r"(?:#|ℹ) pass 4\b", log("node-focused"))
assert "4 pass" in log("bun-focused")
assert "4 passed" in log("deno-focused")
print("All four admission controls pass separately in Node/Bun/Deno.")
before = (base / "phase-context-regression-before.log").read_text()
assert "Failed:     1" in before and "phase=5; exited 0" in before
assert "System.TimeoutException" in before
after = (base / "phase-context-after.log").read_text()
assert "queued context callbacks=0" in after and "PASS finite progress supervision" in after
assert "PASS unchanged baseline reproduces phase5/exit0 timeout" in (base / "phase-context-baseline-reproduction.log").read_text()
print("Finite deferred-context regression first times out at phase5/exit0; corrected standalone child passes without caller callbacks.")
print("No production source or persistence contract changes. No production coverage denominator is excluded.")
print("Complete native/C#/linked-WASM 100% and the wider unmet R1-R12 criteria remain unfinished.")
