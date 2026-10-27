#!/usr/bin/env python3
"""Generate shared Merkle vectors from the bounded independent Python oracle."""
import json
from pathlib import Path
import runpy

BASE = Path(__file__).resolve().parent
model = runpy.run_path(str(BASE / "causal-merkle-model.py"))
snapshots = model["snapshots"]
result = {"format": "causal-merkle1-vectors", "snapshots": [], "leaves": [], "nodes": []}
for index, snapshot in enumerate(snapshots):
    result["snapshots"].append({"id": index, "versions": [value for _, values in sorted(snapshot.rows.items())
                                                        for value in values], "root": snapshot.root.hex()})
for index, address in [(0, 1), (3, 1), (5, 2**64 - 1), (6, 1)]:
    snapshot = snapshots[index]
    values, path = snapshot.proof(address)
    result["leaves"].append({"snapshot": index, "root": snapshot.root.hex(), "address": str(address),
                             "versions": values, "proof": [[item.hex() for item in level] for level in path]})
for index, prefix in [(0, ""), (5, "000000000000000"), (6, "f")]:
    snapshot = snapshots[index]
    depth = len(prefix)
    address = int((prefix or "0") + "0" * (16 - depth), 16)
    _, path = snapshot.proof(max(1, address))
    result["nodes"].append({"snapshot": index, "root": snapshot.root.hex(), "prefix": prefix,
                            "children": [value.hex() for value in snapshot.children(depth, int(prefix or "0", 16))],
                            "proof": [[item.hex() for item in level] for level in path[:depth]]})
destination = BASE.parents[1] / "docs/protocol/causal-merkle-v1.json"
destination.write_text(json.dumps(result, indent=2) + "\n")
print(f"PASS generated {len(result['snapshots'])} roots, {len(result['leaves'])} leaf proofs and {len(result['nodes'])} node proofs without importing production code.")
