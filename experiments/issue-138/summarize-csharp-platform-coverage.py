#!/usr/bin/env python3
"""Verify real platform artifacts against one successful, immutable C# run."""
import argparse
import hashlib
import json
import re
import subprocess
import xml.etree.ElementTree as ET
import zipfile
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("run", type=int)
parser.add_argument("sha")
parser.add_argument("--expected-tests", type=int, default=2462)
args = parser.parse_args()
assert re.fullmatch(r"[0-9a-f]{40}", args.sha)
repository = "link-foundation/link-cli"
base = Path(__file__).resolve().parent
prefix = base / f"csharp-platform-coverage-{args.run}"


def saved_json(suffix, command):
    path = Path(f"{prefix}-{suffix}.json")
    with path.open("wb") as output:
        subprocess.run(["gh", *command], stdout=output, check=True)
    return json.loads(path.read_text())


run = saved_json("run", ["run", "view", str(args.run), "--repo", repository,
                         "--json", "headSha,status,conclusion,createdAt,jobs"])
assert run["headSha"] == args.sha
assert run["status"] == "completed" and run["conclusion"] == "success"
print("PR: https://github.com/link-foundation/link-cli/pull/139", flush=True)
print(f"C# run: https://github.com/{repository}/actions/runs/{args.run}", flush=True)
print(f"Exact head: {args.sha}; created {run['createdAt']}", flush=True)
artifacts = saved_json("artifacts", ["api", f"repos/{repository}/actions/runs/{args.run}/artifacts?per_page=100", "--paginate"])["artifacts"]
by_name = {artifact["name"]: artifact for artifact in artifacts}
platforms = {"ubuntu-24.04": "csharp-coverage", "windows-2025": "csharp-coverage-windows-2025", "macos-26": "csharp-coverage-macos-26"}
missing = sorted(set(platforms.values()) - by_name.keys())
assert not missing, f"Missing complete platform coverage artifacts: {missing}"
log_path = Path(f"{prefix}-full.log")
with log_path.open("wb") as output:
    subprocess.run(["gh", "run", "view", str(args.run), "--repo", repository, "--log"], stdout=output, check=True)
full_log = log_path.read_text()
for platform, name in platforms.items():
    job, = [job for job in run["jobs"] if job["name"] == f"Test ({platform})"]
    assert job["status"] == "completed" and job["conclusion"] == "success"
    step = rf"^Test \({re.escape(platform)}\)\tRun tests\t[^\n]*"
    for label in ("Total tests:", "Passed:"):
        counts = re.findall(step + re.escape(label) + r"\s+(\d+)\s*$", full_log, re.M)
        assert counts == [str(args.expected_tests)], (platform, label, counts)
    assert f"Test ({platform})\tWrite coverage summary\t" in full_log
    artifact = by_name[name]
    assert not artifact["expired"] and artifact["workflow_run"]["head_sha"] == args.sha
    assert artifact["size_in_bytes"] < 16 * 1024 * 1024
    archive = Path(f"{prefix}-{platform}.zip")
    with archive.open("wb") as output:
        subprocess.run(["gh", "api", f"repos/{repository}/actions/artifacts/{artifact['id']}/zip"], stdout=output, check=True)
    assert archive.stat().st_size == artifact["size_in_bytes"] < 16 * 1024 * 1024
    with archive.open("rb") as source:
        digest = hashlib.file_digest(source, "sha256").hexdigest()
    assert f"sha256:{digest}" == artifact["digest"]
    with zipfile.ZipFile(archive) as zipped:
        assert sum(info.file_size for info in zipped.infolist()) < 64 * 1024 * 1024
        reports = [name for name in zipped.namelist() if name.endswith("coverage.cobertura.xml")]
        assert len(reports) == 1, reports
        root = ET.fromstring(zipped.read(reports[0]))
    print(f"{platform}: {args.expected_tests} tests pass, platform summary present; artifact {artifact['id']}, verified SHA256 {digest}.")
    for metric in ("lines", "branches"):
        covered, valid = [int(root.attrib[f"{metric}-{key}"]) for key in ("covered", "valid")]
        assert 0 <= covered <= valid and valid > 0
        print(f"Complete {platform} {metric}: {covered}/{valid} ({100 * covered / valid:.5f}%).")
print("All three existing full reports are retained; Linux artifact name and Codecov behavior are preserved.")
print("No platform percentages are added or treated as newly achieved 100% coverage.")
print("Complete native/C#/linked-WASM coverage and all other unmet R1-R12 criteria remain unfinished; PR139 stays draft/unmerged.")
