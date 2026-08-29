#!/usr/bin/env bash
# Embeds the Rust library as a transactional store — no `clink` binary
# involved (issue #98).
#
# Usage:
#   ./examples/embedded-store/run-rust.sh

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

cargo run \
    --manifest-path "$repo_root/rust/Cargo.toml" \
    --quiet \
    --example embedded_store \
    -- "$work_dir/store"
