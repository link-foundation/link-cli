#!/usr/bin/env python3
"""Preserve actual test-first batch diagnostics without embedding complete logs."""
import hashlib
from pathlib import Path

BASE = Path(__file__).resolve().parent
ROOT = BASE.parents[1]
print("PR: https://github.com/link-foundation/link-cli/pull/139")
print("Test-first controls precede the selected production batch implementations.")
controls = (
    ("api-before", "ERR_MODULE_NOT_FOUND", "Node: the requested engine-causal-batch.js API was absent."),
    ("native-before", "error[E0432]", "Rust: engine::causal_batch was absent."),
    ("csharp-before", "error CS0246", "C#: CausalBatch/CausalBatchEdit types were absent."),
    ("ownership-before", "+ 'changed after admission'", "JS: caller payload mutation crossed the first store await."),
    ("physical-generation-before", "'3' !== '4'", "Physical composition: the example incorrectly assumed receipt-only publication advanced the owner key."),
)
for suffix, diagnostic, explanation in controls:
    path = BASE / f"causal-batch-{suffix}.log"
    data = path.read_bytes()
    lines = data.decode().splitlines()
    matching = [str(index) for index, line in enumerate(lines, 1) if diagnostic in line]
    assert matching, (suffix, diagnostic)
    print(explanation)
    print(f"  {path.relative_to(ROOT)} line(s) {', '.join(matching)}: {diagnostic}")
    print(f"  {len(data)} bytes; SHA256 {hashlib.sha256(data).hexdigest()}.")
print("The JS production correction owns normalized observations and payloads before any await.")
print("The physical correction changes all three examples' expected owner generation to 3.")
print("An added real-WAL/table contract checks unchanged owner generation after an empty receipt and the following jump to 3.")
print("The underlying physical store updates only changed keys using its shared generation counter; the batch implementation already guards both actual key generations.")
print("No production store generation semantics were changed to match the example.")
print("The old ambient SDK 10.0.112 formatter also produced only existing-file whitespace diagnostics; SDK 10.0.403 matches CI and is explicitly selected by the drivers.")
print((BASE / "causal-batch-null-token-before.log").read_text().strip())
print("The existing C# admission contract now includes null retry tokens; production code is unchanged.")
print((BASE / "causal-batch-observation-limit-before.log").read_text().strip())
print("The existing portable admission contract now includes 65 supplied observations before pruning; production code is unchanged.")
print("These controls do not establish completion of ordinary links sessions or any wider unmet issue criterion.")
