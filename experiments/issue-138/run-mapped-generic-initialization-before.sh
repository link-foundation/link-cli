#!/usr/bin/env bash
# Finite, valid-bit-pattern destructor values; no invalid persisted pointers.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=9bb985f602c4cad882703722c7a496e5b8b3b8de
probe_dir=$(mktemp -d "$PWD/experiments/issue-138/mapped-generic-initialization-baseline.XXXXXX")
git archive "$baseline" rust | tar -x -C "$probe_dir"
cp rust/tests/mapped_generic_initialization_tests.rs "$probe_dir/rust/tests/"
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_TARGET_DIR="$probe_dir/target"
set +e
cargo test --manifest-path "$probe_dir/rust/Cargo.toml" --no-default-features --test mapped_generic_initialization_tests > "$probe_dir/tests.log" 2>&1
result=$?
set -e
test "$result" -eq 101
rg -q 'test result: FAILED. 0 passed; 3 failed;' "$probe_dir/tests.log"
printf 'PASS immutable %s reproduces three generic initialization failures; logs: %s\n' "$baseline" "$probe_dir"
