#!/usr/bin/env bash
# Reproduce unchanged source in isolated source and artifact directories.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=89fbfadf145dab8e5671838f47752d2e047ed7c9
probe_dir=$(mktemp -d "$PWD/experiments/issue-138/mapped-grow-baseline.XXXXXX")
git archive "$baseline" rust | tar -x -C "$probe_dir"
cp rust/tests/mapped_growth_tests.rs "$probe_dir/rust/tests/"
cp rust/tests/support/mapped_growth_quota.rs "$probe_dir/rust/tests/support/"
mkdir -p "$probe_dir/experiments/issue-138/mapped-grow-failure/src"
cp experiments/issue-138/mapped-grow-failure/Cargo.toml "$probe_dir/experiments/issue-138/mapped-grow-failure/"
cp experiments/issue-138/mapped-grow-failure/src/main.rs "$probe_dir/experiments/issue-138/mapped-grow-failure/src/"
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export CARGO_TARGET_DIR="$probe_dir/target"
set +e
cargo test --manifest-path "$probe_dir/rust/Cargo.toml" --no-default-features --test mapped_growth_tests > "$probe_dir/tests.log" 2>&1
result=$?
set -e
test "$result" -eq 101
rg -q 'test result: FAILED. 1 passed; 5 failed; 1 ignored;' "$probe_dir/tests.log"
cargo build --manifest-path "$probe_dir/experiments/issue-138/mapped-grow-failure/Cargo.toml" > "$probe_dir/build.log" 2>&1
LINK_MAPPED_GROWTH_PROBE="$probe_dir/target/debug/mapped-grow-failure-probe" python3 experiments/issue-138/run-mapped-grow-failure.py > "$probe_dir/quota.log" 2>&1
printf 'PASS immutable 89fbfad reproduces five regression failures and bounded stale-pointer signal; logs: %s\n' "$probe_dir"
