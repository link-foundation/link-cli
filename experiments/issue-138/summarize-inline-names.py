"""Summarize the complete run; never substitute diagnostic coverage unions."""
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path

root = Path(__file__).resolve().parent
progress = (root / "inline-names-validation.log").read_text()
assert progress.endswith("Node, Bun and Deno actual-WASM conformance: passed\n"), progress


def counts(name):
    log = (root / name).read_text()
    results = [tuple(map(int, match)) for match in re.findall(
        r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", log)]
    assert results, name
    return dict(zip(["passed", "failed", "ignored"], map(sum, zip(*results))))


def coverage(name):
    report = json.loads((root / name).read_text())["data"][0]
    result = {key: report["totals"][key] for key in ("lines", "branches", "functions")}
    component = [file for file in report["files"]
        if file["filename"].endswith("/text_storage_format.rs")]
    assert len(component) == 1
    result["text_storage_format"] = {key: component[0]["summary"][key]
        for key in ("lines", "branches", "functions")}
    if "wasm" in name:
        result["wrappers"] = {
            file["filename"]: {key: file["summary"][key] for key in ("lines", "branches", "functions")}
            for file in report["files"] if any(file["filename"].endswith("/rust/wasm/src/" + leaf)
                for leaf in ("engine.rs", "engine_log.rs", "lib.rs"))
        }
        assert len(result["wrappers"]) == 3
        for file in result["wrappers"].values():
            for metric in file.values():
                assert metric["count"] == metric["covered"], file
    return result


result = {
    "native": counts("inline-names-native-full.log"),
    "wasm": counts("inline-names-wasm-full.log"),
    "native_coverage": coverage("inline-names-native-coverage.json"),
    "wasm_coverage": coverage("inline-names-wasm-coverage.json"),
}
for mode in ("Debug", "Release"):
    log = (root / f"inline-names-csharp-{mode}-full.log").read_text()
    match = re.search(r"Passed!\s+- Failed:\s+(\d+), Passed:\s+(\d+), Skipped:\s+(\d+), Total:\s+(\d+)", log)
    assert match, mode
    match_path = re.search(r"Attachments:\s*\n\s*([^\n]+coverage.cobertura.xml)", log)
    assert match_path, mode
    report = ET.parse(match_path[1].strip()).getroot()
    result["csharp_" + mode] = {
        **dict(zip(["failed", "passed", "skipped", "total"], map(int, match.groups()))),
        "coverage": {key: int(report.attrib[key]) for key in (
            "lines-covered", "lines-valid", "branches-covered", "branches-valid")},
    }
for key, name in [("js", "js-full"), ("repository", "repository-tests")]:
    log = (root / ("inline-names-" + name + ".log")).read_text()
    result[key] = {field: int(re.search(r"(?:#|ℹ) " + field + r" (\d+)\b", log)[1])
        for field in ("tests", "pass", "fail", "skipped")}
    assert result[key]["fail"] == 0
print(json.dumps(result, indent=2))
