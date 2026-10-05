#!/usr/bin/env bash
# Serves a database with one port and queries it with the other, over the
# text protocol and every binary protocol option (issue #105).
#
# Usage:
#   ./examples/tcp/run-interop.sh            # Rust server, C# client
#   ./examples/tcp/run-interop.sh csharp     # C# server, Rust client

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
work_dir="$(mktemp -d)"
server_pid=""
cleanup() {
    if [[ -n "$server_pid" ]]; then kill "$server_pid" 2>/dev/null || true; fi
    rm -rf "$work_dir"
}
trap cleanup EXIT

cargo build --manifest-path "$repo_root/rust/Cargo.toml" --quiet
dotnet build "$repo_root/csharp/Foundation.Data.Doublets.Cli" --nologo --verbosity quiet >/dev/null
rust_clink=("$repo_root/rust/target/debug/clink")
csharp_clink=(dotnet "$repo_root/csharp/Foundation.Data.Doublets.Cli/bin/Debug/net10.0/clink.dll")

if [[ "${1:-rust}" == "csharp" ]]; then
    server=("${csharp_clink[@]}") client=("${rust_clink[@]}")
else
    server=("${rust_clink[@]}") client=("${csharp_clink[@]}")
fi

"${server[@]}" --db "$work_dir/served.links" --serve 127.0.0.1:0 >"$work_dir/banner" &
server_pid=$!
for _ in $(seq 100); do
    grep -q "listening" "$work_dir/banner" && break
    sleep 0.1
done
address="$(sed -n 's/^clink server listening on //p' "$work_dir/banner")"
echo "server: $(head -n 1 "$work_dir/banner")"

index=0
for flags in "" "--protocol binary" "--external-references" "--arity 2..3" \
    "--arity 1.." "--packed-widths" "--external-references --arity 1.. --packed-widths"; do
    index=$((index + 1))
    # shellcheck disable=SC2086 # flags are meant to split
    echo "[${flags:-text}] $("${client[@]}" --connect "$address" $flags "() (($index $index))")"
done
echo "listing:"
"${client[@]}" --connect "$address" --protocol binary --arity 1..
