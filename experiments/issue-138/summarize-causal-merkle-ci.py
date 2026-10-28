#!/usr/bin/env python3
"""Summarize completed Rust job logs without counting repeated doctests twice."""
import hashlib
import re
from collections import defaultdict
from pathlib import Path

root = Path(__file__).resolve().parents[2]
print("PR: https://github.com/link-foundation/link-cli/pull/139")
print("Exact head: 283b86dba44ca59797feb1557c8d59128c8b2c87")
for job, platform in [(122309740987, "macos-26"), (122309741199, "ubuntu-24.04"),
                      (122309741185, "windows-2025")]:
    path = root / f"experiments/issue-138/ci-logs/causal-merkle-rust-{job}.log"
    data = path.read_bytes()
    assert data, f"No downloaded log for {job}"
    groups = defaultdict(lambda: [0, 0, 0, 0])
    command = None
    for line in data.decode("utf-8", errors="replace").splitlines():
        if "##[group]Run " in line:
            command = line.split("##[group]Run ", 1)[1]
        match = re.search(r"test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored;", line)
        if match and command and command.startswith("cargo test "):
            for index, value in enumerate(match.groups()):
                groups[command][index] += int(value)
            groups[command][3] += 1
    assert len(groups) == 3, groups
    print(f"\n{platform}: https://github.com/link-foundation/link-cli/actions/runs/40537256041/job/{job}")
    print(f"{path.relative_to(root)}: {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}; {len(data.splitlines())} lines.")
    for command, (passed, failed, ignored, records) in groups.items():
        assert passed > 0 and failed == 0
        print(f"{command}: {passed} passed, {failed} failed, {ignored} ignored across {records} result records.")
print("\nAll-features includes six passing doctests and one existing ignored doctest; the separate doc command repeats them.")
print("Focused no-default-feature tests are recorded separately, not added to the full-suite count.")
print("These results cover the published Merkle implementation, not subsequent admission controls or unfinished R1-R12 criteria.")
