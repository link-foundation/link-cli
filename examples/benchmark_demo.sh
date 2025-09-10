#!/bin/bash

# Benchmark CLI access vs LiNo protocol server access demo
# This script demonstrates the benchmark functionality implemented for issue #31

echo "=== LiNo CLI Benchmark Demo ==="
echo ""
echo "This demo shows the benchmark functionality that compares:"
echo "1. CLI direct file access"
echo "2. LiNo protocol server access"
echo ""

# Clean up any existing database files
rm -f /tmp/demo.links /tmp/demo.names.links /tmp/demo.server.links /tmp/demo.server.names.links

echo "Creating some initial data..."

# Create a few links using CLI
echo "Adding initial data with CLI:"
dotnet run --project ../Foundation.Data.Doublets.Cli -- --db "/tmp/demo.links" '() ((1 1))' --changes --after
echo ""

dotnet run --project ../Foundation.Data.Doublets.Cli -- --db "/tmp/demo.links" '() ((2 2))' --changes --after  
echo ""

dotnet run --project ../Foundation.Data.Doublets.Cli -- --db "/tmp/demo.links" '() ((1 2))' --changes --after
echo ""

echo "Current database state:"
dotnet run --project ../Foundation.Data.Doublets.Cli -- --db "/tmp/demo.links" --after
echo ""

echo "Now running benchmark with minimal iterations to demonstrate functionality..."
echo ""

# Run benchmark with minimal settings to avoid file locking issues
dotnet run --project ../Foundation.Data.Doublets.Cli -- --db "/tmp/demo.links" benchmark --iterations 1 --warmup 0 --server-port 8082 --queries "(((\$i: \$s \$t)) ((\$i: \$s \$t)))"

echo ""
echo "Demo completed!"
echo ""
echo "Note: The benchmark implementation includes:"
echo "- LinoProtocolServer.cs: TCP/IP server for LiNo protocol"
echo "- LinoProtocolClient.cs: Client for connecting to the server"  
echo "- BenchmarkRunner.cs: Orchestrates the comparison"
echo "- Integration into Program.cs as a subcommand"
echo ""
echo "Usage: clink benchmark [options]"
echo "  --iterations: Number of test iterations per query"
echo "  --warmup: Number of warmup iterations"
echo "  --server-port: Port for benchmark server"
echo "  --queries: Custom queries to test"

# Clean up
rm -f /tmp/demo.links /tmp/demo.names.links /tmp/demo.server.links /tmp/demo.server.names.links