#!/usr/bin/env bash
# Compile executable rust-script helpers exactly as releases do, without running
# main (which can commit, push, or publish). Test builds alone miss imports that
# are used only under #[cfg(test)]. Included modules compile via their callers.
set -euo pipefail

cd "$(dirname "$0")"
export RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-D warnings"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$(pwd)/../target/scripts}"
status=0
for script in *.rs; do
  grep -q '^fn main' "$script" || continue
  echo "::group::Compile $script (non-test, warnings denied)"
  if ! package=$(rust-script --package "$script") ||
    ! cargo build --release --manifest-path "$package/Cargo.toml"; then
    echo "::error file=rust/scripts/$script::Non-test compilation of $script failed"
    status=1
  fi
  echo "::endgroup::"
done
exit "$status"
