#!/usr/bin/env bash
# Check the test-only CI correction without uncommitted physical-store work.
set -euo pipefail
cd "$(dirname "$0")/../.."
fixture_checkout="$PWD"
snapshot=$(mktemp -d)
trap 'rm -rf "$snapshot"' EXIT
git archive HEAD | tar -x -C "$snapshot"
cp rust/tests/version_movement_exit_tests.rs "$snapshot/rust/tests/"
cp rust/tests/common/movement_progress.rs "$snapshot/rust/tests/common/"
export CARGO_TARGET_DIR="$fixture_checkout/rust/target"
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
cd "$snapshot"
printf 'Clean committed production snapshot %s plus movement harness correction\n' "$(git -C "$fixture_checkout" rev-parse HEAD)"
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-features --test version_movement_exit_tests -- -D warnings
cargo test --manifest-path rust/Cargo.toml --all-features --test version_movement_exit_tests -- --nocapture
