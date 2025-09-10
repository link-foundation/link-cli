#!/bin/bash

# Clean up any existing test files
rm -f test_db.links test_patterns.json

echo "=== Testing Storable Transformation Patterns ==="

echo ""
echo "1. Create initial links"
dotnet run --project Foundation.Data.Doublets.Cli -- '() ((1 1) (2 2))' --db test_db.links --changes --after

echo ""
echo "2. Store a transformation pattern using --always"
dotnet run --project Foundation.Data.Doublets.Cli -- '((1: 1 1)) ((1: 1 3))' --db test_db.links --always --patterns-file test_patterns.json --changes

echo ""
echo "3. Check if patterns.json file was created and contains the pattern"
if [ -f test_patterns.json ]; then
    echo "Patterns file created:"
    cat test_patterns.json
else
    echo "ERROR: Patterns file not found"
fi

echo ""
echo "4. Make a change to trigger stored patterns"
dotnet run --project Foundation.Data.Doublets.Cli -- '() ((3 3))' --db test_db.links --patterns-file test_patterns.json --changes --after --trace

echo ""
echo "5. Remove the stored pattern using --never"
dotnet run --project Foundation.Data.Doublets.Cli -- '((1: 1 1)) ((1: 1 3))' --db test_db.links --never --patterns-file test_patterns.json

echo ""
echo "6. Check if pattern was removed"
if [ -f test_patterns.json ]; then
    echo "Patterns file after removal:"
    cat test_patterns.json
else
    echo "Patterns file not found"
fi

echo ""
echo "=== Test completed ==="