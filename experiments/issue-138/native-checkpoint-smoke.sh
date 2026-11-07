#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../.."
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools:$PATH"
export DOTNET_GCHeapHardLimit=0x28000000
export CLINK_WASM="${CLINK_WASM:-$PWD/experiments/issue-138/native-checkpoint-wasm-package}"
compact_data=$(mktemp -d)
trap 'rm -rf "$compact_data"' EXIT
for mode in single mirrored; do
 flags=(--compact --references --indexed-constraints --sorted-runs)
 [[ "$mode" == single ]] || flags+=(--mirrored --delta --snapshot)
 path="$compact_data/$mode"
 rust/target/debug/examples/engine_native_pages "$path" "${flags[@]}"
 node js/client/examples/engine-native-pages.mjs "$path" "${flags[@]}"
 bun js/client/examples/engine-native-pages.mjs "$path" "${flags[@]}"
 dotnet examples/native-pages-csharp/bin/Debug/net10.0/NativePagesExample.dll "$path" "${flags[@]}"
 deno run --allow-all js/client/examples/engine-native-pages.mjs "$path" "${flags[@]}"
 rust/target/debug/examples/engine_native_pages "$path" "${flags[@]}"
done
