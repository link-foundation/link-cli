#!/usr/bin/env python3
"""Inspect complete reports; aggregate guard instantiations without filtering files."""
import argparse
import hashlib
import json
from collections import defaultdict
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("reports", nargs="+", type=Path)
parser.add_argument("--require-covered", action="store_true")
args = parser.parse_args()
root = Path(__file__).resolve().parents[2]
source = root / "rust/src/engine/causal_merkle.rs"
guard, = [index for index, line in enumerate(source.read_text().splitlines(), 1)
          if "if key.is_empty() || key.len() > 20" in line]
print("PR: https://github.com/link-foundation/link-cli/pull/139", flush=True)
print(f"Unchanged production source SHA256: {hashlib.sha256(source.read_bytes()).hexdigest()}", flush=True)
for path in args.reports:
    with path.open("rb") as data:
        digest = hashlib.file_digest(data, "sha256").hexdigest()
    report, = json.loads(path.read_text())["data"]
    component, = [item for item in report["files"]
                  if item["filename"].endswith("/rust/src/engine/causal_merkle.rs")]
    print(f"Report {path}: {path.stat().st_size} bytes; SHA256 {digest}.", flush=True)
    for name, metrics in [("Complete production", report["totals"]),
                          ("Merkle component", component["summary"])]:
        for metric in ("lines", "branches", "functions", "regions", "instantiations"):
            value = metrics[metric]
            print(f"{name} {metric}: {value['covered']}/{value['count']} ({value['percent']:.5f}%).", flush=True)
    # LLVM contains repeated generic instantiations. An unused instantiation
    # must not hide observations recorded at the same source condition.
    counts = defaultdict(lambda: [0, 0])
    for branch in component["branches"]:
        if branch[0] == guard:
            for index in range(2):
                counts[tuple(branch[:4])][index] += branch[4 + index]
    assert len(counts) == 2, counts
    for location, observations in sorted(counts.items()):
        print(f"Address guard {location}: true={observations[0]}, false={observations[1]}.", flush=True)
        if args.require_covered:
            assert all(observations), f"Uncovered address admission outcome at {location}: {observations}"
print("All original production files and denominators remain in each complete report.")
print("Coverage outside these two guards, generic mapped failures and all wider unmet requirements remain open.")
