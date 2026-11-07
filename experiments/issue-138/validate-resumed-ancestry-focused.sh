#!/usr/bin/env bash
# Rebuild both C# configurations before running the full validation script.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
cargo test --manifest-path rust/Cargo.toml --test version_ancestry_depth_tests --test version_ancestry_semantics_tests --test version_movement_failure_tests --test version_movement_recovery_tests > experiments/issue-138/resumed-ancestry-native-focused.log 2>&1
printf 'Native ancestry and movement controls: passed\n'
for mode in Debug Release; do
 dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --filter 'FullyQualifiedName~VersionAncestry|FullyQualifiedName~VersionMovement' > "experiments/issue-138/resumed-ancestry-csharp-$mode-focused.log" 2>&1
 printf 'C# %s ancestry and movement controls: passed\n' "$mode"
done
