using System.Net.WebSockets;
using System.Text;
using System.Text.Json;
using Microsoft.AspNetCore.Builder;
using Microsoft.Extensions.DependencyInjection;
using Microsoft.Extensions.Hosting;
using Foundation.Data.Doublets.Cli;
using System.IO;
using Xunit;
using Platform.Data.Doublets;

namespace Foundation.Data.Doublets.Cli.Tests
{
    public class ServerModeTests : IDisposable
    {
        private string _tempDbFile;

        public ServerModeTests()
        {
            _tempDbFile = Path.GetTempFileName();
        }

        public void Dispose()
        {
            if (File.Exists(_tempDbFile))
            {
                File.Delete(_tempDbFile);
            }
        }

        [Fact]
        public async Task Should_Accept_WebSocket_Connection()
        {
            // Arrange
            var builder = WebApplication.CreateSlimBuilder();
            builder.Services.AddSingleton(provider => new NamedLinksDecorator<uint>(_tempDbFile, false));
            var app = builder.Build();
            app.UseWebSockets();
            app.Map("/ws", HandleWebSocketEndpoint);
            
            using var server = new TestWebSocketServer(app);
            await server.StartAsync();
            
            // Act & Assert
            using var client = new ClientWebSocket();
            var uri = new Uri($"ws://localhost:{server.Port}/ws");
            
            // Should not throw exception
            await client.ConnectAsync(uri, CancellationToken.None);
            Assert.Equal(WebSocketState.Open, client.State);
            
            await client.CloseAsync(WebSocketCloseStatus.NormalClosure, "", CancellationToken.None);
        }

        [Fact]
        public async Task Should_Process_LiNo_Query_Via_WebSocket()
        {
            // Arrange
            var builder = WebApplication.CreateSlimBuilder();
            builder.Services.AddSingleton(provider => new NamedLinksDecorator<uint>(_tempDbFile, false));
            var app = builder.Build();
            app.UseWebSockets();
            app.Map("/ws", HandleWebSocketEndpoint);
            
            using var server = new TestWebSocketServer(app);
            await server.StartAsync();
            
            using var client = new ClientWebSocket();
            var uri = new Uri($"ws://localhost:{server.Port}/ws");
            await client.ConnectAsync(uri, CancellationToken.None);
            
            // Act - Send a create link query
            var query = "() ((1 1))";
            var queryBytes = Encoding.UTF8.GetBytes(query);
            await client.SendAsync(new ArraySegment<byte>(queryBytes), WebSocketMessageType.Text, true, CancellationToken.None);
            
            // Assert - Receive and verify response
            var buffer = new byte[1024 * 4];
            var result = await client.ReceiveAsync(new ArraySegment<byte>(buffer), CancellationToken.None);
            
            Assert.Equal(WebSocketMessageType.Text, result.MessageType);
            
            var responseText = Encoding.UTF8.GetString(buffer, 0, result.Count);
            var response = JsonSerializer.Deserialize<JsonElement>(responseText);
            
            Assert.True(response.TryGetProperty("query", out var queryProp));
            Assert.Equal(query, queryProp.GetString());
            
            Assert.True(response.TryGetProperty("changes", out var changesProp));
            Assert.True(changesProp.GetArrayLength() > 0);
            
            Assert.True(response.TryGetProperty("links", out var linksProp));
            Assert.True(linksProp.GetArrayLength() > 0);
            
            await client.CloseAsync(WebSocketCloseStatus.NormalClosure, "", CancellationToken.None);
        }

