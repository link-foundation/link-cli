#!/usr/bin/env bash
# Keep fresh complete reports separate from the earlier allocation receipt.
set -euo pipefail
cd "$(dirname "$0")/../.."
export VALIDATION_PREFIX=experiments/issue-138/causal-protocol
CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo test --manifest-path rust/Cargo.toml --all-features \
  --test engine_causal_protocol_tests --test engine_causal_protocol_tcp_tests \
  --test engine_causal_reply_bounds_tests --test engine_causal_session_tests \
  --test engine_query_allocation_tests --test query_processor_csharp_parity_tests \
  > "$VALIDATION_PREFIX-focused.log" 2>&1
bash experiments/issue-138/validate-query-reservations.sh
bash experiments/issue-138/validate-query-reservation-release.sh
bash experiments/issue-138/validate-causal-protocol-clients.sh
