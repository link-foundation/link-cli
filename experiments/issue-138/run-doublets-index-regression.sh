#!/usr/bin/env bash
# Compile only the four upstream contracts; never build the whole CLI locally.
set -euo pipefail
version=${1:-0.9.0}
case "$version" in
  0.8.0|0.9.0) ;;
  *) echo 'Expected dependency version 0.8.0 or 0.9.0' >&2; exit 2 ;;
esac
repo_root=$(git rev-parse --show-toplevel)
probe="$repo_root/experiments/issue-138/doublets-index-regression"
mkdir -p "$probe"
cat > "$probe/Cargo.toml" <<EOF
[package]
name = "doublets-index-regression"
version = "0.0.0"
edition = "2021"
publish = false
[lib]
path = "../../../rust/tests/doublets_split_index_tests.rs"
[dependencies]
doublets = "=$version"
EOF
export CARGO_BUILD_JOBS=1
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
# Rust's Linux linker otherwise starts a worker per host CPU. Bound that too;
# this container's memory budget is independent of the host's CPU count.
export RUSTFLAGS="${RUSTFLAGS:-} -C link-arg=-Wl,--threads=1"
# Four finite, tiny stores, one test thread and a 1.5 GiB virtual-memory cap.
ulimit -v 1572864
cargo test --manifest-path "$probe/Cargo.toml" --lib -- --test-threads=1
