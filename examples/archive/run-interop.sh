#!/usr/bin/env bash
# Exports a store with one port as a binary store archive, imports it with the
# other, and checks that both stores print the same LiNo (issue #105).
#
# Usage:
#   ./examples/archive/run-interop.sh            # Rust exports, C# imports
#   ./examples/archive/run-interop.sh csharp     # C# exports, Rust imports

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

cargo build --manifest-path "$repo_root/rust/Cargo.toml" --quiet
dotnet build "$repo_root/csharp/Foundation.Data.Doublets.Cli" --nologo --verbosity quiet >/dev/null
rust_clink=("$repo_root/rust/target/debug/clink")
csharp_clink=(dotnet "$repo_root/csharp/Foundation.Data.Doublets.Cli/bin/Debug/net10.0/clink.dll")

if [[ "${1:-rust}" == "csharp" ]]; then
    exporter=("${csharp_clink[@]}") importer=("${rust_clink[@]}")
else
    exporter=("${rust_clink[@]}") importer=("${csharp_clink[@]}")
fi

# Named links, an unnamed one, and a hole at 4.
"${exporter[@]}" --db "$work_dir/source.links" --auto-create-missing-references \
    "() ((child: father mother) (4 4) (5 5))" >/dev/null
"${exporter[@]}" --db "$work_dir/source.links" "((4 4)) ()" \
    --export-binary "$work_dir/store.bin" --out "$work_dir/source.lino" >/dev/null
"${importer[@]}" --db "$work_dir/target.links" \
    --import-binary "$work_dir/store.bin" --out "$work_dir/target.lino" >/dev/null

echo "archive: $(wc -c <"$work_dir/store.bin") bytes, LiNo: $(wc -c <"$work_dir/source.lino") bytes"
od -An -tx1 "$work_dir/store.bin"
diff "$work_dir/source.lino" "$work_dir/target.lino"
cat "$work_dir/target.lino"
