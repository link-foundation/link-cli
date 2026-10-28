#!/usr/bin/env bash
# Build both examples before running. Finite five-writer/five-reader matrix.
set -euo pipefail
cd "$(dirname "$0")/../.."
native_example="${SORTED_TABLE_RUST_EXAMPLE:-$PWD/rust/target/debug/examples/sorted_table}"
managed_example="${SORTED_TABLE_CSHARP_EXAMPLE:-$PWD/examples/sorted-tables-csharp/bin/Debug/net10.0/SortedTablesExample.dll}"
if [[ ! -x "$native_example" || ! -f "$managed_example" ]]; then
 printf '%s\n' 'Build both sorted-table examples first; see examples/sorted-tables/README.md.' >&2
 exit 1
fi
table_data=$(mktemp -d)
trap 'rm -rf "$table_data"' EXIT
provider() {
 local selected="$1";shift
 case "$selected" in
 rust) "$native_example" "$@" ;;
 csharp) dotnet "$managed_example" "$@" ;;
 node|bun) "$selected" js/client/examples/sorted-table.mjs "$@" ;;
 deno) deno run --allow-all js/client/examples/sorted-table.mjs "$@" ;;
 esac
}
for writer in rust csharp node bun deno; do
 database="$table_data/$writer"
 provider "$writer" "$database" create
 sha256sum "$database" "$database.meta0" "$database.meta1" > "$table_data/$writer.sha256"
 for reader in rust csharp node bun deno; do
  provider "$reader" "$database" read
  sha256sum --check --status "$table_data/$writer.sha256"
  test ! -e "$database.writer"
  printf 'Verified physical table: %s -> %s; all three native files unchanged\n' "$writer" "$reader"
 done
done
printf '%s\n' 'PASS: 25 physical native-table writer/reader combinations; no SQL; every owner closed.'
