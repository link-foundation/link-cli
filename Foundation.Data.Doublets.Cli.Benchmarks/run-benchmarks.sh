#!/bin/bash

echo "LINO API Transport Protocols Benchmark Runner"
echo "============================================="
echo ""

# Check if .NET is available
if ! command -v dotnet &> /dev/null; then
    echo "Error: .NET SDK not found. Please install .NET 8.0 SDK."
    exit 1
fi

# Navigate to the benchmark directory
cd "$(dirname "$0")"

# Clean up any existing database files
rm -f *.links

echo "Building benchmark project..."
dotnet build --configuration Release --verbosity quiet

if [ $? -ne 0 ]; then
    echo "Error: Failed to build benchmark project"
    exit 1
fi

echo "Running benchmarks..."
echo ""

# Run the benchmarks
dotnet run --configuration Release --no-build

echo ""
echo "Benchmark completed! Database files cleaned up."

# Clean up database files
rm -f *.links

echo "Results saved in BenchmarkDotNet.Artifacts/ (if BenchmarkDotNet was used)"