#!/usr/bin/env bash
# All providers reopen the exact preceding provider's SQLite database.
set -euo pipefail
cd "$(dirname "$0")/../.."
export DOTNET_GCHeapHardLimit=0x28000000
: "${CLINK_WASM:=$PWD/experiments/issue-138/changes-wasm-package}"
export CLINK_WASM
for runtime in node bun deno; do
 if [[ "$runtime" == deno ]]; then
  deno run --allow-all experiments/issue-138/sqlite-adapter-contracts.mjs > experiments/issue-138/sqlite-adapter-deno.log 2>&1
 else
  "$runtime" experiments/issue-138/sqlite-adapter-contracts.mjs > "experiments/issue-138/sqlite-adapter-$runtime.log" 2>&1
 fi
done
printf 'Node, Bun and Deno actual-provider contracts: passed\n'
sqlite_data=$(mktemp -d "$PWD/experiments/issue-138/sqlite-example-data.XXXXXX")
for structure in flat shared; do
 for encoding in snapshot changes; do
  for physical in plain delta; do
   flags=()
   if [[ "$structure" == flat ]]; then flags+=(--flat); fi
   if [[ "$encoding" == snapshot ]]; then flags+=(--snapshot); fi
   if [[ "$physical" == delta ]]; then flags+=(--delta); fi
   database="$sqlite_data/$structure-$encoding-$physical.db"
   output="experiments/issue-138/sqlite-provider-example-$structure-$encoding-$physical.log"
   {
    rust/target/debug/examples/engine_sqlite "$database" "${flags[@]}"
    dotnet examples/sqlite-csharp/bin/Debug/net10.0/SqliteExample.dll "$database" "${flags[@]}"
    node js/client/examples/engine-sqlite.mjs "$database" "${flags[@]}"
    bun js/client/examples/engine-sqlite.mjs "$database" "${flags[@]}"
    deno run --allow-all js/client/examples/engine-sqlite.mjs "$database" "${flags[@]}"
   } > "$output" 2>&1
   printf 'Persistent SQLite %s/%s/%s: all five providers passed\n' "$structure" "$encoding" "$physical"
  done
 done
done
