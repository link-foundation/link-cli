#!/usr/bin/env bash
# Demonstrates the optional transactions layer with the C# `clink` binary.
#
# Usage:
#   ./examples/transactions/run-csharp.sh
#
# Builds and runs the binary from the csharp/ workspace. All artifacts
# land in a fresh tmp directory so multiple runs stay isolated.

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

echo "=== 1. Create two links with --transactions (sync, default retention) ==="
clink --db data.links --transactions --auto-create-missing-references "() ((1 1) (2 2))"
ls -1 *.links

echo
echo "=== 2. Inspect the transitions log ==="
clink --db data.links --log

echo
echo "=== 3. Create another link with explicit async commits and sized retention ==="
clink \
    --db data.links \
    --auto-create-missing-references \
    --commit-mode async \
    --retention sized:128 \
    "() ((3 3))"

echo
echo "=== 4. Print the log again — sequence numbers grew ==="
clink --db data.links --log

echo
echo "=== 5. Try a chunked retention archive (every 1 transition rolls over) ==="
mkdir -p "$work_dir/archive"
clink \
    --db data.links \
    --auto-create-missing-references \
    --retention "chunked:1:$work_dir/archive" \
    "() ((4 4))"
ls -1 "$work_dir/archive" || true

echo
echo "Demo complete. Working dir: $work_dir"
