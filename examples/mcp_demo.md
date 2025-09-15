# MCP Server Demo for Neural Network Memory

This demonstrates how the link-cli MCP server enables neural networks to store and retrieve persistent memory.

## Quick Start

1. **Start the MCP server:**
```bash
clink --mcp-server
```

2. **The server is now ready for MCP clients** to connect via JSON-RPC 2.0 over stdio.

## MCP Protocol Example

Here's what a typical interaction might look like:

### Initialize Connection
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "initialize",
  "params": {
    "protocolVersion": "2024-11-05",
    "clientInfo": {
      "name": "neural-network-client",
      "version": "1.0.0"
    }
  }
}
```

### Store Memory
```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "store_memory",
    "arguments": {
      "content": "User prefers dark roast coffee over light roast",
      "name": "coffee_preference"
    }
  }
}
```

### Search Memory
```json
{
  "jsonrpc": "2.0",
  "id": 3,
  "method": "tools/call",
  "params": {
    "name": "search_memory",
    "arguments": {
      "query": "coffee"
    }
  }
}
```

### Access All Memory Links
```json
{
  "jsonrpc": "2.0",
  "id": 4,
  "method": "resources/read",
  "params": {
    "uri": "memory://links/all"
  }
}
```

## Integration with AI Assistants

Neural networks and AI assistants can use this MCP server to:

1. **Store Context**: Remember important information from conversations
2. **Build Knowledge**: Create persistent knowledge bases
3. **Learn Preferences**: Remember user preferences and settings  
4. **Track Relationships**: Store associations between concepts
5. **Maintain State**: Keep information across sessions

## Benefits

- **Persistent Memory**: Survives restarts and sessions
- **Associative Storage**: Natural relationship representation
- **Fast Retrieval**: Efficient search and filtering
- **Standard Protocol**: Works with any MCP client
- **Scalable**: Handles large amounts of structured data

The links database provides the perfect foundation for neural network memory systems!