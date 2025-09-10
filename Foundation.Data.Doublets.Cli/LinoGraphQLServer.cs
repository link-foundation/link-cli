using Microsoft.AspNetCore.Builder;
using Microsoft.AspNetCore.Hosting;
using Microsoft.AspNetCore.Http;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Microsoft.Extensions.Logging;
using System.Text;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli
{
    public class LinoGraphQLServer
    {
        private readonly NamedLinksDecorator<uint> _links;
        private readonly LinoGraphQLProcessor _processor;
        private readonly int _port;
        private readonly bool _trace;

        public LinoGraphQLServer(NamedLinksDecorator<uint> links, int port = 5000, bool trace = false)
        {
            _links = links;
            _processor = new LinoGraphQLProcessor(links);
            _port = port;
            _trace = trace;
        }

        public async Task StartAsync(CancellationToken cancellationToken = default)
        {
            var builder = WebApplication.CreateBuilder();

            // Configure services
            builder.Services.AddLogging(logging =>
            {
                if (_trace)
                {
                    logging.SetMinimumLevel(LogLevel.Debug);
                }
                logging.AddConsole();
            });

            builder.Services.AddCors(options =>
            {
                options.AddPolicy("AllowAll", policy =>
                {
                    policy.AllowAnyOrigin()
                          .AllowAnyMethod()
                          .AllowAnyHeader();
                });
            });

            // Configure the web host
            builder.WebHost.UseUrls($"http://localhost:{_port}");

            var app = builder.Build();

            // Configure middleware
            app.UseCors("AllowAll");

            // GraphQL endpoint
            app.MapPost("/graphql", HandleGraphQLRequest);
            app.MapGet("/graphql", HandleGraphQLRequest);

            // GraphQL playground/introspection endpoint
            app.MapGet("/", ServePlayground);

            // Schema introspection
            app.MapGet("/schema", HandleSchemaRequest);

            if (_trace)
            {
                Console.WriteLine($"LINO GraphQL API server starting on http://localhost:{_port}");
                Console.WriteLine("Endpoints:");
                Console.WriteLine("  POST /graphql - Main GraphQL endpoint");
                Console.WriteLine("  GET  /graphql - GraphQL GET queries");
                Console.WriteLine("  GET  /       - GraphQL playground");
                Console.WriteLine("  GET  /schema - Schema introspection");
            }

            await app.RunAsync(cancellationToken);
        }

        private async Task<IResult> HandleGraphQLRequest(HttpContext context)
        {
            try
            {
                string queryString = "";
                Dictionary<string, object>? variables = null;
                string? operationName = null;

                if (context.Request.Method == "POST")
                {
                    using var reader = new StreamReader(context.Request.Body);
                    var requestBody = await reader.ReadToEndAsync();

                    if (_trace)
                    {
                        Console.WriteLine($"[GraphQL POST] Request body: {requestBody}");
                    }

                    // Try to parse as JSON first (for compatibility)
                    if (requestBody.TrimStart().StartsWith("{"))
                    {
                        var jsonRequest = JsonSerializer.Deserialize<LinoGraphQLProcessor.GraphQLQuery>(requestBody);
                        queryString = jsonRequest?.Query ?? "";
                        variables = jsonRequest?.Variables;
                        operationName = jsonRequest?.OperationName;
                    }
                    else
                    {
                        // Treat as pure LINO notation
                        queryString = requestBody;
                    }
                }
                else if (context.Request.Method == "GET")
                {
                    queryString = context.Request.Query["query"].ToString();
                    var variablesParam = context.Request.Query["variables"].ToString();
                    if (!string.IsNullOrEmpty(variablesParam))
                    {
                        variables = JsonSerializer.Deserialize<Dictionary<string, object>>(variablesParam);
                    }
                    operationName = context.Request.Query["operationName"].ToString();

                    if (_trace)
                    {
                        Console.WriteLine($"[GraphQL GET] Query: {queryString}");
                    }
                }

                if (string.IsNullOrEmpty(queryString))
                {
                    return Results.BadRequest(new LinoGraphQLProcessor.GraphQLResponse
                    {
                        Errors = new List<LinoGraphQLProcessor.GraphQLError>
                        {
                            new LinoGraphQLProcessor.GraphQLError
                            {
                                Message = "No query provided"
                            }
                        }
                    });
                }

                var result = _processor.ProcessLinoGraphQLQuery(queryString, variables);

                if (_trace)
                {
                    Console.WriteLine($"[GraphQL Response] Data: {result.Data}");
                    if (result.Errors?.Any() == true)
                    {
                        Console.WriteLine($"[GraphQL Response] Errors: {string.Join(", ", result.Errors.Select(e => e.Message))}");
                    }
                }

                // Return response in LINO format by default, but support JSON for compatibility
                var acceptHeader = context.Request.Headers["Accept"].ToString();
                if (acceptHeader.Contains("application/json"))
                {
                    return Results.Json(result);
                }
                else
                {
                    // Return as LINO notation
                    var linoResponse = FormatResponseAsLino(result);
                    return Results.Content(linoResponse, "text/plain; charset=utf-8");
                }
            }
            catch (Exception ex)
            {
                if (_trace)
                {
                    Console.WriteLine($"[GraphQL Error] {ex.Message}");
                    Console.WriteLine($"[GraphQL Error] Stack trace: {ex.StackTrace}");
                }

                var errorResponse = new LinoGraphQLProcessor.GraphQLResponse
                {
                    Errors = new List<LinoGraphQLProcessor.GraphQLError>
                    {
                        new LinoGraphQLProcessor.GraphQLError
                        {
                            Message = ex.Message
                        }
                    }
                };

                return Results.Json(errorResponse);
            }
        }

        private static IResult HandleSchemaRequest(HttpContext context)
        {
            var schema = @"(schema 
  (types 
    (Link 
      (fields 
        (id (type: ID))
        (source (type: ID))
        (target (type: ID))
      )
    )
    (Query
      (fields
        (links (type: (List Link)))
        (link (args (id (type: ID))) (type: Link))
      )
    )
    (Mutation
      (fields
        (createLink (args (source: ID) (target: ID)) (type: Link))
        (updateLink (args (id: ID) (source: ID) (target: ID)) (type: Link))
        (deleteLink (args (id: ID)) (type: Boolean))
      )
    )
  )
)";

            return Results.Content(schema, "text/plain; charset=utf-8");
        }

        private static IResult ServePlayground(HttpContext context)
        {
            var playgroundHtml = @"<!DOCTYPE html>
<html>
<head>
    <title>LINO GraphQL Playground</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; }
        .container { max-width: 1200px; margin: 0 auto; }
        .section { margin-bottom: 30px; }
        textarea { width: 100%; height: 200px; font-family: monospace; }
        button { padding: 10px 20px; background: #007acc; color: white; border: none; border-radius: 4px; cursor: pointer; }
        button:hover { background: #005999; }
        .response { background: #f5f5f5; padding: 20px; border-radius: 4px; white-space: pre-wrap; font-family: monospace; }
        .examples { background: #e8f4f8; padding: 20px; border-radius: 4px; }
        .example { margin-bottom: 10px; cursor: pointer; color: #007acc; text-decoration: underline; }
    </style>
</head>
<body>
    <div class='container'>
        <h1>LINO GraphQL API Playground</h1>
        
        <div class='section'>
            <h2>Try LINO GraphQL Queries</h2>
            <textarea id='query' placeholder='Enter your LINO GraphQL query here...
Example: (query (links (id source target)))'></textarea>
            <br><br>
            <button onclick='executeQuery()'>Execute Query</button>
        </div>

        <div class='section'>
            <h2>Response</h2>
            <div id='response' class='response'>No query executed yet.</div>
        </div>

        <div class='section examples'>
            <h2>Example Queries</h2>
            <div class='example' onclick='loadExample(this.innerText)'>(query (links (id source target)))</div>
            <div class='example' onclick='loadExample(this.innerText)'>(query (link (id: 1) (id source target)))</div>
            <div class='example' onclick='loadExample(this.innerText)'>(query (__schema))</div>
            <div class='example' onclick='loadExample(this.innerText)'>(query (schema))</div>
        </div>

        <div class='section'>
            <h2>About LINO GraphQL API</h2>
            <p>This API uses LINO (Links Notation) instead of JSON for GraphQL queries and responses.</p>
            <p>LINO uses parentheses to represent linked data structures, making it natural for graph operations.</p>
            <ul>
                <li><strong>POST /graphql</strong> - Main GraphQL endpoint</li>
                <li><strong>GET /graphql?query=...</strong> - GraphQL GET queries</li>
                <li><strong>GET /schema</strong> - Schema introspection</li>
            </ul>
        </div>
    </div>

    <script>
        function loadExample(query) {
            document.getElementById('query').value = query;
        }

        async function executeQuery() {
            const query = document.getElementById('query').value;
            const responseDiv = document.getElementById('response');
            
            if (!query.trim()) {
                responseDiv.innerText = 'Please enter a query.';
                return;
            }

            try {
                responseDiv.innerText = 'Executing query...';
                
                const response = await fetch('/graphql', {
                    method: 'POST',
                    headers: {
                        'Content-Type': 'text/plain',
                        'Accept': 'text/plain'
                    },
                    body: query
                });

                const result = await response.text();
                responseDiv.innerText = result;
            } catch (error) {
                responseDiv.innerText = 'Error: ' + error.message;
            }
        }
    </script>
</body>
</html>";

            return Results.Content(playgroundHtml, "text/html");
        }

        private static string FormatResponseAsLino(LinoGraphQLProcessor.GraphQLResponse response)
        {
            if (response.Errors?.Any() == true)
            {
                var errors = string.Join(" ", response.Errors.Select(e => $"(error: \"{e.Message}\")"));
                return $"(response (errors ({errors})))";
            }

            return $"(response (data {response.Data ?? "()"}))";
        }
    }
}