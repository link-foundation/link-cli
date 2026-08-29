#!/usr/bin/env bash
# Embeds the C# library as a transactional store — no `clink` binary
# involved (issue #98).
#
# Usage:
#   ./examples/embedded-store/run-csharp.sh

set -euo pipefail

repo_root="$(cd "$(dirname "$0")/../.." && pwd)"
work_dir="$(mktemp -d)"
trap 'rm -rf "$work_dir"' EXIT

dotnet restore "$repo_root/examples/embedded-store/csharp" > /dev/null

dotnet run \
    --project "$repo_root/examples/embedded-store/csharp" \
    --configuration Release \
    --no-restore \
    -- "$work_dir/store"
