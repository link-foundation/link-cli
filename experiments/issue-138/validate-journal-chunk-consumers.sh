#!/usr/bin/env bash
# Compile sequentially within the shared 2.92 GiB container memory budget.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_ROOT="$PWD/experiments/issue-138/tools/dotnet-10.0.403"
export PATH="$DOTNET_ROOT:$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
export DOTNET_GCHeapHardLimit=0x28000000
prefix=experiments/issue-138/partitioned-chunks
printf 'BOUNDED CONSUMERS START %s\n' "$(date -u +%FT%TZ)"
RUSTFLAGS=-Clink-arg=--max-memory=268435456 cargo build --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown > "$prefix-wasm-package.log" 2>&1
wasm-bindgen --target web --out-dir "$prefix-wasm-package" rust/wasm/target/wasm32-unknown-unknown/debug/clink_wasm.wasm >> "$prefix-wasm-package.log" 2>&1
python3 experiments/issue-138/verify-wasm-memory-cap.py "$prefix-wasm-package/clink_wasm_bg.wasm" >> "$prefix-wasm-package.log" 2>&1
export CLINK_WASM="$PWD/$prefix-wasm-package"
cargo build --manifest-path rust/Cargo.toml --no-default-features --features native-pages,physical-lsm --example native_lsm > "$prefix-rust-example.log" 2>&1
dotnet build examples/cluster-csharp --configuration Release --disable-build-servers -m:1 > "$prefix-csharp-example.log" 2>&1
for mode in direct partitioned chunks reclaiming; do
 rust/target/debug/examples/native_lsm cluster "$mode" > "$prefix-rust-consumer-$mode.log" 2>&1
 dotnet examples/cluster-csharp/bin/Release/net10.0/ClusterExample.dll --native-lsm "$mode" > "$prefix-csharp-consumer-$mode.log" 2>&1
done
for runtime in node bun; do
 "$runtime" examples/native-lsm-js/cluster.mjs > "$prefix-$runtime-consumers.log" 2>&1
done
deno run --allow-all examples/native-lsm-js/cluster.mjs > "$prefix-deno-consumers.log" 2>&1
for runtime in node bun; do
 "$runtime" examples/journal-chunks/measure.mjs > "$prefix-$runtime-payloads.json"
done
deno run --allow-read --allow-env examples/journal-chunks/measure.mjs > "$prefix-deno-payloads.json"
cmp "$prefix-node-payloads.json" "$prefix-bun-payloads.json"
cmp "$prefix-node-payloads.json" "$prefix-deno-payloads.json"
python3 examples/native-lsm-js/run-interop.py > "$prefix-whole-handoffs.log" 2>&1
NATIVE_LSM_LAYOUT=chunks python3 examples/native-lsm-js/run-interop.py > "$prefix-chunk-handoffs.log" 2>&1
printf 'FIVE RUNTIME CONSUMERS AND HANDOFFS PASS %s\n' "$(date -u +%FT%TZ)"
for runtime in node bun; do
 "$runtime" js/client/test/conformance/run.mjs > "$prefix-$runtime-conformance.log" 2>&1
done
deno run --allow-all js/client/test/conformance/run.mjs > "$prefix-deno-conformance.log" 2>&1
node .github/scripts/check-sqlite-optional.mjs rust > "$prefix-optional.log" 2>&1
node .github/scripts/check-sqlite-optional.mjs csharp >> "$prefix-optional.log" 2>&1
node .github/scripts/check-sqlite-optional.mjs js >> "$prefix-optional.log" 2>&1
printf 'ALL PASS %s\n' "$(date -u +%FT%TZ)"
