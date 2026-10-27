#!/usr/bin/env bash
# Compile an immutable baseline separately; impossible sizes reject before allocation.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=9bb985f602c4cad882703722c7a496e5b8b3b8de
probe_dir=$(mktemp -d "$PWD/experiments/issue-138/mapped-growth-admission-baseline.XXXXXX")
git archive "$baseline" rust | tar -x -C "$probe_dir"
cp rust/tests/mapped_growth_admission_tests.rs "$probe_dir/rust/tests/"
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_TARGET_DIR="$probe_dir/target"
set +e
cargo test --manifest-path "$probe_dir/rust/Cargo.toml" --no-default-features --test mapped_growth_admission_tests > "$probe_dir/tests.log" 2>&1
result=$?
set -e
test "$result" -eq 101
rg -q 'test result: FAILED. 1 passed; 3 failed;' "$probe_dir/tests.log"
printf 'PASS immutable %s reproduces three generic growth admission failures; successful zero-growth control passes; logs: %s\n' "$baseline" "$probe_dir"
