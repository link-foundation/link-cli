#!/usr/bin/env python3
"""Check every compiled causal branch, including methods omitted by Cobertura."""
import argparse
import hashlib
import json
from pathlib import Path

parser = argparse.ArgumentParser()
parser.add_argument("report", type=Path)
args = parser.parse_args()
raw = args.report.read_bytes()
print(f"{args.report}: {len(raw)} bytes; SHA256 {hashlib.sha256(raw).hexdigest()}")
files = {}
required = {"CausalBatch.cs", "CausalMerkle.cs", "CausalProjection.cs",
            "CausalReplica.cs", "CausalVersion.cs"}
for documents in json.loads(raw).values():
    for filename, classes in documents.items():
        # Coverlet records the producer's paths, including Windows backslashes
        # when its archive is inspected later on Linux or macOS.
        name = Path(filename.replace('\\', '/')).name
        if name not in required | {"CausalSession.cs"}:
            continue
        branches = files.setdefault(name, [])
        for methods in classes.values():
            for method, evidence in methods.items():
                for branch in evidence["Branches"]:
                    branches.append(branch)
                    if branch["Hits"] == 0:
                        print(f"UNCOVERED {name}: {method}; source lines={len(evidence['Lines'])}; {branch}")
assert required <= files.keys(), sorted(files)
missing = 0
for name, branches in sorted(files.items()):
    covered = sum(branch["Hits"] > 0 for branch in branches)
    print(f"{name}: {covered}/{len(branches)} raw branches")
    assert branches, name
    missing += len(branches) - covered
assert missing == 0, f"{missing} compiled causal branches remain uncovered"
print("PASS every compiled causal branch, including methods without source lines")
