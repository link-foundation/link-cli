#!/usr/bin/env bash
# Retain ordinary and initial-checkpoint controls; this selected factory uses
# two-revision retention and rewrites both banks repeatedly across providers.
set -euo pipefail
cd "$(dirname "$0")/../.."
report="${NATIVE_COMPACT_REPORT:-experiments/issue-138/native-compact-interop.log}"
mkdir -p "$(dirname "$report")"
NATIVE_PAGES_COMPACT=1 bash examples/native-pages/run-interop.sh > "$report" 2>&1
python3 - "$report" <<'PY'
import re, sys
from pathlib import Path
text = Path(sys.argv[1]).read_text()
configurations = re.findall(r'^(single|mirrored)/(scan|indexed)/(flat|shared|sorted-runs)/(snapshot|changes)/(plain|delta)$', text, re.M)
assert len(configurations) == len(set(configurations)) == 48
groups = re.findall(r'^Native compacting pages group: (\d+) -> (\d+) revisions; (\d+) -> (\d+) rows; private reservation (\d+), pinned rows and branch base verified$', text, re.M)
assert groups == [tuple(map(str, (writer * 2, writer * 2 + 2, writer * 2, writer * 2 + 2, writer * 2 + 3))) for _ in range(48) for writer in range(6)], groups
epochs = re.findall(r'^Native checkpoint bank epoch: (\d+)$', text, re.M)
assert epochs == [str(writer + 1) for _ in range(48) for writer in range(6)], epochs
print('PASS native compaction: 48 configurations, 288 persistent writer handoffs; epochs 1 -> 6; two retained revisions, exact names/allocators, private reservations, pins and branch bases.')
print('Selected mirrored banks are damaged while closed and repaired by the next provider. Complete report: ' + sys.argv[1])
PY
