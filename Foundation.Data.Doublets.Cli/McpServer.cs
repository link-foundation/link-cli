using System.Text.Json;
using System.Text.Json.Serialization;
using StreamJsonRpc;
using Platform.Data.Doublets;
using Platform.Data;
using DoubletLink = Platform.Data.Doublets.Link<uint>;

namespace Foundation.Data.Doublets.Cli
{
    public class McpServer
    {
        private readonly NamedLinksDecorator<uint> _links;
        private readonly bool _tracingEnabled;

        public McpServer(NamedLinksDecorator<uint> links, bool tracingEnabled = false)
        {
            _links = links;
            _tracingEnabled = tracingEnabled;
        }

        // MCP Initialization
        [JsonRpcMethod("initialize")]
        public McpInitializeResult Initialize(McpInitializeParams parameters)
        {
            if (_tracingEnabled)
                Console.WriteLine($"[MCP] Initialize called with clientInfo: {parameters.ClientInfo?.Name}");

            return new McpInitializeResult
            {
                ProtocolVersion = "2024-11-05",
                ServerInfo = new McpServerInfo
                {
                    Name = "link-cli-mcp",
                    Version = "1.0.0"
                },
                Capabilities = new McpServerCapabilities
                {
                    Resources = new McpResourcesCapability
                    {
                        Subscribe = false,
                        ListChanged = false
                    },
                    Tools = new McpToolsCapability
                    {
                        ListChanged = false
                    },
                    Prompts = new McpPromptsCapability
                    {
                        ListChanged = false
                    }
                }
            };
        }

        // Resources: Expose links data for reading
        [JsonRpcMethod("resources/list")]
        public McpResourcesListResult ListResources()
        {
            if (_tracingEnabled)
                Console.WriteLine("[MCP] resources/list called");

            return new McpResourcesListResult
            {
                Resources = new[]
                {
                    new McpResource
                    {
                        Uri = "memory://links/all",
                        Name = "All Memory Links",
                        Description = "All links stored in the neural network memory",
                        MimeType = "application/json"
                    },
                    new McpResource
                    {
                        Uri = "memory://links/search",
                        Name = "Search Memory Links",
                        Description = "Search links by pattern or content",
                        MimeType = "application/json"
                    }
                }
            };
        }

        [JsonRpcMethod("resources/read")]
        public McpResourcesReadResult ReadResource(McpResourcesReadParams parameters)
        {
            if (_tracingEnabled)
                Console.WriteLine($"[MCP] resources/read called for: {parameters.Uri}");

            return parameters.Uri switch
            {
                "memory://links/all" => ReadAllLinks(),
                "memory://links/search" => ReadSearchableLinks(),
                _ => throw new InvalidOperationException($"Unknown resource: {parameters.Uri}")
            };
        }

        private McpResourcesReadResult ReadAllLinks()
        {
            var linksList = new List<object>();
            var any = _links.Constants.Any;
            var query = new DoubletLink(index: any, source: any, target: any);

            _links.Each(query, link =>
            {
                var doubletLink = new DoubletLink(link);
                var name = _links.GetName(doubletLink.Index);
                linksList.Add(new
                {
                    id = doubletLink.Index.ToString(),
                    source = doubletLink.Source.ToString(),
                    target = doubletLink.Target.ToString(),
                    name = name,
                    formatted = _links.Format(link)
                });
                return _links.Constants.Continue;
            });

            return new McpResourcesReadResult
            {
                Contents = new[]
                {
                    new McpResourceContent
                    {
                        Uri = "memory://links/all",
                        MimeType = "application/json",
                        Text = JsonSerializer.Serialize(new { links = linksList }, new JsonSerializerOptions { WriteIndented = true })
                    }
                }
            };
        }

        private McpResourcesReadResult ReadSearchableLinks()
        {
            var searchInfo = new
            {
                description = "Use the search_memory tool to find specific links by pattern or content",
                usage = "Call the search_memory tool with your search criteria"
            };

            return new McpResourcesReadResult
            {
                Contents = new[]
                {
                    new McpResourceContent
                    {
                        Uri = "memory://links/search",
                        MimeType = "application/json",
                        Text = JsonSerializer.Serialize(searchInfo, new JsonSerializerOptions { WriteIndented = true })
                    }
                }
            };
        }

