#!/usr/bin/env bash
# Additional local release gates; run after the complete native/WASM driver.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
prefix=${VALIDATION_PREFIX:-experiments/issue-138/query-reservation}
repo_root="$PWD"
tool_root="$PWD/experiments/issue-138/tools/rust-script"
if [[ ! -x "$tool_root/bin/rust-script" ]]; then
  CARGO_TARGET_DIR="$PWD/experiments/issue-138/rust-script-install/target" \
    cargo install rust-script --version 0.36.0 --locked --root "$tool_root" > "$prefix-rust-script-install.log" 2>&1
fi
export PATH="$tool_root/bin:$PATH"
cargo clippy --manifest-path rust/wasm/Cargo.toml --all-targets --all-features -- -D warnings > "$prefix-wasm-host-clippy.log" 2>&1
source_snapshot=$(python3 experiments/issue-138/snapshot-source-size.py)
(cd "$source_snapshot" && rust-script "$repo_root/rust/scripts/check-file-size.rs") > "$prefix-rust-source-size.log" 2>&1
bash rust/scripts/check-scripts.sh > "$prefix-release-compile.log" 2>&1
bash rust/scripts/test-scripts.sh > "$prefix-release-tests.log" 2>&1
cargo metadata --locked --format-version 1 --manifest-path rust/Cargo.toml > "$prefix-native-metadata.json" 2> "$prefix-native-metadata.log"
cargo metadata --locked --format-version 1 --manifest-path rust/wasm/Cargo.toml > "$prefix-wasm-metadata.json" 2> "$prefix-wasm-metadata.log"
node .github/scripts/check-sqlite-optional.mjs rust > "$prefix-sqlite-optional.log" 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/validate-query-reservation-release.sh > "$prefix-release-shellcheck.log" 2>&1
printf 'RELEASE PASS %s\n' "$(date -u +%FT%TZ)"