        [Fact]
        public async Task Should_Handle_Multiple_Queries()
        {
            // Arrange
            var builder = WebApplication.CreateSlimBuilder();
            builder.Services.AddSingleton(provider => new NamedLinksDecorator<uint>(_tempDbFile, false));
            var app = builder.Build();
            app.UseWebSockets();
            app.Map("/ws", HandleWebSocketEndpoint);
            
            using var server = new TestWebSocketServer(app);
            await server.StartAsync();
            
            using var client = new ClientWebSocket();
            var uri = new Uri($"ws://localhost:{server.Port}/ws");
            await client.ConnectAsync(uri, CancellationToken.None);
            
            // Act & Assert - Send multiple queries
            var queries = new[] { "() ((1 1))", "() ((2 2))", "((($i:)) (($i:)))" };
            
            foreach (var query in queries)
            {
                var queryBytes = Encoding.UTF8.GetBytes(query);
                await client.SendAsync(new ArraySegment<byte>(queryBytes), WebSocketMessageType.Text, true, CancellationToken.None);
                
                var buffer = new byte[1024 * 4];
                var result = await client.ReceiveAsync(new ArraySegment<byte>(buffer), CancellationToken.None);
                
                Assert.Equal(WebSocketMessageType.Text, result.MessageType);
                
                var responseText = Encoding.UTF8.GetString(buffer, 0, result.Count);
                var response = JsonSerializer.Deserialize<JsonElement>(responseText);
                
                Assert.True(response.TryGetProperty("query", out var queryProp));
                Assert.Equal(query, queryProp.GetString());
            }
            
            await client.CloseAsync(WebSocketCloseStatus.NormalClosure, "", CancellationToken.None);
        }

        [Fact]
        public async Task Should_Return_Error_For_Invalid_Query()
        {
            // Arrange
            var builder = WebApplication.CreateSlimBuilder();
            builder.Services.AddSingleton(provider => new NamedLinksDecorator<uint>(_tempDbFile, false));
            var app = builder.Build();
            app.UseWebSockets();
            app.Map("/ws", HandleWebSocketEndpoint);
            
            using var server = new TestWebSocketServer(app);
            await server.StartAsync();
            
            using var client = new ClientWebSocket();
            var uri = new Uri($"ws://localhost:{server.Port}/ws");
            await client.ConnectAsync(uri, CancellationToken.None);
            
            // Act - Send an invalid query
            var invalidQuery = "((invalid query))";
            var queryBytes = Encoding.UTF8.GetBytes(invalidQuery);
            await client.SendAsync(new ArraySegment<byte>(queryBytes), WebSocketMessageType.Text, true, CancellationToken.None);
            
            // Assert - Should receive error response
            var buffer = new byte[1024 * 4];
            var result = await client.ReceiveAsync(new ArraySegment<byte>(buffer), CancellationToken.None);
            
            Assert.Equal(WebSocketMessageType.Text, result.MessageType);
            
            var responseText = Encoding.UTF8.GetString(buffer, 0, result.Count);
            var response = JsonSerializer.Deserialize<JsonElement>(responseText);
            
            Assert.True(response.TryGetProperty("error", out _));
            
            await client.CloseAsync(WebSocketCloseStatus.NormalClosure, "", CancellationToken.None);
        }

        [Fact]
        public async Task Should_Reject_Non_WebSocket_Requests()
        {
            // Arrange
            var builder = WebApplication.CreateSlimBuilder();
            builder.Services.AddSingleton(provider => new NamedLinksDecorator<uint>(_tempDbFile, false));
            var app = builder.Build();
            app.UseWebSockets();
            app.Map("/ws", HandleWebSocketEndpoint);
            
            using var server = new TestWebSocketServer(app);
            await server.StartAsync();
            
            // Act & Assert - Regular HTTP request should be rejected
            using var httpClient = new HttpClient();
            var response = await httpClient.GetAsync($"http://localhost:{server.Port}/ws");
            
            Assert.Equal(System.Net.HttpStatusCode.BadRequest, response.StatusCode);
        }

        // Helper method - extracted from Program.cs
        private static async Task HandleWebSocketEndpoint(Microsoft.AspNetCore.Http.HttpContext context)
        {
            if (!context.WebSockets.IsWebSocketRequest)
            {
                context.Response.StatusCode = 400;
                return;
            }

            var decoratedLinks = context.RequestServices.GetRequiredService<NamedLinksDecorator<uint>>();
            using var webSocket = await context.WebSockets.AcceptWebSocketAsync();
            await HandleWebSocketConnection(webSocket, decoratedLinks);
        }

