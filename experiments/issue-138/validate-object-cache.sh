#!/usr/bin/env bash
# Complete cache validation, sequential compilers under the container budget.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner WASM_BINDGEN_TEST_TIMEOUT=30 RUST_LOG=warn
cargo fmt --manifest-path rust/Cargo.toml --check
cargo fmt --manifest-path rust/wasm/Cargo.toml --check
dotnet format csharp --no-restore --verify-no-changes > experiments/issue-138/object-cache-format-check.log 2>&1
node csharp/scripts/validate-changeset.mjs > experiments/issue-138/object-cache-changeset.log 2>&1
node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/object-cache-file-size.log 2>&1
printf 'Formatting, release fragments and source sizes: passed\n'
for mode in Debug Release; do
 dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/object-cache-$mode-results" > "experiments/issue-138/object-cache-csharp-$mode-full.log" 2>&1
 printf 'Full C# %s with coverage: passed\n' "$mode"
done
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/object-cache-clippy.log 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > experiments/issue-138/object-cache-wasm-clippy.log 2>&1
printf 'Strict native and WASM Clippy: passed\n'
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path experiments/issue-138/object-cache-native-coverage.json > experiments/issue-138/object-cache-native-full.log 2>&1
printf 'Full native Rust with optional backends and coverage: passed\n'
CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" bash experiments/issue-138/wasm-coverage.sh > experiments/issue-138/object-cache-wasm-full.log 2>&1
cp experiments/issue-138/wasm-coverage.json experiments/issue-138/object-cache-wasm-coverage.json
printf 'Actual WASM full coverage suite: passed\n'
(cd js/client && node --test --test-concurrency=1 --test-timeout=30000 --experimental-test-coverage --test-coverage-include='src/**' --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 test/*.test.mjs && npm run typecheck) > experiments/issue-138/object-cache-js-full.log 2>&1
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/object-cache-repository-tests.log 2>&1
git diff --check
printf 'JS 100%% coverage, TypeScript and repository checks: passed\n'
cargo build --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown > experiments/issue-138/object-cache-wasm-package.log 2>&1
wasm-bindgen --target web --out-dir experiments/issue-138/object-cache-wasm-package rust/wasm/target/wasm32-unknown-unknown/debug/clink_wasm.wasm >> experiments/issue-138/object-cache-wasm-package.log 2>&1
export CLINK_WASM="$PWD/experiments/issue-138/object-cache-wasm-package"
for runtime in node bun; do
 "$runtime" js/client/test/conformance/run.mjs > "experiments/issue-138/object-cache-$runtime-conformance.log" 2>&1
done
deno run --allow-all js/client/test/conformance/run.mjs > experiments/issue-138/object-cache-deno-conformance.log 2>&1
printf 'Node, Bun and Deno actual-WASM conformance: passed\n'
cargo build --manifest-path rust/Cargo.toml --features etcd --example cluster > experiments/issue-138/object-cache-rust-example-build.log 2>&1
dotnet build examples/cluster-csharp --disable-build-servers -m:1 > experiments/issue-138/object-cache-csharp-example-build.log 2>&1
export CLUSTER_RUST_EXAMPLE="$PWD/rust/target/debug/examples/cluster"
export CLUSTER_CSHARP_EXAMPLE="$PWD/examples/cluster-csharp/bin/Debug/net10.0/ClusterExample.dll"
for bytes in 0 65536; do
 CLUSTER_OBJECT_CACHE_BYTES="$bytes" node experiments/issue-138/etcd-cluster.mjs > "experiments/issue-138/object-cache-etcd-$bytes.log" 2>&1
done
printf 'Real etcd interoperability, leader/majority/data failures, default and cached: passed\n'
