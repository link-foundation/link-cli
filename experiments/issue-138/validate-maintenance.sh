#!/usr/bin/env bash
# Complete measured suites, sequential compilers, finite runtime/network probes.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PWD/experiments/issue-138/tools:$PATH"
printf 'START native %s\n' "$(date -u +%FT%TZ)"
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/maintenance-clippy.log 2>&1
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path experiments/issue-138/maintenance-native-coverage.json > experiments/issue-138/maintenance-native-full.log 2>&1
cargo test --manifest-path rust/Cargo.toml --all-features --doc > experiments/issue-138/maintenance-doctests.log 2>&1
printf 'PASS native; START csharp %s\n' "$(date -u +%FT%TZ)"
dotnet format csharp --no-restore --verify-no-changes > experiments/issue-138/maintenance-format-check.log 2>&1
dotnet format examples/journal-maintenance-csharp --verify-no-changes > experiments/issue-138/maintenance-example-format-check.log 2>&1
for mode in Debug Release; do
 dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/maintenance-csharp-$mode-results" > "experiments/issue-138/maintenance-csharp-$mode-full.log" 2>&1
done
printf 'PASS csharp; START wasm %s\n' "$(date -u +%FT%TZ)"
cargo fmt --manifest-path rust/wasm/Cargo.toml -- --check
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > experiments/issue-138/maintenance-wasm-clippy.log 2>&1
WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" bash experiments/issue-138/wasm-coverage.sh > experiments/issue-138/maintenance-wasm-full.log 2>&1
cp experiments/issue-138/wasm-coverage.json experiments/issue-138/maintenance-wasm-coverage.json
python3 experiments/issue-138/verify-wasm-memory-cap.py > experiments/issue-138/maintenance-wasm-memory-cap.log
printf 'PASS wasm; START javascript %s\n' "$(date -u +%FT%TZ)"
(cd js/client && node --test --test-concurrency=1 --test-timeout=30000 --experimental-test-coverage --test-coverage-include='src/**' --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 test/*.test.mjs && npm run typecheck) > experiments/issue-138/maintenance-js-full.log 2>&1
node experiments/issue-138/verify-maintenance-lazy.mjs > experiments/issue-138/maintenance-lazy.log 2>&1
bun test js/client/test/cluster-maintenance.test.mjs --timeout 30000 > experiments/issue-138/maintenance-bun.log 2>&1
deno test --allow-all js/client/test/cluster-maintenance.test.mjs > experiments/issue-138/maintenance-deno.log 2>&1
printf 'PASS javascript; START real-etcd %s\n' "$(date -u +%FT%TZ)"
cargo build --manifest-path rust/Cargo.toml --features etcd --example journal_maintenance > experiments/issue-138/maintenance-example-rust-build.log 2>&1
dotnet build examples/journal-maintenance-csharp --configuration Debug --disable-build-servers -m:1 > experiments/issue-138/maintenance-example-csharp-build.log 2>&1
MAINTENANCE_RUST_EXAMPLE="$PWD/rust/target/debug/examples/journal_maintenance" MAINTENANCE_CSHARP_EXAMPLE="$PWD/examples/journal-maintenance-csharp/bin/Debug/net10.0/JournalMaintenanceExample.dll" node experiments/issue-138/etcd-maintenance.mjs > experiments/issue-138/maintenance-etcd.log 2>&1
printf 'PASS real-etcd; START repository %s\n' "$(date -u +%FT%TZ)"
python3 experiments/issue-138/make-maintenance-vectors.py experiments/issue-138/maintenance-regenerated.json
cmp docs/protocol/journal-maintenance-v1.json experiments/issue-138/maintenance-regenerated.json
node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/maintenance-file-size.log
node csharp/scripts/validate-changeset.mjs > experiments/issue-138/maintenance-changeset.log
for language in rust csharp js; do
 node .github/scripts/check-sqlite-optional.mjs "$language" > "experiments/issue-138/maintenance-$language-optional.log" 2>&1
done
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/maintenance-repository.log 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/validate-maintenance.sh experiments/issue-138/reproduce-maintenance.sh > experiments/issue-138/maintenance-shellcheck.log 2>&1
experiments/issue-138/tools/actionlint -shellcheck="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck" .github/workflows/js-client.yml .github/workflows/rust.yml .github/workflows/csharp.yml > experiments/issue-138/maintenance-actionlint.log 2>&1
git diff --check
printf 'PASS repository; ALL PASSED %s\n' "$(date -u +%FT%TZ)"
