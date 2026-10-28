#!/usr/bin/env bash
# Sequential complete validation within the issue solver's memory budget.
# Requires setup-resumed-validation.sh and restored project dependencies.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PATH"
export CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner WASM_BINDGEN_TEST_TIMEOUT=30 RUST_LOG=warn
cargo fmt --manifest-path rust/Cargo.toml --check
cargo fmt --manifest-path rust/wasm/Cargo.toml --check
dotnet format csharp --no-restore --verify-no-changes > experiments/issue-138/sqlite-format.log 2>&1
node csharp/scripts/validate-changeset.mjs > experiments/issue-138/sqlite-changeset.log 2>&1
node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/sqlite-file-size.log 2>&1
printf 'Formatting, release fragments, source sizes: passed\n'
for mode in Debug Release; do
 dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/sqlite-$mode-results" > "experiments/issue-138/sqlite-csharp-$mode-full.log" 2>&1
 printf 'Full C# %s with coverage: passed\n' "$mode"
done
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/sqlite-clippy.log 2>&1
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > experiments/issue-138/sqlite-wasm-clippy.log 2>&1
printf 'Strict native and WASM Clippy: passed\n'
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path experiments/issue-138/sqlite-native-coverage.json > experiments/issue-138/sqlite-native-full.log 2>&1
printf 'Full native Rust + SQLite coverage suite: passed\n'
CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" bash experiments/issue-138/wasm-coverage.sh > experiments/issue-138/sqlite-wasm-full.log 2>&1
cp experiments/issue-138/wasm-coverage.json experiments/issue-138/sqlite-wasm-coverage.json
printf 'Actual WASM coverage suite: passed\n'
(cd js/client && node --test --test-concurrency=1 --test-timeout=30000 --experimental-test-coverage --test-coverage-include='src/**' --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 test/*.test.mjs && npm run typecheck) > experiments/issue-138/sqlite-js-full.log 2>&1
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/sqlite-repository-tests.log 2>&1
git diff --check
printf 'JS coverage, types, repository checks and whitespace: passed\n'
# Build uninstrumented glue for runtime conformance and persistent examples.
cargo build --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown > experiments/issue-138/sqlite-wasm-package.log 2>&1
wasm-bindgen --target web --out-dir experiments/issue-138/sqlite-wasm-package rust/wasm/target/wasm32-unknown-unknown/debug/clink_wasm.wasm >> experiments/issue-138/sqlite-wasm-package.log 2>&1
export CLINK_WASM="$PWD/experiments/issue-138/sqlite-wasm-package"
node js/client/test/conformance/run.mjs > experiments/issue-138/sqlite-node-conformance.log 2>&1
bun js/client/test/conformance/run.mjs > experiments/issue-138/sqlite-bun-conformance.log 2>&1
deno run --allow-all js/client/test/conformance/run.mjs > experiments/issue-138/sqlite-deno-conformance.log 2>&1
printf 'Node, Bun and Deno actual-WASM conformance: passed\n'
example_data=$(mktemp -d "$PWD/experiments/issue-138/group-example-data.XXXXXX")
for encoding in snapshot delta; do
 flags=()
 if [[ "$encoding" == delta ]]; then flags=(--delta); fi
 # Each following language reopens the preceding language's exact journal.
 cargo run --manifest-path rust/Cargo.toml --example engine_group_commit -- "$example_data/$encoding" "${flags[@]}" > "experiments/issue-138/sqlite-example-$encoding.log" 2>&1
 dotnet run --project examples/group-commit-csharp --disable-build-servers -- "$example_data/$encoding" "${flags[@]}" >> "experiments/issue-138/sqlite-example-$encoding.log" 2>&1
 node js/client/examples/engine-group-commit.mjs "$example_data/$encoding" "${flags[@]}" >> "experiments/issue-138/sqlite-example-$encoding.log" 2>&1
done
printf 'Persistent snapshot/delta examples across all three languages: passed\n'

name_data=$(mktemp -d "$PWD/experiments/issue-138/named-example-data.XXXXXX")
cargo run --manifest-path rust/Cargo.toml --example transaction_names -- "$name_data/rust" > experiments/issue-138/sqlite-example-rust.log 2>&1
dotnet run --project examples/transaction-names-csharp --disable-build-servers -- "$name_data/csharp" > experiments/issue-138/sqlite-example-csharp.log 2>&1
for mode in rust csharp; do
 if [[ "$mode" == rust ]]; then
  cargo run --manifest-path rust/Cargo.toml --example transaction_names -- "$name_data/rust-selected" --row-image > experiments/issue-138/sqlite-example-rust-selected.log 2>&1
 else
  dotnet run --project examples/transaction-names-csharp --disable-build-servers -- "$name_data/csharp-selected" --row-image > experiments/issue-138/sqlite-example-csharp-selected.log 2>&1
 fi
done
printf 'Persistent ordinary transaction/name/history examples, both policies: passed\n'

strategy_data=$(mktemp -d "$PWD/experiments/issue-138/shared-strategy-data.XXXXXX")
for structure in flat shared; do
 for encoding in snapshot changes; do
  for physical in plain delta; do
  flags=(--read-set --pessimistic)
  if [[ "$structure" == shared ]]; then flags+=(--shared); fi
  if [[ "$encoding" == changes ]]; then flags+=(--changes); fi
  if [[ "$physical" == delta ]]; then flags+=(--delta); fi
  journal="$strategy_data/$structure-$encoding-$physical"
  output="experiments/issue-138/sqlite-example-$structure-$encoding-$physical.log"
  cargo run --manifest-path rust/Cargo.toml --example engine_strategies -- "$journal" "${flags[@]}" > "$output" 2>&1
  dotnet run --project examples/engine-csharp --disable-build-servers -- "$journal" "${flags[@]}" >> "$output" 2>&1
  node js/client/examples/engine.mjs "$journal" "${flags[@]}" >> "$output" 2>&1
  done
 done
done
printf 'Flat/shared snapshot/changes plain/delta strategy examples across all languages: passed\n'

cargo build --manifest-path rust/Cargo.toml --features sqlite --example engine_sqlite > experiments/issue-138/sqlite-rust-example-build.log 2>&1
dotnet build examples/sqlite-csharp --disable-build-servers -m:1 > experiments/issue-138/sqlite-csharp-example-build.log 2>&1
bash experiments/issue-138/sqlite-examples.sh > experiments/issue-138/sqlite-examples.log 2>&1
printf 'SQLite contracts and eight five-provider persistent configurations: passed\n'
