using System.Net.Sockets;
using System.Text;
using System.Text.Json;

namespace Foundation.Data.Doublets.Cli
{
    public class LinoProtocolClient : IDisposable
    {
        private TcpClient? _tcpClient;
        private NetworkStream? _stream;
        private StreamWriter? _writer;
        private StreamReader? _reader;
        private readonly string _host;
        private readonly int _port;
        private bool _disposed = false;

        public LinoProtocolClient(string host, int port)
        {
            _host = host ?? throw new ArgumentNullException(nameof(host));
            _port = port;
        }

        public async Task ConnectAsync()
        {
            _tcpClient = new TcpClient();
            await _tcpClient.ConnectAsync(_host, _port);
            _stream = _tcpClient.GetStream();
            _writer = new StreamWriter(_stream, Encoding.UTF8);
            _reader = new StreamReader(_stream, Encoding.UTF8);
        }

        public async Task<LinoResponse> SendQueryAsync(string query)
        {
            if (_writer == null || _reader == null)
            {
                throw new InvalidOperationException("Client not connected. Call ConnectAsync first.");
            }

            var request = new LinoRequest
            {
                Query = query,
                RequestId = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds(),
                Timestamp = DateTime.UtcNow
            };

            var requestJson = JsonSerializer.Serialize(request);
            await _writer.WriteLineAsync(requestJson);
            await _writer.FlushAsync();

            var responseJson = await _reader.ReadLineAsync();
            if (string.IsNullOrEmpty(responseJson))
            {
                throw new InvalidOperationException("Received empty response from server");
            }

            var response = JsonSerializer.Deserialize<LinoResponse>(responseJson);
            return response ?? throw new InvalidOperationException("Failed to deserialize server response");
        }

        public void Disconnect()
        {
            _writer?.Dispose();
            _reader?.Dispose();
            _stream?.Dispose();
            _tcpClient?.Dispose();
            _writer = null;
            _reader = null;
            _stream = null;
            _tcpClient = null;
        }

        public void Dispose()
        {
            if (!_disposed)
            {
                Disconnect();
                _disposed = true;
            }
        }
    }
}