#!/usr/bin/env bash
# CLINK_WASM is an existing web package; build examples before this script.
set -euo pipefail
cd "$(dirname "$0")/../.."
: "${CLINK_WASM:?provide the generated actual WASM web package}"
native_example="${NATIVE_PAGES_RUST_EXAMPLE:-$PWD/rust/target/debug/examples/engine_native_pages}"
managed_example="${NATIVE_PAGES_CSHARP_EXAMPLE:-$PWD/examples/native-pages-csharp/bin/Debug/net10.0/NativePagesExample.dll}"
test -x "$native_example"
test -f "$managed_example"
page_data=$(mktemp -d)
trap 'rm -rf "$page_data"' EXIT
for validation in scan indexed; do
 for structure in flat shared; do
  for encoding in snapshot changes; do
   for physical in plain delta; do
    flags=(--references)
    [[ "$validation" != indexed ]] || flags+=(--indexed-constraints)
    [[ "$structure" != flat ]] || flags+=(--flat)
    [[ "$encoding" != snapshot ]] || flags+=(--snapshot)
    [[ "$physical" != delta ]] || flags+=(--delta)
    database="$page_data/$validation-$structure-$encoding-$physical"
    printf '%s\n' "$validation/$structure/$encoding/$physical"
    "$native_example" "$database" "${flags[@]}"
    node js/client/examples/engine-native-pages.mjs "$database" "${flags[@]}"
    bun js/client/examples/engine-native-pages.mjs "$database" "${flags[@]}"
    dotnet "$managed_example" "$database" "${flags[@]}"
    deno run --allow-all js/client/examples/engine-native-pages.mjs "$database" "${flags[@]}"
    "$native_example" "$database" "${flags[@]}"
   done
  done
 done
done
