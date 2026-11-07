#!/usr/bin/env bash
# Keep baseline source and compiler artifacts isolated for reproducibility.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=2478d59ef43f7766748317be53ca928164d43274
probe_dir=$(mktemp -d "$PWD/experiments/issue-138/mapped-empty-baseline.XXXXXX")
git archive "$baseline" rust | tar -x -C "$probe_dir"
cp rust/tests/mapped_empty_tests.rs "$probe_dir/rust/tests/"
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_TARGET_DIR="$probe_dir/target"
set +e
cargo test --manifest-path "$probe_dir/rust/Cargo.toml" --no-default-features --test mapped_empty_tests > "$probe_dir/tests.log" 2>&1
result=$?
set -e
test "$result" -eq 101
rg -q 'test result: FAILED. 2 passed; 4 failed;' "$probe_dir/tests.log"
printf 'PASS immutable %s reproduces four empty/rejected shrink regressions; two complete-shrink controls pass; logs: %s\n' "$baseline" "$probe_dir"
