#!/usr/bin/env bash
# Finite real page-image composition experiment; no production/native-file claim.
set -euo pipefail
cd "$(dirname "$0")/../.."
runtime="${1:-all}"
case "$runtime" in all|node) ;; *) printf 'Usage: %s [all|node]\n' "$0" >&2; exit 2 ;; esac
prefix=experiments/issue-138/lsm-store-prototype
generated="$prefix-core.test.mjs"
trap 'rm -f "$generated"' EXIT
python3 - <<'PY'
from pathlib import Path
base=Path('experiments/issue-138')
source=(base/'lsm-store-before.test.mjs').read_text()
original="from '../../js/client/src/engine-lsm-store.js'"
assert source.count(original)==1
(base/'lsm-store-prototype-core.test.mjs').write_text(source.replace(original,"from './lsm-store-prototype.mjs'"))
PY
node --max-old-space-size=128 --stack-size=512 --test --test-concurrency=1 --test-timeout=30000 "$generated" "$prefix.test.mjs" > "$prefix-node.log" 2>&1
if [[ "$runtime" == node ]]; then
 printf 'PASS: Node runs thirteen finite prototype controls; log %s-node.log\n' "$prefix"
 exit 0
fi
bun --smol test "$generated" "$prefix.test.mjs" --timeout 30000 > "$prefix-bun.log" 2>&1
deno test --allow-read --v8-flags=--max-old-space-size=128,--stack-size=512 "$generated" "$prefix.test.mjs" > "$prefix-deno.log" 2>&1
printf 'PASS: Node/Bun/Deno each run thirteen finite prototype controls; logs %s-{node,bun,deno}.log\n' "$prefix"
