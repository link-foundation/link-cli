#!/bin/bash
set -euo pipefail

workdir="$(mktemp -d)"
trap 'rm -rf "$workdir"' EXIT

db="$workdir/storable-patterns.links"
triggers="$workdir/storable-patterns.triggers.links"

echo "=== Testing persistent transformation triggers ==="

echo ""
echo "1. Store an always-on trigger in a binary trigger links file"
dotnet run --project csharp/Foundation.Data.Doublets.Cli -- \
  --db "$db" \
  --triggers-file "$triggers" \
  --always \
  '(((1: 1 1)) ((1: 1 2)))'

echo ""
echo "2. Create a matching link; the trigger updates it"
dotnet run --project csharp/Foundation.Data.Doublets.Cli -- \
  --db "$db" \
  --triggers-file "$triggers" \
  --auto-create-missing-references \
  '() ((1: 1 1))' \
  --after

echo ""
echo "3. Remove the stored trigger"
dotnet run --project csharp/Foundation.Data.Doublets.Cli -- \
  --db "$db" \
  --triggers-file "$triggers" \
  --never \
  '(((1: 1 1)) ((1: 1 2)))'

echo ""
echo "=== Test completed ==="
