#!/usr/bin/env bash
# Demonstrates the optional version-control layer with the C# `clink` binary.
#
# Usage:
#   ./examples/version-control/run-csharp.sh

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

clink() {
    dotnet run \
        --project "$repo_root/csharp/Foundation.Data.Doublets.Cli" \
        --configuration Release \
        --no-restore \
        -- "$@"
}

cd "$work_dir"

dotnet restore "$repo_root/csharp/Foundation.Data.Doublets.Cli" > /dev/null

echo "=== 1. Apply two links on the default 'main' branch (creates VC sidecar) ==="
clink --db data.links --vc --auto-create-missing-references "() ((1 1) (2 2))"
ls -1 *.links

echo
echo "=== 2. Tag the current head as v1 ==="
clink --db data.links --vc --tag v1

echo
echo "=== 3. Fork branch 'feature' and apply changes there ==="
clink --db data.links --vc --auto-create-missing-references --branch feature --branch-from 2 "() ((3 3))"

echo
echo "=== 4. Tag this branch head as v2 ==="
clink --db data.links --vc --tag v2

echo
echo "=== 5. List branches (current marked with *) ==="
clink --db data.links --vc --list-branches

echo
echo "=== 6. List tags ==="
clink --db data.links --vc --list-tags

echo
echo "=== 7. Switch back to main — feature transitions are rolled back ==="
clink --db data.links --vc --branch main

echo
echo "=== 8. Show full transitions log so far ==="
clink --db data.links --vc --log

echo
echo "=== 9. Checkout the v1 tag (rewind further to seq=2) ==="
clink --db data.links --vc --checkout v1

echo
echo "Demo complete. Working dir: $work_dir"
