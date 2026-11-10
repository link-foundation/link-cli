#!/usr/bin/env bash
# Build the existing native page examples and provide the actual WASM package.
set -euo pipefail
cd "$(dirname "$0")/../.."
report="${CHECKPOINT_PAGED_REPORT:-experiments/issue-138/checkpoint-paged-interop.log}"
mkdir -p "$(dirname "$report")"
NATIVE_PAGES_CHECKPOINT=1 bash examples/native-pages/run-interop.sh > "$report" 2>&1
python3 - "$report" <<'PY'
import re, sys
from pathlib import Path
text = Path(sys.argv[1]).read_text()
configurations = re.findall(r'^(single|mirrored)/(scan|indexed)/(flat|shared|sorted-runs)/(snapshot|changes)/(plain|delta)$', text, re.M)
assert len(configurations) == len(set(configurations)) == 48
groups = re.findall(r'^Native checkpoint pages group: (\d+) -> (\d+) revisions; (\d+) -> (\d+) rows;', text, re.M)
assert groups == [tuple(map(str, (40 + writer * 2, 42 + writer * 2, 32 + writer * 2, 34 + writer * 2)))
                  for _ in range(48) for writer in range(6)], groups
print('PASS checkpoint pages: 48 configurations, 288 persistent writer handoffs, revision 40 -> 52; allocator holes and retained checkpoint verified.')
print('Mirrored handoffs include damaged-copy repair; complete report: ' + sys.argv[1])
PY
