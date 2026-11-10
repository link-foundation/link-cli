#!/usr/bin/env bash
# A tiny library and two consumers exercise every branch in one/two types.
set -euo pipefail
cd "$(dirname "$0")"
coverage_sysroot=$(rustc --print sysroot)
coverage_host=$(rustc -vV | sed -n 's/^host: //p')
coverage_tools="$coverage_sysroot/lib/rustlib/$coverage_host/bin"
rustc -vV > versions.txt
RUSTC_BOOTSTRAP=1 rustc --edition=2021 --crate-type=rlib --crate-name=instantiation_probe \
  -Cinstrument-coverage -Zcoverage-options=branch -Cdebuginfo=0 lib.rs -o libinstantiation_probe.rlib
for variant in one two; do
  coverage_variant_flags=()
  if [[ "$variant" == two ]]; then coverage_variant_flags=(--cfg two_types); fi
  RUSTC_BOOTSTRAP=1 rustc --edition=2021 -Cinstrument-coverage -Zcoverage-options=branch \
    -Cdebuginfo=0 "${coverage_variant_flags[@]}" --extern instantiation_probe=libinstantiation_probe.rlib main.rs -o "consumer-$variant"
  LLVM_PROFILE_FILE="$variant.profraw" "./consumer-$variant"
  "$coverage_tools/llvm-profdata" merge -sparse "$variant.profraw" -o "$variant.profdata"
  "$coverage_tools/llvm-cov" export -instr-profile="$variant.profdata" "./consumer-$variant" > "$variant.json"
done
python3 - <<'PY'
import hashlib
import json
from pathlib import Path
for variant, covered in [('one', 2), ('two', 3)]:
    raw = Path(variant + '.json').read_bytes()
    print(variant, 'report SHA256', hashlib.sha256(raw).hexdigest())
    group, = json.loads(raw)['data']
    source, = [f for f in group['files'] if f['filename'].endswith('/lib.rs')]
    summary = source['summary']
    print(variant, json.dumps(summary, sort_keys=True))
    for metric in ['lines', 'branches', 'functions', 'regions']:
        assert summary[metric]['count'] == summary[metric]['covered'] > 0
    assert summary['instantiations']['covered'] == covered
    assert summary['instantiations']['count'] == covered + 1
    missing = [f for f in group['functions'] if f['count'] == 0]
    placeholder, = missing
    assert '6selectpE' in placeholder['name']
    print('UNEXECUTED placeholder:', placeholder['name'])
print('PASS both consumers return exact values and exercise every library source counter')
print('Adding a concrete type retains the compiler placeholder and its zero-count instantiation')
PY
