#!/usr/bin/env bash
# Reproduce the missing capability against immutable published source.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=7e9799a7b7ef417d54a456f9dc9606ae6ef35439
probe_dir=$(mktemp -d "$PWD/experiments/issue-138/mapped-flush-baseline.XXXXXX")
git archive "$baseline" rust | tar -x -C "$probe_dir"
cp rust/tests/mapped_flush_tests.rs "$probe_dir/rust/tests/"
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
# Isolate artifacts too: identical crate versions in a shared target directory
# can otherwise make the working-tree build reuse a baseline library artifact.
export CARGO_TARGET_DIR="$probe_dir/target"
set +e
cargo test --manifest-path "$probe_dir/rust/Cargo.toml" --all-features --test mapped_flush_tests --no-run > "$probe_dir/build.log" 2>&1
result=$?
set -e
test "$result" -eq 101
rg -q 'error\[E0599\]' "$probe_dir/build.log"
rg -q 'no method named .flush_handle.' "$probe_dir/build.log"
rg -q 'no method named .with_mapped_flush.' "$probe_dir/build.log"
printf 'PASS immutable baseline lacks mapped flush ownership; complete log: %s/build.log\n' "$probe_dir"
