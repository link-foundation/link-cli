#!/usr/bin/env bash
# Focused native regressions; complete suites run only in CI.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 CARGO_INCREMENTAL=0
export RUSTFLAGS="${RUSTFLAGS:--C link-arg=-Wl,--threads=1}"
cargo test --manifest-path rust/Cargo.toml \
  --test recycling_query_allocation_tests \
  --test query_processor_tests \
  --test query_processor_csharp_parity_tests \
  --test query_processor_change_report_tests -- --test-threads=1