        // Helper method - extracted from Program.cs
        private static async Task HandleWebSocketConnection(WebSocket webSocket, NamedLinksDecorator<uint> decoratedLinks)
        {
            var buffer = new byte[1024 * 4];

            while (webSocket.State == WebSocketState.Open)
            {
                try
                {
                    var result = await webSocket.ReceiveAsync(new ArraySegment<byte>(buffer), CancellationToken.None);

                    if (result.MessageType == WebSocketMessageType.Close)
                    {
                        await webSocket.CloseAsync(WebSocketCloseStatus.NormalClosure, "", CancellationToken.None);
                        break;
                    }

                    if (result.MessageType == WebSocketMessageType.Text)
                    {
                        var query = Encoding.UTF8.GetString(buffer, 0, result.Count);
                        var response = await ProcessLinoQuery(query, decoratedLinks);

                        var responseBytes = Encoding.UTF8.GetBytes(response);
                        await webSocket.SendAsync(
                            new ArraySegment<byte>(responseBytes),
                            WebSocketMessageType.Text,
                            true,
                            CancellationToken.None
                        );
                    }
                }
                catch (WebSocketException)
                {
                    break;
                }
                catch (Exception ex)
                {
                    var errorResponse = JsonSerializer.Serialize(new { error = ex.Message });
                    var errorBytes = Encoding.UTF8.GetBytes(errorResponse);

                    try
                    {
                        await webSocket.SendAsync(
                            new ArraySegment<byte>(errorBytes),
                            WebSocketMessageType.Text,
                            true,
                            CancellationToken.None
                        );
                    }
                    catch (WebSocketException)
                    {
                        break;
                    }
                }
            }
        }

        // Helper method - extracted from Program.cs
        private static async Task<string> ProcessLinoQuery(string query, NamedLinksDecorator<uint> decoratedLinks)
        {
            return await Task.Run(() =>
            {
                try
                {
                    var changesList = new List<(Platform.Data.Doublets.Link<uint> Before, Platform.Data.Doublets.Link<uint> After)>();
                    var results = new List<string>();

                    if (!string.IsNullOrWhiteSpace(query))
                    {
                        var options = new AdvancedMixedQueryProcessor.Options
                        {
                            Query = query,
                            Trace = false,
                            ChangesHandler = (beforeLink, afterLink) =>
                            {
                                changesList.Add((new Platform.Data.Doublets.Link<uint>(beforeLink), new Platform.Data.Doublets.Link<uint>(afterLink)));
                                return decoratedLinks.Constants.Continue;
                            }
                        };

                        AdvancedMixedQueryProcessor.ProcessQuery(decoratedLinks, options);

                        // Collect current state of all links
                        var any = decoratedLinks.Constants.Any;
                        var linkQuery = new Platform.Data.Doublets.Link<uint>(index: any, source: any, target: any);

                        decoratedLinks.Each(linkQuery, link =>
                        {
                            var formattedLink = decoratedLinks.Format(link);
                            results.Add(formattedLink); // Note: Namify is not included in tests for simplicity
                            return decoratedLinks.Constants.Continue;
                        });
                    }

                    return JsonSerializer.Serialize(new
                    {
                        query,
                        changes = changesList.Select(change => new
                        {
                            before = change.Before.IsNull() ? null : decoratedLinks.Format(change.Before),
                            after = change.After.IsNull() ? null : decoratedLinks.Format(change.After)
                        }),
                        links = results
                    });
                }
                catch (Exception ex)
                {
                    return JsonSerializer.Serialize(new { error = ex.Message });
                }
            });
        }
    }

    // Helper class for testing WebSocket server
    public class TestWebSocketServer : IDisposable
    {
        private readonly WebApplication _app;
        private Task? _serverTask;
        private readonly CancellationTokenSource _cancellationTokenSource = new();

        public int Port { get; private set; }

        public TestWebSocketServer(WebApplication app)
        {
            _app = app;
            Port = GetAvailablePort();
        }

        public async Task StartAsync()
        {
            _app.Urls.Add($"http://localhost:{Port}");
            _serverTask = _app.RunAsync(_cancellationTokenSource.Token);
            await Task.Delay(500); // Wait for server to start
        }

        private static int GetAvailablePort()
        {
            using var socket = new System.Net.Sockets.TcpListener(System.Net.IPAddress.Any, 0);
            socket.Start();
            var port = ((System.Net.IPEndPoint)socket.LocalEndpoint).Port;
            socket.Stop();
            return port;
        }

        public void Dispose()
        {
            _cancellationTokenSource.Cancel();
            _serverTask?.Wait(TimeSpan.FromSeconds(5));
            _app.DisposeAsync().AsTask().Wait();
            _cancellationTokenSource.Dispose();
        }
    }
}