        // Tools: Expose CRUD operations
        [JsonRpcMethod("tools/list")]
        public McpToolsListResult ListTools()
        {
            if (_tracingEnabled)
                Console.WriteLine("[MCP] tools/list called");

            return new McpToolsListResult
            {
                Tools = new[]
                {
                    new McpTool
                    {
                        Name = "store_memory",
                        Description = "Store information as links in neural network memory",
                        InputSchema = new
                        {
                            type = "object",
                            properties = new
                            {
                                content = new { type = "string", description = "Content to store in memory" },
                                name = new { type = "string", description = "Optional name/label for the memory" },
                                source = new { type = "string", description = "Optional source link ID" },
                                target = new { type = "string", description = "Optional target link ID" }
                            },
                            required = new[] { "content" }
                        }
                    },
                    new McpTool
                    {
                        Name = "search_memory",
                        Description = "Search for information in neural network memory",
                        InputSchema = new
                        {
                            type = "object",
                            properties = new
                            {
                                query = new { type = "string", description = "Search query or pattern" },
                                name = new { type = "string", description = "Search by name/label" },
                                source = new { type = "string", description = "Filter by source link ID" },
                                target = new { type = "string", description = "Filter by target link ID" }
                            }
                        }
                    },
                    new McpTool
                    {
                        Name = "update_memory",
                        Description = "Update existing memory links",
                        InputSchema = new
                        {
                            type = "object",
                            properties = new
                            {
                                id = new { type = "string", description = "ID of the link to update" },
                                source = new { type = "string", description = "New source link ID" },
                                target = new { type = "string", description = "New target link ID" },
                                name = new { type = "string", description = "New name/label" }
                            },
                            required = new[] { "id" }
                        }
                    },
                    new McpTool
                    {
                        Name = "delete_memory",
                        Description = "Delete memory links",
                        InputSchema = new
                        {
                            type = "object",
                            properties = new
                            {
                                id = new { type = "string", description = "ID of the link to delete" },
                                source = new { type = "string", description = "Delete by source link ID" },
                                target = new { type = "string", description = "Delete by target link ID" },
                                name = new { type = "string", description = "Delete by name/label" }
                            }
                        }
                    }
                }
            };
        }

        [JsonRpcMethod("tools/call")]
        public McpToolsCallResult CallTool(McpToolsCallParams parameters)
        {
            if (_tracingEnabled)
                Console.WriteLine($"[MCP] tools/call called: {parameters.Name}");

            return parameters.Name switch
            {
                "store_memory" => StoreMemory(parameters.Arguments),
                "search_memory" => SearchMemory(parameters.Arguments),
                "update_memory" => UpdateMemory(parameters.Arguments),
                "delete_memory" => DeleteMemory(parameters.Arguments),
                _ => throw new InvalidOperationException($"Unknown tool: {parameters.Name}")
            };
        }

        private McpToolsCallResult StoreMemory(JsonElement arguments)
        {
            var content = arguments.GetProperty("content").GetString() ?? throw new ArgumentException("content is required");
            var name = arguments.TryGetProperty("name", out var nameElement) ? nameElement.GetString() : null;
            var sourceStr = arguments.TryGetProperty("source", out var sourceElement) ? sourceElement.GetString() : null;
            var targetStr = arguments.TryGetProperty("target", out var targetElement) ? targetElement.GetString() : null;

            uint source = sourceStr != null && uint.TryParse(sourceStr, out var s) ? s : 1;
            uint target = targetStr != null && uint.TryParse(targetStr, out var t) ? t : 1;

            // Create the link
            var createdLink = _links.GetOrCreate(source, target);

            // Set name if provided
            if (!string.IsNullOrWhiteSpace(name))
            {
                _links.SetName(createdLink, name);
            }

            return new McpToolsCallResult
            {
                Content = new[]
                {
                    new McpContent
                    {
                        Type = "text",
                        Text = $"Memory stored successfully. Link ID: {createdLink}, Content: {content}" + 
                               (name != null ? $", Name: {name}" : "")
                    }
                }
            };
        }

