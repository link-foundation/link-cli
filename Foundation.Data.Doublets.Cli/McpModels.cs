using System.Text.Json.Serialization;

namespace Foundation.Data.Doublets.Cli
{
    // Initialize request/response
    public class McpInitializeParams
    {
        [JsonPropertyName("protocolVersion")]
        public string ProtocolVersion { get; set; } = "";
        
        [JsonPropertyName("capabilities")]
        public McpClientCapabilities? Capabilities { get; set; }
        
        [JsonPropertyName("clientInfo")]
        public McpClientInfo? ClientInfo { get; set; }
    }

    public class McpInitializeResult
    {
        [JsonPropertyName("protocolVersion")]
        public string ProtocolVersion { get; set; } = "";
        
        [JsonPropertyName("serverInfo")]
        public McpServerInfo? ServerInfo { get; set; }
        
        [JsonPropertyName("capabilities")]
        public McpServerCapabilities? Capabilities { get; set; }
    }

    public class McpClientInfo
    {
        [JsonPropertyName("name")]
        public string? Name { get; set; }
        
        [JsonPropertyName("version")]
        public string? Version { get; set; }
    }

    public class McpServerInfo
    {
        [JsonPropertyName("name")]
        public string? Name { get; set; }
        
        [JsonPropertyName("version")]
        public string? Version { get; set; }
    }

    public class McpClientCapabilities
    {
        [JsonPropertyName("experimental")]
        public object? Experimental { get; set; }
        
        [JsonPropertyName("sampling")]
        public object? Sampling { get; set; }
    }

    public class McpServerCapabilities
    {
        [JsonPropertyName("experimental")]
        public object? Experimental { get; set; }
        
        [JsonPropertyName("logging")]
        public object? Logging { get; set; }
        
        [JsonPropertyName("prompts")]
        public McpPromptsCapability? Prompts { get; set; }
        
        [JsonPropertyName("resources")]
        public McpResourcesCapability? Resources { get; set; }
        
        [JsonPropertyName("tools")]
        public McpToolsCapability? Tools { get; set; }
    }

    public class McpPromptsCapability
    {
        [JsonPropertyName("listChanged")]
        public bool ListChanged { get; set; }
    }

    public class McpResourcesCapability
    {
        [JsonPropertyName("subscribe")]
        public bool Subscribe { get; set; }
        
        [JsonPropertyName("listChanged")]
        public bool ListChanged { get; set; }
    }

    public class McpToolsCapability
    {
        [JsonPropertyName("listChanged")]
        public bool ListChanged { get; set; }
    }

    // Resources
    public class McpResourcesListResult
    {
        [JsonPropertyName("resources")]
        public McpResource[]? Resources { get; set; }
    }

    public class McpResource
    {
        [JsonPropertyName("uri")]
        public string Uri { get; set; } = "";
        
        [JsonPropertyName("name")]
        public string Name { get; set; } = "";
        
        [JsonPropertyName("description")]
        public string? Description { get; set; }
        
        [JsonPropertyName("mimeType")]
        public string? MimeType { get; set; }
    }

    public class McpResourcesReadParams
    {
        [JsonPropertyName("uri")]
        public string Uri { get; set; } = "";
    }

    public class McpResourcesReadResult
    {
        [JsonPropertyName("contents")]
        public McpResourceContent[]? Contents { get; set; }
    }

    public class McpResourceContent
    {
        [JsonPropertyName("uri")]
        public string Uri { get; set; } = "";
        
        [JsonPropertyName("mimeType")]
        public string? MimeType { get; set; }
        
        [JsonPropertyName("text")]
        public string? Text { get; set; }
        
        [JsonPropertyName("blob")]
        public byte[]? Blob { get; set; }
    }

    // Tools
    public class McpToolsListResult
    {
        [JsonPropertyName("tools")]
        public McpTool[]? Tools { get; set; }
    }

    public class McpTool
    {
        [JsonPropertyName("name")]
        public string Name { get; set; } = "";
        
        [JsonPropertyName("description")]
        public string? Description { get; set; }
        
        [JsonPropertyName("inputSchema")]
        public object? InputSchema { get; set; }
    }

    public class McpToolsCallParams
    {
        [JsonPropertyName("name")]
        public string Name { get; set; } = "";
        
        [JsonPropertyName("arguments")]
        public System.Text.Json.JsonElement Arguments { get; set; }
    }

    public class McpToolsCallResult
    {
        [JsonPropertyName("content")]
        public McpContent[]? Content { get; set; }
        
        [JsonPropertyName("isError")]
        public bool IsError { get; set; }
    }

    public class McpContent
    {
        [JsonPropertyName("type")]
        public string Type { get; set; } = "";
        
        [JsonPropertyName("text")]
        public string? Text { get; set; }
    }

    // Prompts
    public class McpPromptsListResult
    {
        [JsonPropertyName("prompts")]
        public McpPrompt[]? Prompts { get; set; }
    }

    public class McpPrompt
    {
        [JsonPropertyName("name")]
        public string Name { get; set; } = "";
        
        [JsonPropertyName("description")]
        public string? Description { get; set; }
        
        [JsonPropertyName("arguments")]
        public McpPromptArgument[]? Arguments { get; set; }
    }

    public class McpPromptArgument
    {
        [JsonPropertyName("name")]
        public string Name { get; set; } = "";
        
        [JsonPropertyName("description")]
        public string? Description { get; set; }
        
        [JsonPropertyName("required")]
        public bool Required { get; set; }
    }

    public class McpPromptsGetParams
    {
        [JsonPropertyName("name")]
        public string Name { get; set; } = "";
        
        [JsonPropertyName("arguments")]
        public Dictionary<string, string>? Arguments { get; set; }
    }

    public class McpPromptsGetResult
    {
        [JsonPropertyName("description")]
        public string? Description { get; set; }
        
        [JsonPropertyName("messages")]
        public McpPromptMessage[]? Messages { get; set; }
    }

    public class McpPromptMessage
    {
        [JsonPropertyName("role")]
        public string Role { get; set; } = "";
        
        [JsonPropertyName("content")]
        public McpContent? Content { get; set; }
    }
}