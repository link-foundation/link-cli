# LINO API Transport Protocols Benchmark

This benchmark suite evaluates the performance characteristics of different transport protocols when using LINO (Links Notation) instead of JSON for data serialization.

## Overview

This benchmark addresses issue #35 by comparing the performance of three transport protocol approaches:

1. **REST API** with LINO serialization
2. **gRPC** with LINO in protobuf messages 
3. **GraphQL** with LINO in query responses

The benchmarks measure:
- Serialization/deserialization performance
- Data operation throughput
- Protocol processing overhead

## Architecture

### Core Components

- `LinksService`: Provides CRUD operations using the links database
- `LinoSerializer`: Handles LINO format serialization/deserialization
- `TransportProtocolBenchmarks`: BenchmarkDotNet-based performance tests
- `SimpleBenchmark`: Lightweight benchmarking implementation

### Transport Protocol Implementations

#### REST API (`RestLinksController`)
- HTTP endpoints that accept/return LINO format data
- Standard RESTful operations (GET, POST, PUT, DELETE)
- Content-Type: `text/plain` for LINO data

#### gRPC API (`links.proto`)
- Protocol buffer definitions with LINO data wrapped in string fields
- Unary RPC methods for all CRUD operations
- Efficient binary transport with LINO payload

#### GraphQL API
- Schema supporting flexible queries with LINO responses
- Query resolution returns LINO-formatted data
- Single endpoint with query complexity analysis

## Benchmark Results

Latest benchmark run on .NET 8.0:

### Serialization Performance
```
LINO Serialization (100,000 iterations): 862 ms (116 ops/ms)
JSON Serialization (100,000 iterations): 1,409 ms (71 ops/ms)
LINO Deserialization (100,000 iterations): 12,002 ms (8 ops/ms)
JSON Deserialization (100,000 iterations): 748 ms (134 ops/ms)

LINO vs JSON Serialization: 1.63x faster
LINO vs JSON Deserialization: 0.06x (16x slower)
```

### Data Operations Performance
```
Create Links (1,000 operations): 174 ms (6 ops/ms)
Query Links (1,000 operations): 36 ms (28 ops/ms)
```

### Transport Protocol Performance
```
REST-like Processing (10,000 operations): 188 ms (53 ops/ms)
gRPC-like Processing (10,000 operations): 327 ms (31 ops/ms)
GraphQL-like Processing (10,000 operations): 72 ms (139 ops/ms)

Relative Performance:
- GraphQL-like: 1.00x (baseline - fastest)
- REST-like: 2.61x 
- gRPC-like: 4.54x (slowest)
```

## Key Findings

### LINO Serialization Advantages
- **63% faster** serialization compared to JSON
- Compact format reduces payload size
- Human-readable format aids debugging

### LINO Serialization Challenges  
- **16x slower** deserialization compared to JSON
- Complex parsing for nested structures
- Limited tooling compared to JSON ecosystem

### Transport Protocol Performance
1. **GraphQL-style** processing is fastest (139 ops/ms)
2. **REST-style** processing is moderate (53 ops/ms)
3. **gRPC-style** processing is slowest (31 ops/ms)

Note: These results measure protocol processing overhead, not network transport efficiency.

## Running the Benchmarks

### Prerequisites
- .NET 8.0 SDK
- 2GB+ available memory
- Write permissions for database files

### Simple Benchmarks
```bash
dotnet run --configuration Release --project Foundation.Data.Doublets.Cli.Benchmarks
```

### BenchmarkDotNet (Comprehensive)
```bash
# Enable full BenchmarkDotNet suite
dotnet run --configuration Release --project Foundation.Data.Doublets.Cli.Benchmarks -- --use-benchmarkdotnet
```

### Custom Iterations
```bash
# Modify iteration counts in SimpleBenchmark.cs
const int iterations = 50000; // Adjust for your system
```

## Implementation Details

### LINO Format Examples

**Create Request**: `() ((source target))`
```
() ((1 2))  // Create link from 1 to 2
```

**Query Request**: `((id: source target)) ((id: source target))`
```
((1: * *)) ((1: * *))  // Find link with ID 1
((*: 1 *)) ((*: 1 *))  // Find links with source 1
```

**Update Request**: `((id: old_source old_target)) ((id: new_source new_target))`
```
((1: * *)) ((1: 2 3))  // Update link 1 to connect 2->3
```

**Delete Request**: `((id: * *)) ()`
```
((1: * *)) ()  // Delete link with ID 1
```

### Performance Optimization Notes

1. **Serialization**: LINO's simple format enables fast string building
2. **Deserialization**: Complex parsing logic causes performance bottleneck
3. **Memory**: Links database uses memory-mapped files for efficiency
4. **Caching**: Consider caching parsed LINO structures for repeated operations

## Future Improvements

### Deserialization Performance
- Implement compiled parsing using Roslyn
- Add LINO binary format option
- Cache frequently used patterns

### Protocol Extensions
- Add streaming support for large datasets
- Implement batching for bulk operations
- Add compression for network transport

### Monitoring Integration
- Add OpenTelemetry metrics
- Include memory usage tracking  
- Add latency percentile reporting

## Contributing

When adding new benchmarks:

1. Implement in both `SimpleBenchmark.cs` and `TransportProtocolBenchmarks.cs`
2. Follow naming convention: `[Protocol][Operation]Benchmark`
3. Include both throughput and latency measurements
4. Add corresponding unit tests

## Related Issues

- #32: LINO API (REST)
- #33: LINO API (gRPC)  
- #34: LINO API (GraphQL)
- #35: Benchmark LINO API transport protocols

## License

Unlicense - same as parent project