#!/usr/bin/env bash
# Independent stages keep compilers sequential and preserve complete reports.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PWD/experiments/issue-138/tools:$PATH"
stage="${1:?choose csharp, native, wasm, js, cluster, interop or repository}"
case "$stage" in
 csharp)
  dotnet format csharp --no-restore --verify-no-changes > experiments/issue-138/native-checkpoint-format-check.log 2>&1
  dotnet format examples/native-pages-csharp --verify-no-changes > experiments/issue-138/native-checkpoint-example-format.log 2>&1
  dotnet format examples/cluster-csharp --verify-no-changes > experiments/issue-138/native-checkpoint-cluster-format.log 2>&1
  for mode in Debug Release; do
   dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/native-checkpoint-csharp-$mode-results" > "experiments/issue-138/native-checkpoint-csharp-$mode-full.log" 2>&1
   printf 'Complete C# %s with coverage: passed\n' "$mode"
  done
  ;;
 native)
  python3 experiments/issue-138/make-checkpoint-page-vectors.py experiments/issue-138/native-checkpoint-vectors-regenerated.json
  cmp docs/protocol/checkpoint-pages-v1.json experiments/issue-138/native-checkpoint-vectors-regenerated.json
  cargo fmt --manifest-path rust/Cargo.toml --all -- --check
  cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/native-checkpoint-clippy.log 2>&1
  RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path experiments/issue-138/native-checkpoint-native-coverage.json > experiments/issue-138/native-checkpoint-native-full.log 2>&1
  cargo test --manifest-path rust/Cargo.toml --all-features --doc > experiments/issue-138/native-checkpoint-doctests.log 2>&1
  ;;
 wasm)
  cargo fmt --manifest-path rust/wasm/Cargo.toml -- --check
  cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > experiments/issue-138/native-checkpoint-wasm-clippy.log 2>&1
  WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" bash experiments/issue-138/wasm-coverage.sh > experiments/issue-138/native-checkpoint-wasm-full.log 2>&1
  cp experiments/issue-138/wasm-coverage.json experiments/issue-138/native-checkpoint-wasm-coverage.json
  python3 experiments/issue-138/verify-wasm-memory-cap.py > experiments/issue-138/native-checkpoint-wasm-memory-cap.log
  CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUSTFLAGS=-Clink-arg=--max-memory=268435456 cargo build --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown > experiments/issue-138/native-checkpoint-wasm-package.log 2>&1
  wasm-bindgen --target web --out-dir experiments/issue-138/native-checkpoint-wasm-package rust/wasm/target/wasm32-unknown-unknown/debug/clink_wasm.wasm >> experiments/issue-138/native-checkpoint-wasm-package.log 2>&1
  ;;
 js)
  (cd js/client && node --test --test-concurrency=1 --test-timeout=30000 --experimental-test-coverage --test-coverage-include='src/**' --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 test/*.test.mjs && npm run typecheck) > experiments/issue-138/native-checkpoint-js-full.log 2>&1
  export CLINK_WASM="$PWD/experiments/issue-138/native-checkpoint-wasm-package"
  for runtime in node bun; do
   if [[ "$runtime" == bun ]]; then
    bun test js/client/test/engine-checkpoint-pages.test.mjs js/client/test/engine-compaction*.test.mjs --timeout 30000 > experiments/issue-138/native-checkpoint-bun-portable.log 2>&1
   else
    node --test --test-timeout=30000 js/client/test/engine-checkpoint-pages.test.mjs js/client/test/engine-compaction*.test.mjs > experiments/issue-138/native-checkpoint-node-portable.log 2>&1
   fi
   "$runtime" experiments/issue-138/sorted-runs-contract.mjs > "experiments/issue-138/native-checkpoint-$runtime-contract.log" 2>&1
   "$runtime" js/client/test/conformance/run.mjs > "experiments/issue-138/native-checkpoint-$runtime-conformance.log" 2>&1
   "$runtime" examples/journal-checkpoints/delta.mjs > "experiments/issue-138/native-checkpoint-$runtime-composition.json" 2> "experiments/issue-138/native-checkpoint-$runtime-composition.log"
   "$runtime" examples/journal-checkpoints/compact.mjs > "experiments/issue-138/native-checkpoint-$runtime-compact.json" 2> "experiments/issue-138/native-checkpoint-$runtime-compact.log"
   "$runtime" examples/journal-checkpoints/measure.mjs > "experiments/issue-138/native-checkpoint-$runtime-payload.json" 2> "experiments/issue-138/native-checkpoint-$runtime-payload.log"
  done
  deno test --allow-all js/client/test/engine-checkpoint-pages.test.mjs js/client/test/engine-compaction*.test.mjs > experiments/issue-138/native-checkpoint-deno-portable.log 2>&1
  deno run --allow-all experiments/issue-138/sorted-runs-contract.mjs > experiments/issue-138/native-checkpoint-deno-contract.log 2>&1
  deno run --allow-all js/client/test/conformance/run.mjs > experiments/issue-138/native-checkpoint-deno-conformance.log 2>&1
  deno run --allow-all examples/journal-checkpoints/delta.mjs > experiments/issue-138/native-checkpoint-deno-composition.json 2> experiments/issue-138/native-checkpoint-deno-composition.log
  deno run --allow-all examples/journal-checkpoints/compact.mjs > experiments/issue-138/native-checkpoint-deno-compact.json 2> experiments/issue-138/native-checkpoint-deno-compact.log
  deno run --allow-all examples/journal-checkpoints/measure.mjs > experiments/issue-138/native-checkpoint-deno-payload.json 2> experiments/issue-138/native-checkpoint-deno-payload.log
  cmp experiments/issue-138/native-checkpoint-node-composition.json experiments/issue-138/native-checkpoint-bun-composition.json
  cmp experiments/issue-138/native-checkpoint-node-composition.json experiments/issue-138/native-checkpoint-deno-composition.json
  cmp experiments/issue-138/native-checkpoint-node-payload.json experiments/issue-138/native-checkpoint-bun-payload.json
  cmp experiments/issue-138/native-checkpoint-node-payload.json experiments/issue-138/native-checkpoint-deno-payload.json
  cmp experiments/issue-138/native-checkpoint-node-compact.json experiments/issue-138/native-checkpoint-bun-compact.json
  cmp experiments/issue-138/native-checkpoint-node-compact.json experiments/issue-138/native-checkpoint-deno-compact.json
  ;;
 cluster)
  cargo build --manifest-path rust/Cargo.toml --features etcd --example cluster > experiments/issue-138/native-checkpoint-rust-cluster-build.log 2>&1
  dotnet build examples/cluster-csharp --configuration Debug --disable-build-servers -m:1 > experiments/issue-138/native-checkpoint-csharp-cluster-build.log 2>&1
  export CLINK_WASM="$PWD/experiments/issue-138/native-checkpoint-wasm-package"
  export CLUSTER_RUST_EXAMPLE="$PWD/rust/target/debug/examples/cluster"
  export CLUSTER_CSHARP_EXAMPLE="$PWD/examples/cluster-csharp/bin/Debug/net10.0/ClusterExample.dll"
  for interval in 0 1; do
   CLUSTER_JOURNAL_CHECKPOINT_INTERVAL="$interval" node experiments/issue-138/etcd-cluster.mjs > "experiments/issue-138/native-checkpoint-etcd-$interval.log" 2>&1
  done
  CLUSTER_JOURNAL_CHECKPOINT_INTERVAL=1 CLUSTER_OBJECT_CACHE_BYTES=65536 node experiments/issue-138/etcd-cluster.mjs > experiments/issue-138/native-checkpoint-etcd-cache.log 2>&1
  ;;
 interop)
  cargo build --manifest-path rust/Cargo.toml --no-default-features --features native-pages --example engine_native_pages > experiments/issue-138/native-checkpoint-rust-example-build.log 2>&1
  dotnet build examples/native-pages-csharp --configuration Debug --disable-build-servers -m:1 > experiments/issue-138/native-checkpoint-csharp-example-build.log 2>&1
  export CLINK_WASM="$PWD/experiments/issue-138/native-checkpoint-wasm-package"
  bash examples/native-pages/run-interop.sh > experiments/issue-138/native-checkpoint-all-interop.log 2>&1
  CHECKPOINT_PAGED_REPORT=experiments/issue-138/native-checkpoint-initial-interop.log bash examples/journal-checkpoints/run-paged-interop.sh > experiments/issue-138/native-checkpoint-checkpoint-interop.log 2>&1
  NATIVE_COMPACT_REPORT=experiments/issue-138/native-checkpoint-compact-interop.log bash examples/journal-checkpoints/run-native-compact-interop.sh > experiments/issue-138/native-checkpoint-compact-interop-summary.log 2>&1
  ;;
 repository)
  node csharp/scripts/validate-changeset.mjs > experiments/issue-138/native-checkpoint-changeset.log 2>&1
  node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/native-checkpoint-file-size.log 2>&1
  for language in rust csharp js; do
   node .github/scripts/check-sqlite-optional.mjs "$language" > "experiments/issue-138/native-checkpoint-$language-optional.log" 2>&1
  done
  node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/native-checkpoint-repository.log 2>&1
  "$PWD/experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck" experiments/issue-138/validate-native-checkpoint.sh experiments/issue-138/validate-native-checkpoint-all.sh experiments/issue-138/reproduce-native-checkpoint-factories.sh examples/native-pages/run-interop.sh examples/journal-checkpoints/run-native-compact-interop.sh experiments/issue-138/native-checkpoint-smoke.sh > experiments/issue-138/native-checkpoint-shellcheck.log 2>&1
  "$PWD/experiments/issue-138/tools/actionlint" -shellcheck="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck" .github/workflows/js-client.yml > experiments/issue-138/native-checkpoint-actionlint.log 2>&1
  git diff --check
  ;;
 *) printf 'Unknown stage: %s\n' "$stage" >&2; exit 2 ;;
esac
printf 'Native-checkpoint %s stage: passed\n' "$stage"
