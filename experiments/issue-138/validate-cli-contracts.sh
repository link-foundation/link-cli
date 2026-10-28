#!/usr/bin/env bash
# Complete denominators and bounded, sequential native compilers.
set -euo pipefail
cd "$(dirname "$0")/../.."
export CARGO_BUILD_JOBS=1 CARGO_PROFILE_DEV_DEBUG=0 CARGO_PROFILE_TEST_DEBUG=0
export DOTNET_GCHeapHardLimit=0x28000000
export PATH="$PWD/experiments/issue-138/tools:$PATH"
cargo fmt --manifest-path rust/Cargo.toml --all -- --check
cargo clippy --manifest-path rust/Cargo.toml --all-features --all-targets -- -D warnings > experiments/issue-138/cli-rust-clippy.log 2>&1
RUSTC_BOOTSTRAP=1 cargo llvm-cov test --manifest-path rust/Cargo.toml --all-features --branch --json --output-path experiments/issue-138/cli-rust-coverage.json > experiments/issue-138/cli-rust-full.log 2>&1
printf 'Complete native Rust with coverage: passed\n'
for mode in Debug Release; do
  dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/cli-csharp-$mode-results" > "experiments/issue-138/cli-csharp-$mode-full.log" 2>&1
  printf 'Complete C# %s with coverage: passed\n' "$mode"
done
dotnet format csharp --no-restore --verify-no-changes > experiments/issue-138/cli-csharp-format-check.log 2>&1
bash examples/version-control/tag-names.sh rust/target/debug/clink > experiments/issue-138/cli-tags-rust-example.log 2>&1
bash examples/version-control/tag-names.sh dotnet csharp/Foundation.Data.Doublets.Cli/bin/Release/net10.0/clink.dll > experiments/issue-138/cli-tags-csharp-example.log 2>&1
node csharp/scripts/validate-changeset.mjs > experiments/issue-138/cli-changeset.log 2>&1
node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/cli-file-size.log 2>&1
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/cli-repository.log 2>&1
git diff --check
