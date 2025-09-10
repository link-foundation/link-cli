# LINO GRPC API

This document describes the GRPC-style API for LINO operations, as requested in issue #33.

## Overview

The LINO GRPC API provides a modern, efficient way to interact with the links database using LINO notation instead of JSON. Unlike traditional GRPC APIs that use JSON for message payloads, this API uses LINO (Links Notation) strings directly in the protocol buffer messages.

## Key Features

- **LINO-based Messages**: Uses LINO notation strings instead of JSON for all operations
- **Full CRUD Support**: Create, Read, Update, Delete operations using familiar LINO syntax
- **Batch Operations**: Execute multiple LINO queries in a single request
- **Streaming**: Real-time bidirectional streaming for continuous operations
- **Type Safety**: Strongly-typed protocol buffer messages
- **Performance**: HTTP/2-based communication with multiplexing and compression

## Starting the GRPC Server

```bash
# Start GRPC server on default port 5001
clink --grpc-server

# Start GRPC server on custom port
clink --grpc-server --grpc-port 8080
```

## API Service Definition

The service is defined in `protos/lino_service.proto`:

```protobuf
service LinoService {
  // Execute a single LINO query
  rpc ExecuteQuery(LinoQueryRequest) returns (LinoQueryResponse);
  
  // Execute multiple LINO queries in a batch
  rpc ExecuteBatch(LinoBatchRequest) returns (LinoBatchResponse);
  
  // Stream LINO queries and get real-time responses
  rpc StreamQueries(stream LinoStreamRequest) returns (stream LinoStreamResponse);
  
  // Get all links in the database
  rpc GetAllLinks(GetAllLinksRequest) returns (GetAllLinksResponse);
  
  // Get structure of a specific link
  rpc GetStructure(GetStructureRequest) returns (GetStructureResponse);
}
```

## Message Types

### LinoQueryRequest
```protobuf
message LinoQueryRequest {
  string query = 1;              // LINO query string (e.g., "() ((1 1))")
  string database_path = 2;      // Optional database path
  bool trace = 3;                // Enable verbose output
  bool include_changes = 4;      // Include changes in response
  bool include_before_state = 5; // Include state before operation
  bool include_after_state = 6;  // Include state after operation
}
```

### LinoQueryResponse
```protobuf
message LinoQueryResponse {
  bool success = 1;                    // Operation success status
  string error_message = 2;            // Error message if failed
  repeated string changes = 3;         // Changes in LINO format
  repeated string before_state = 4;    // State before in LINO format
  repeated string after_state = 5;     // State after in LINO format
  ExecutionMetadata metadata = 6;      // Execution metadata
}
```

## LINO vs JSON Comparison

### Traditional GRPC with JSON:
```json
{
  "operation": "create",
  "links": [
    {"source": 1, "target": 1},
    {"source": 2, "target": 2}
  ]
}
```

### LINO GRPC API:
```protobuf
query: "() ((1 1) (2 2))"
```

The LINO approach is more concise and directly compatible with the existing LINO ecosystem.

## Usage Examples

### 1. Create Links
```csharp
var request = new LinoQueryRequest
{
    Query = "() ((1 1) (2 2))",  // Create two links
    IncludeChanges = true,
    IncludeAfterState = true
};

var response = await client.ExecuteQueryAsync(request);
```

### 2. Update Links
```csharp
var request = new LinoQueryRequest
{
    Query = "((1: 1 1)) ((1: 1 2))",  // Update link 1
    IncludeChanges = true
};

var response = await client.ExecuteQueryAsync(request);
```

### 3. Read Links
```csharp
var request = new LinoQueryRequest
{
    Query = "((($i: $s $t)) (($i: $s $t)))",  // Read all links
    IncludeAfterState = true
};

var response = await client.ExecuteQueryAsync(request);
```

### 4. Delete Links
```csharp
var request = new LinoQueryRequest
{
    Query = "((1 2)) ()",  // Delete link with source 1, target 2
    IncludeChanges = true
};

var response = await client.ExecuteQueryAsync(request);
```

### 5. Batch Operations
```csharp
var batchRequest = new LinoBatchRequest();
batchRequest.Queries.Add(new LinoQueryRequest { Query = "() ((3 3))" });
batchRequest.Queries.Add(new LinoQueryRequest { Query = "() ((4 4))" });

var response = await client.ExecuteBatchAsync(batchRequest);
```

### 6. Streaming Operations
```csharp
using var stream = client.StreamQueries();

await stream.RequestStream.WriteAsync(new LinoStreamRequest
{
    Query = new LinoQueryRequest { Query = "() ((5 5))" }
});

await foreach (var response in stream.ResponseStream.ReadAllAsync())
{
    Console.WriteLine($"Response: {response.QueryResponse.Success}");
}
```

## Benefits over JSON-based GRPC

1. **Native LINO Support**: Direct use of LINO syntax without translation layers
2. **Consistency**: Same syntax as CLI tool and existing toolchain
3. **Expressiveness**: LINO's pattern matching is more powerful than JSON structures
4. **Compactness**: LINO notation is often more concise than equivalent JSON
5. **Type Safety**: Protocol buffers provide compile-time type checking
6. **Performance**: Binary protocol buffer encoding vs text-based JSON

## Implementation Status

- ✅ Protocol buffer definitions
- ✅ GRPC service interface  
- ✅ Basic request/response handling
- ✅ CLI integration with `--grpc-server` flag
- 🔄 Full ASP.NET Core server implementation (in progress)
- 🔄 Client libraries and tooling
- 🔄 Advanced features (authentication, load balancing, etc.)

## Development Setup

1. Install .NET 8 SDK
2. Install Protocol Buffers compiler (`protoc`)
3. Build the project: `dotnet build`
4. Run server: `clink --grpc-server`
5. Use any GRPC client to connect to `localhost:5001`

## Client Generation

Generate client libraries for various languages:

```bash
# C#/.NET (already included)
dotnet build

# Python
python -m grpc_tools.protoc -I./protos --python_out=./clients/python --grpc_python_out=./clients/python ./protos/lino_service.proto

# Go
protoc --go_out=./clients/go --go-grpc_out=./clients/go ./protos/lino_service.proto

# Node.js/TypeScript
protoc --js_out=import_style=commonjs:./clients/nodejs --grpc-web_out=import_style=typescript,mode=grpcjs:./clients/nodejs ./protos/lino_service.proto
```

## Testing

Use tools like BloomRPC, grpcurl, or custom clients to test the API:

```bash
# Using grpcurl to test
grpcurl -plaintext -d '{"query": "() ((1 1))"}' localhost:5001 lino.api.LinoService/ExecuteQuery
```

## Future Enhancements

- Authentication and authorization
- Rate limiting and throttling  
- Metrics and monitoring integration
- Advanced streaming patterns
- GraphQL-style query optimization
- Multi-database support
- Clustering and load balancing

---

This GRPC API provides a modern, efficient alternative to REST APIs while maintaining full compatibility with LINO notation and the existing links ecosystem.