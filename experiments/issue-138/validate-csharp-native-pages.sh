#!/usr/bin/env bash
# Sequential single-compiler validation; preserve complete logs and reports.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_GCHeapHardLimit=0x28000000
dotnet format csharp --no-restore > experiments/issue-138/paged-csharp-format.log 2>&1
dotnet format examples/native-pages-csharp > experiments/issue-138/paged-csharp-example-format.log 2>&1
for mode in Debug Release; do
 dotnet test csharp --configuration "$mode" --disable-build-servers -m:1 --collect:'XPlat Code Coverage' --results-directory "$PWD/experiments/issue-138/paged-csharp-$mode-results" > "experiments/issue-138/paged-csharp-$mode-full.log" 2>&1
 printf 'Complete C# %s with coverage: passed\n' "$mode"
done
dotnet format csharp --no-restore --verify-no-changes > experiments/issue-138/paged-csharp-format-check.log 2>&1
dotnet build examples/native-pages-csharp --configuration Release --disable-build-servers -m:1 > experiments/issue-138/paged-csharp-example-build.log 2>&1
node .github/scripts/check-sqlite-optional.mjs csharp > experiments/issue-138/paged-csharp-optional.log 2>&1
export NATIVE_PAGES_CSHARP_EXAMPLE="$PWD/examples/native-pages-csharp/bin/Release/net10.0/NativePagesExample.dll"
: "${CLINK_WASM:?provide an existing actual-WASM web package}"
bash examples/native-pages/run-interop.sh > experiments/issue-138/paged-all-interop.log 2>&1
node csharp/scripts/validate-changeset.mjs > experiments/issue-138/paged-csharp-changeset.log 2>&1
node csharp/scripts/check-file-size.mjs --lang all > experiments/issue-138/paged-csharp-file-size.log 2>&1
node --test .github/scripts/*.test.mjs js/test/*.test.mjs csharp/scripts/*.test.mjs > experiments/issue-138/paged-csharp-repository.log 2>&1
git diff --check
