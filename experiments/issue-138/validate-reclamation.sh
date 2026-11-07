#!/usr/bin/env bash
# Complete measured suites, sequential compilers, finite runtime/network probes.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0 DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools/dotnet-10.0.403:$PWD/experiments/issue-138/tools/wasm-bindgen-0.2.129-x86_64-unknown-linux-musl:$PWD/experiments/issue-138/tools:$PATH"
printf 'START native %s\n' "$(date -u +%FT%TZ)"
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/reclamation-clippy.log 2>&1
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path experiments/issue-138/reclamation-native-coverage.json > experiments/issue-138/reclamation-native-full.log 2>&1
cargo test --manifest-path rust/Cargo.toml --all-features --doc > experiments/issue-138/reclamation-doctests.log 2>&1
printf 'PASS native; START csharp %s\n' "$(date -u +%FT%TZ)"
dotnet format csharp --no-restore --verify-no-changes > experiments/issue-138/reclamation-format-check.log 2>&1
dotnet format examples/journal-reclamation-csharp --verify-no-changes > experiments/issue-138/reclamation-example-format-check.log 2>&1
for mode in Debug Release; do
 dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/reclamation-csharp-$mode-results" > "experiments/issue-138/reclamation-csharp-$mode-full.log" 2>&1
done
printf 'PASS csharp; START wasm %s\n' "$(date -u +%FT%TZ)"
cargo fmt --manifest-path rust/wasm/Cargo.toml -- --check
cargo clippy --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown --all-targets -- -D warnings > experiments/issue-138/reclamation-wasm-clippy.log 2>&1
WASM_COVERAGE_EXTRA_RUSTFLAGS=-Clink-arg=--max-memory=268435456 CARGO_LLVM_COV_BIN="$PWD/experiments/issue-138/tools/cargo-llvm-cov" bash experiments/issue-138/wasm-coverage.sh > experiments/issue-138/reclamation-wasm-full.log 2>&1
cp experiments/issue-138/wasm-coverage.json experiments/issue-138/reclamation-wasm-coverage.json
python3 experiments/issue-138/verify-wasm-memory-cap.py > experiments/issue-138/reclamation-wasm-memory-cap.log
printf 'PASS wasm; START javascript %s\n' "$(date -u +%FT%TZ)"
(cd js/client && node --test --test-concurrency=1 --test-timeout=30000 --experimental-test-coverage --test-coverage-include='src/**' --test-coverage-lines=100 --test-coverage-branches=100 --test-coverage-functions=100 test/*.test.mjs && npm run typecheck) > experiments/issue-138/reclamation-js-full.log 2>&1
node experiments/issue-138/verify-reclamation-lazy.mjs > experiments/issue-138/reclamation-lazy.log 2>&1
bun test js/client/test/reclaiming*.test.mjs --timeout 30000 > experiments/issue-138/reclamation-bun.log 2>&1
deno test --allow-all js/client/test/reclaiming*.test.mjs > experiments/issue-138/reclamation-deno.log 2>&1
printf 'PASS javascript; START real-etcd %s\n' "$(date -u +%FT%TZ)"
RUSTFLAGS=-Clink-arg=--max-memory=268435456 cargo build --manifest-path rust/wasm/Cargo.toml --target wasm32-unknown-unknown > experiments/issue-138/reclamation-wasm-package-build.log 2>&1
wasm-bindgen --target web --out-dir experiments/issue-138/reclamation-wasm-package rust/wasm/target/wasm32-unknown-unknown/debug/clink_wasm.wasm > experiments/issue-138/reclamation-wasm-package-bindgen.log 2>&1
export CLINK_WASM="$PWD/experiments/issue-138/reclamation-wasm-package"
cargo build --manifest-path rust/Cargo.toml --features etcd --example journal_reclamation > experiments/issue-138/reclamation-example-rust-build.log 2>&1
dotnet build examples/journal-reclamation-csharp --configuration Debug --disable-build-servers -m:1 > experiments/issue-138/reclamation-example-csharp-build.log 2>&1
RECLAMATION_RUST_EXAMPLE="$PWD/rust/target/debug/examples/journal_reclamation" RECLAMATION_CSHARP_EXAMPLE="$PWD/examples/journal-reclamation-csharp/bin/Debug/net10.0/JournalReclamationExample.dll" node experiments/issue-138/etcd-reclamation.mjs > experiments/issue-138/reclamation-etcd.log 2>&1
printf 'PASS real-etcd; START repository %s\n' "$(date -u +%FT%TZ)"
python3 experiments/issue-138/model-object-reclamation.py > experiments/issue-138/reclamation-model-final.log
python3 experiments/issue-138/model-reclamation-readers.py > experiments/issue-138/reclamation-reader-model-final.log
node experiments/issue-138/verify-reclamation-memory.mjs > experiments/issue-138/reclamation-memory-final.log
python3 experiments/issue-138/make-reclamation-vectors.py experiments/issue-138/reclamation-regenerated.json
cmp docs/protocol/reclamation-v1.json experiments/issue-138/reclamation-regenerated.json
node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/reclamation-file-size.log
node csharp/scripts/validate-changeset.mjs > experiments/issue-138/reclamation-changeset.log
for language in rust csharp js; do
 node .github/scripts/check-sqlite-optional.mjs "$language" > "experiments/issue-138/reclamation-$language-optional.log" 2>&1
done
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/reclamation-repository.log 2>&1
experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck experiments/issue-138/validate-reclamation.sh experiments/issue-138/run-reclamation-before.sh > experiments/issue-138/reclamation-shellcheck.log 2>&1
experiments/issue-138/tools/actionlint -shellcheck="$PWD/experiments/issue-138/tools/shellcheck-v0.11.0/shellcheck" .github/workflows/js-client.yml .github/workflows/rust.yml .github/workflows/csharp.yml .github/workflows/wasm.yml > experiments/issue-138/reclamation-actionlint.log 2>&1
git diff --check
git diff --cached --check
printf 'PASS repository; ALL PASSED %s\n' "$(date -u +%FT%TZ)"
