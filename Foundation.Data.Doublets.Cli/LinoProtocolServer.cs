using System.Net;
using System.Net.Sockets;
using System.Text;
using System.Text.Json;
using Platform.Data.Doublets;
using Platform.Protocols.Lino;

namespace Foundation.Data.Doublets.Cli
{
    public class LinoProtocolServer : IDisposable
    {
        private readonly NamedLinksDecorator<uint> _links;
        private readonly TcpListener _listener;
        private readonly CancellationTokenSource _cancellationTokenSource;
        private bool _disposed = false;

        public LinoProtocolServer(NamedLinksDecorator<uint> links, IPAddress address, int port)
        {
            _links = links ?? throw new ArgumentNullException(nameof(links));
            _listener = new TcpListener(address, port);
            _cancellationTokenSource = new CancellationTokenSource();
        }

        public async Task StartAsync()
        {
            _listener.Start();
            Console.WriteLine($"LiNo Protocol Server started on {_listener.LocalEndpoint}");

            try
            {
                while (!_cancellationTokenSource.Token.IsCancellationRequested)
                {
                    var tcpClient = await _listener.AcceptTcpClientAsync();
                    Task.Run(async () => await HandleClientAsync(tcpClient, _cancellationTokenSource.Token));
                }
            }
            catch (ObjectDisposedException)
            {
                // Expected when stopping
            }
        }

        public void Stop()
        {
            _cancellationTokenSource.Cancel();
            _listener?.Stop();
        }

        private async Task HandleClientAsync(TcpClient client, CancellationToken cancellationToken)
        {
            using (client)
            using (var stream = client.GetStream())
            using (var reader = new StreamReader(stream, Encoding.UTF8))
            using (var writer = new StreamWriter(stream, Encoding.UTF8))
            {
                try
                {
                    while (!cancellationToken.IsCancellationRequested && client.Connected)
                    {
                        var request = await reader.ReadLineAsync();
                        if (string.IsNullOrEmpty(request))
                            break;

                        var response = await ProcessRequestAsync(request);
                        await writer.WriteLineAsync(response);
                        await writer.FlushAsync();
                    }
                }
                catch (Exception ex)
                {
                    Console.WriteLine($"Error handling client: {ex.Message}");
                }
            }
        }

        private async Task<string> ProcessRequestAsync(string request)
        {
            try
            {
                var requestObj = JsonSerializer.Deserialize<LinoRequest>(request);
                if (requestObj == null)
                {
                    return CreateErrorResponse("Invalid request format");
                }

                var stopwatch = System.Diagnostics.Stopwatch.StartNew();
                
                var changesList = new List<(Platform.Data.Doublets.Link<uint> Before, Platform.Data.Doublets.Link<uint> After)>();
                
                var options = new AdvancedMixedQueryProcessor.Options
                {
                    Query = requestObj.Query,
                    Trace = false,
                    ChangesHandler = (beforeLink, afterLink) =>
                    {
                        changesList.Add((new Platform.Data.Doublets.Link<uint>(beforeLink), new Platform.Data.Doublets.Link<uint>(afterLink)));
                        return _links.Constants.Continue;
                    }
                };

                await Task.Run(() => AdvancedMixedQueryProcessor.ProcessQuery(_links, options));
                
                stopwatch.Stop();

                var response = new LinoResponse
                {
                    Success = true,
                    ProcessingTimeMs = stopwatch.ElapsedMilliseconds,
                    ChangesCount = changesList.Count,
                    Changes = changesList.Select(c => new ChangeInfo
                    {
                        Before = c.Before.IsNull() ? null : ((ILinks<uint>)_links).Format(c.Before),
                        After = c.After.IsNull() ? null : ((ILinks<uint>)_links).Format(c.After)
                    }).ToList()
                };

                return JsonSerializer.Serialize(response);
            }
            catch (Exception ex)
            {
                return CreateErrorResponse($"Error processing request: {ex.Message}");
            }
        }

        private string CreateErrorResponse(string error)
        {
            var response = new LinoResponse
            {
                Success = false,
                Error = error
            };
            return JsonSerializer.Serialize(response);
        }

        public void Dispose()
        {
            if (!_disposed)
            {
                _cancellationTokenSource?.Cancel();
                _listener?.Stop();
                _cancellationTokenSource?.Dispose();
                _disposed = true;
            }
        }
    }

    public class LinoRequest
    {
        public string? Query { get; set; }
        public long RequestId { get; set; }
        public DateTime Timestamp { get; set; }
    }

    public class LinoResponse
    {
        public bool Success { get; set; }
        public string? Error { get; set; }
        public long ProcessingTimeMs { get; set; }
        public int ChangesCount { get; set; }
        public List<ChangeInfo>? Changes { get; set; }
    }

    public class ChangeInfo
    {
        public string? Before { get; set; }
        public string? After { get; set; }
    }
}