        private McpToolsCallResult SearchMemory(JsonElement arguments)
        {
            var results = new List<object>();
            var any = _links.Constants.Any;

            // Handle search by name
            if (arguments.TryGetProperty("name", out var nameElement) && nameElement.ValueKind == JsonValueKind.String)
            {
                var searchName = nameElement.GetString()!;
                var linkId = _links.GetByName(searchName);
                if (!_links.Constants.Null.Equals(linkId))
                {
                    var linkData = _links.GetLink(linkId);
                    var link = new DoubletLink(linkData);
                    results.Add(new
                    {
                        id = link.Index.ToString(),
                        source = link.Source.ToString(),
                        target = link.Target.ToString(),
                        name = _links.GetName(link.Index),
                        formatted = _links.Format(linkData)
                    });
                }
            }
            else
            {
                // Search all links
                var query = new DoubletLink(index: any, source: any, target: any);
                _links.Each(query, link =>
                {
                    var doubletLink = new DoubletLink(link);
                    var linkName = _links.GetName(doubletLink.Index);
                    var formatted = _links.Format(link);
                    
                    // Apply filters if provided
                    bool matches = true;
                    
                    if (arguments.TryGetProperty("query", out var queryElement) && queryElement.ValueKind == JsonValueKind.String)
                    {
                        var searchQuery = queryElement.GetString()!;
                        matches = linkName?.Contains(searchQuery, StringComparison.OrdinalIgnoreCase) == true ||
                                 formatted.Contains(searchQuery, StringComparison.OrdinalIgnoreCase);
                    }

                    if (matches)
                    {
                        results.Add(new
                        {
                            id = doubletLink.Index.ToString(),
                            source = doubletLink.Source.ToString(),
                            target = doubletLink.Target.ToString(),
                            name = linkName,
                            formatted = formatted
                        });
                    }
                    
                    return _links.Constants.Continue;
                });
            }

            return new McpToolsCallResult
            {
                Content = new[]
                {
                    new McpContent
                    {
                        Type = "text",
                        Text = $"Found {results.Count} memory links:\n" +
                               JsonSerializer.Serialize(results, new JsonSerializerOptions { WriteIndented = true })
                    }
                }
            };
        }

        private McpToolsCallResult UpdateMemory(JsonElement arguments)
        {
            if (!arguments.TryGetProperty("id", out var idElement) || !uint.TryParse(idElement.GetString(), out var linkId))
            {
                throw new ArgumentException("Valid link id is required");
            }

            var existingLinkData = _links.GetLink(linkId);
            var existingLink = new DoubletLink(existingLinkData);
            var newSource = existingLink.Source;
            var newTarget = existingLink.Target;

            if (arguments.TryGetProperty("source", out var sourceElement) && uint.TryParse(sourceElement.GetString(), out var s))
                newSource = s;
            
            if (arguments.TryGetProperty("target", out var targetElement) && uint.TryParse(targetElement.GetString(), out var t))
                newTarget = t;

            // Update the link
            var restriction = new DoubletLink(linkId, existingLink.Source, existingLink.Target);
            var substitution = new DoubletLink(linkId, newSource, newTarget);
            _links.Update(restriction, substitution, null);

            // Update name if provided
            if (arguments.TryGetProperty("name", out var nameElement) && nameElement.ValueKind == JsonValueKind.String)
            {
                var newName = nameElement.GetString()!;
                _links.SetName(linkId, newName);
            }

            return new McpToolsCallResult
            {
                Content = new[]
                {
                    new McpContent
                    {
                        Type = "text",
                        Text = $"Memory link {linkId} updated successfully"
                    }
                }
            };
        }

