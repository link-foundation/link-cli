#!/usr/bin/env bash
# Run only the heap-doublets contracts. An optional immutable revision is a
# negative control; the final test file is copied into its archived source.
set -euo pipefail
repo_root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$repo_root"
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
export RUSTFLAGS="${RUSTFLAGS:--C link-arg=-Wl,--threads=1}"
export RUST_BACKTRACE=full
logs=experiments/issue-138/ci-logs
mkdir -p "$logs"
manifest=rust/Cargo.toml
label=after
if [[ $# != 0 ]]; then
    revision=$(git rev-parse --verify "$1^{commit}")
    fixture="experiments/issue-138/doublets-portable-baseline-$revision"
    mkdir -p "$fixture"
    git archive "$revision" rust docs | tar -x -C "$fixture"
    cp rust/tests/doublets_portable_tests.rs "$fixture/rust/tests/"
    manifest="$fixture/rust/Cargo.toml"
    label=before-immutable
    printf 'Immutable baseline: %s\n' "$revision"
    # Archived source timestamps can be older than an already-built fixed
    # library. Keep its target separate so Cargo cannot reuse that binary.
    export CARGO_TARGET_DIR="$repo_root/$fixture/target"
fi
status=0
cargo test --manifest-path "$manifest" --test doublets_portable_tests \
    -- --test-threads=1 > "$logs/doublets-portable-$label.log" 2>&1 || status=$?
cat "$logs/doublets-portable-$label.log"
if [[ $label == before-immutable ]]; then
    python3 - "$logs/doublets-portable-$label.log" "$status" <<'PY'
import sys
from pathlib import Path
log = Path(sys.argv[1]).read_text()
assert sys.argv[2] == '101', f'unexpected cargo status: {sys.argv[2]}'
assert '8 passed; 3 failed' in log
for name in (
    'concrete_pair_query_returns_every_duplicate_in_an_undecorated_store',
    'null_endpoints_are_searchable_and_get_or_create_reuses_the_existing_row',
    'patterns_intersect_index_source_and_target_without_mutating_rows',
):
    assert f'test {name} ... FAILED' in log
print('PASS immutable negative control: exactly the three semantic regressions fail')
PY
else
    exit "$status"
fi