        private McpToolsCallResult DeleteMemory(JsonElement arguments)
        {
            if (arguments.TryGetProperty("id", out var idElement) && uint.TryParse(idElement.GetString(), out var linkId))
            {
                var linkData = _links.GetLink(linkId);
                var link = new DoubletLink(linkData);
                var restriction = new DoubletLink(linkId, link.Source, link.Target);
                _links.Delete(restriction, null);
                
                return new McpToolsCallResult
                {
                    Content = new[]
                    {
                        new McpContent
                        {
                            Type = "text",
                            Text = $"Memory link {linkId} deleted successfully"
                        }
                    }
                };
            }
            else if (arguments.TryGetProperty("name", out var nameElement) && nameElement.ValueKind == JsonValueKind.String)
            {
                var name = nameElement.GetString()!;
                var deleteLinkId = _links.GetByName(name);
                if (!_links.Constants.Null.Equals(deleteLinkId))
                {
                    var linkData = _links.GetLink(deleteLinkId);
                    var link = new DoubletLink(linkData);
                    var restriction = new DoubletLink(deleteLinkId, link.Source, link.Target);
                    _links.Delete(restriction, null);
                    
                    return new McpToolsCallResult
                    {
                        Content = new[]
                        {
                            new McpContent
                            {
                                Type = "text",
                                Text = $"Memory link '{name}' deleted successfully"
                            }
                        }
                    };
                }
                else
                {
                    return new McpToolsCallResult
                    {
                        Content = new[]
                        {
                            new McpContent
                            {
                                Type = "text",
                                Text = $"Memory link with name '{name}' not found"
                            }
                        }
                    };
                }
            }
            
            throw new ArgumentException("Either 'id' or 'name' is required for deletion");
        }

        // Prompts: Common neural network memory operations
        [JsonRpcMethod("prompts/list")]
        public McpPromptsListResult ListPrompts()
        {
            if (_tracingEnabled)
                Console.WriteLine("[MCP] prompts/list called");

            return new McpPromptsListResult
            {
                Prompts = new[]
                {
                    new McpPrompt
                    {
                        Name = "remember_context",
                        Description = "Store conversational context in neural network memory",
                        Arguments = new[]
                        {
                            new McpPromptArgument
                            {
                                Name = "context",
                                Description = "The context or information to remember",
                                Required = true
                            },
                            new McpPromptArgument
                            {
                                Name = "importance",
                                Description = "Importance level (1-10)",
                                Required = false
                            }
                        }
                    },
                    new McpPrompt
                    {
                        Name = "recall_similar",
                        Description = "Find similar memories based on content",
                        Arguments = new[]
                        {
                            new McpPromptArgument
                            {
                                Name = "query",
                                Description = "What to search for in memory",
                                Required = true
                            }
                        }
                    }
                }
            };
        }

        [JsonRpcMethod("prompts/get")]
        public McpPromptsGetResult GetPrompt(McpPromptsGetParams parameters)
        {
            if (_tracingEnabled)
                Console.WriteLine($"[MCP] prompts/get called: {parameters.Name}");

            return parameters.Name switch
            {
                "remember_context" => GetRememberContextPrompt(parameters.Arguments),
                "recall_similar" => GetRecallSimilarPrompt(parameters.Arguments),
                _ => throw new InvalidOperationException($"Unknown prompt: {parameters.Name}")
            };
        }

        private McpPromptsGetResult GetRememberContextPrompt(Dictionary<string, string>? arguments)
        {
            var context = arguments?.GetValueOrDefault("context", "");
            var importance = arguments?.GetValueOrDefault("importance", "5");

            return new McpPromptsGetResult
            {
                Description = "Store important context in neural network memory for future reference",
                Messages = new[]
                {
                    new McpPromptMessage
                    {
                        Role = "user",
                        Content = new McpContent
                        {
                            Type = "text",
                            Text = $"Please store this context in memory with importance level {importance}: {context}\n\n" +
                                   "Use the store_memory tool to save this information so it can be recalled later."
                        }
                    }
                }
            };
        }

        private McpPromptsGetResult GetRecallSimilarPrompt(Dictionary<string, string>? arguments)
        {
            var query = arguments?.GetValueOrDefault("query", "");

            return new McpPromptsGetResult
            {
                Description = "Search for similar memories based on the provided query",
                Messages = new[]
                {
                    new McpPromptMessage
                    {
                        Role = "user",
                        Content = new McpContent
                        {
                            Type = "text",
                            Text = $"Please search my memory for information related to: {query}\n\n" +
                                   "Use the search_memory tool to find relevant stored information."
                        }
                    }
                }
            };
        }
    }
}