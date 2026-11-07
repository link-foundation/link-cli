using System.Net;
using System.Net.Sockets;
using System.Text;
using Foundation.Data.Doublets.Cli.Graphql;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>
/// The HTTP and WebSocket edges of the GraphQL server. <see cref="GraphqlServerTests"/> drives
/// the installed command; this suite runs the server inside the test process, so odd bytes can
/// be sent at every step. <c>rust/tests/graphql_server_protocol.rs</c> has the same cases.
/// </summary>
public sealed class GraphqlServerProtocolTests
{
    private const string Insert = "mutation { insert_links(objects: {type_id: 1}) { returning { id } } }";

    private const string Watch =
        "subscription { links(where: {type_id: {_eq: 1}}, order_by: {id: desc}, limit: 1) { id } }";

    private static string Package()
    {
        for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory is not null; directory = directory.Parent)
        {
            var path = Path.Combine(directory.FullName, "docs", "protocol", "graphql", "deep-core.json");
            if (File.Exists(path))
            {
                return path;
            }
        }
        throw new FileNotFoundException("deep-core.json");
    }

    private sealed class Running : IDisposable
    {
        private readonly string _database = Path.Combine(Path.GetTempPath(), $"clink-graphql-protocol-{Guid.NewGuid():N}.links");
        private readonly NamedTypesDecorator<uint> _links;
        private readonly GraphqlServer _server;
        private readonly Thread _serving;

        public Running(string? admin = null)
        {
            _links = new NamedTypesDecorator<uint>(_database, false);
            var store = new DeepStore(_links, null);
            store.LoadPackage(Json.Parse(File.ReadAllText(Package())));
            _server = new GraphqlServer(new IPEndPoint(IPAddress.Loopback, 0));
            var settings = new Settings(Encoding.UTF8.GetBytes("s3"), admin);
            _serving = new Thread(() => _server.Serve(store, settings)) { IsBackground = true };
            _serving.Start();
        }

        public IPEndPoint Endpoint => _server.LocalEndpoint;

        public Connection Connect() => new(Endpoint);

        public string PostText(string body, string headers = "")
        {
            using var connection = Connect();
            return connection.Exchange($"POST /v1/graphql HTTP/1.1\r\nHost: x\r\n{headers}Content-Length: {Encoding.UTF8.GetByteCount(body)}\r\n\r\n{body}").Body;
        }

        public Json Post(string query, string headers = "") =>
            Json.Parse(PostText(Json.Object(("query", Json.Str(query))).Write(), headers));

        public string GuestToken() =>
            Post("query { guest { token } }").Get("data")!.Get("guest")!.Get("token")!.AsStr()!;

        public void Dispose()
        {
            _server.Stop();
            _serving.Join();
            _server.Dispose();
            _links.Dispose();
            foreach (var file in Directory.GetFiles(Path.GetTempPath(), Path.GetFileName(_database) + "*"))
            {
                File.Delete(file);
            }
        }
    }

    private class Connection : IDisposable
    {
        private readonly TcpClient _client = new();

        public Connection(IPEndPoint endpoint)
        {
            _client.Connect(endpoint);
            _client.ReceiveTimeout = 20000;
            _client.SendTimeout = 20000;
            Stream = _client.GetStream();
        }

        protected NetworkStream Stream { get; }

        public void Write(byte[] bytes) => Stream.Write(bytes);

        public void EndWriting() => _client.Client.Shutdown(SocketShutdown.Send);

        /// <summary>Sends text and reads one response: its head and body.</summary>
        public (string Head, string Body) Exchange(string request)
        {
            Write(Encoding.UTF8.GetBytes(request));
            var head = new StringBuilder();
            var length = 0;
            while (true)
            {
                var line = ReadLine();
                if (line.Length == 0)
                {
                    break;
                }
                if (line.StartsWith("content-length:", StringComparison.OrdinalIgnoreCase))
                {
                    length = int.Parse(line["content-length:".Length..].Trim(), System.Globalization.CultureInfo.InvariantCulture);
                }
                head.Append(line).Append("\r\n");
            }
            var body = new byte[length];
            Stream.ReadExactly(body);
            return (head.ToString(), Encoding.UTF8.GetString(body));
        }

        /// <summary>Everything the server still sends; a reset counts as hanging up.</summary>
        public int ReadToEnd()
        {
            var total = 0;
            var buffer = new byte[256];
            try
            {
                int read;
                while ((read = Stream.Read(buffer)) > 0)
                {
                    total += read;
                }
            }
            catch (IOException error) when (error.InnerException is SocketException { SocketErrorCode: not SocketError.TimedOut })
            {
            }
            return total;
        }

        private string ReadLine()
        {
            var bytes = new List<byte>();
            while (true)
            {
                var next = Stream.ReadByte();
                if (next < 0)
                {
                    throw new EndOfStreamException();
                }
                if (next == '\n')
                {
                    return Encoding.UTF8.GetString(bytes.ToArray()).TrimEnd('\r');
                }
                bytes.Add((byte)next);
            }
        }

        public void Dispose() => _client.Dispose();
    }

    private static byte[] Frame(byte finAndOpcode, byte[] payload, bool masked = true)
    {
        var bit = masked ? 0x80 : 0;
        var bytes = new List<byte> { finAndOpcode };
        if (payload.Length <= 125)
        {
            bytes.Add((byte)(bit | payload.Length));
        }
        else if (payload.Length <= 65535)
        {
            bytes.Add((byte)(bit | 126));
            bytes.Add((byte)(payload.Length >> 8));
            bytes.Add((byte)payload.Length);
        }
        else
        {
            bytes.Add((byte)(bit | 127));
            bytes.AddRange(Enumerable.Range(0, 8).Select(at => (byte)((long)payload.Length >> (56 - (at * 8)))));
        }
        byte[] mask = [9, 8, 7, 6];
        if (masked)
        {
            bytes.AddRange(mask);
        }
        bytes.AddRange(payload.Select((value, at) => masked ? (byte)(value ^ mask[at % 4]) : value));
        return bytes.ToArray();
    }

    private static byte[] Ascii(string text) => Encoding.UTF8.GetBytes(text);

    private sealed class Socket : Connection
    {
        public Socket(Running server, string path = "/v1/graphql", string lines = "")
            : base(server.Endpoint)
        {
            Head = Exchange($"GET {path} HTTP/1.1\r\nUpgrade: WebSocket\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n{lines}\r\n").Head;
            Assert.StartsWith("HTTP/1.1 101", Head, StringComparison.Ordinal);
        }

        public string Head { get; }

        public static Socket Modern(Running server) =>
            new(server, lines: "Sec-WebSocket-Protocol: graphql-transport-ws\r\n");

        public static Socket Legacy(Running server) => new(server);

        public void Send(string text) => Write(Frame(0x81, Ascii(text)));

        public void Init()
        {
            Send("""{"type":"connection_init"}""");
            Assert.Equal("connection_ack", Kind());
        }

        public void Subscribe(string kind, string id, string query) => Send(Json.Object(
            ("id", Json.Str(id)),
            ("type", Json.Str(kind)),
            ("payload", Json.Object(("query", Json.Str(query))))).Write());

        /// <summary>One frame: its opcode and payload.</summary>
        public (int Opcode, string Payload) Raw()
        {
            var head = new byte[2];
            Stream.ReadExactly(head);
            long length = head[1] & 0x7f;
            if (length is 126 or 127)
            {
                var size = new byte[length == 126 ? 2 : 8];
                Stream.ReadExactly(size);
                length = size.Aggregate(0L, (sum, value) => (sum << 8) | value);
            }
            var payload = new byte[length];
            Stream.ReadExactly(payload);
            return (head[0] & 0x0f, Encoding.UTF8.GetString(payload));
        }

        public Json Message()
        {
            var (opcode, payload) = Raw();
            Assert.Equal(1, opcode);
            return Json.Parse(payload);
        }

        public string Kind() => Message().Get("type")!.AsStr()!;

        public void AssertClosed() => Assert.Equal(0, ReadToEnd());
    }

    [Fact]
    public void RoutesShareOneConnection()
    {
        using var server = new Running();
        using var connection = server.Connect();
        foreach (var path in new[] { "/healthz", "/api/healthz", "/v1/healthz", "/healthz?full=1" })
        {
            var (head, body) = connection.Exchange($"GET {path} HTTP/1.1\r\n\r\n");
            Assert.StartsWith("HTTP/1.1 200", head, StringComparison.Ordinal);
            Assert.Equal("""{"status":"ok"}""", body);
        }
        foreach (var path in new[] { "/v1/version", "/v1/version/" })
        {
            Assert.Equal("""{"version":"clink"}""", connection.Exchange($"GET {path} HTTP/1.1\r\nno colon on this line\r\n\r\n").Body);
        }
        foreach (var request in new[]
        {
            "GET /missing HTTP/1.1\r\n\r\n",
            "POST /missing HTTP/1.1\r\n\r\n",
            "GET /v1/graphql HTTP/1.1\r\n\r\n",
            // Only GraphQL paths upgrade.
            "GET /missing HTTP/1.1\r\nUpgrade: websocket\r\n\r\n",
            // A blank request line is a request for nothing.
            "\r\n\r\n",
        })
        {
            Assert.StartsWith("HTTP/1.1 404", connection.Exchange(request).Head, StringComparison.Ordinal);
        }
    }

    [Fact]
    public void PreflightEchoesWhatItIsAskedAbout()
    {
        using var server = new Running();
        using var connection = server.Connect();
        var head = connection.Exchange("OPTIONS /v1/graphql HTTP/1.1\r\nAccess-Control-Request-Headers: x-mine\r\n\r\n").Head;
        Assert.StartsWith("HTTP/1.1 204", head, StringComparison.Ordinal);
        Assert.Contains("Allow-Headers: x-mine", head, StringComparison.Ordinal);
        Assert.Contains("x-hasura-admin-secret", connection.Exchange("OPTIONS /other HTTP/1.1\r\n\r\n").Head, StringComparison.Ordinal);
    }

    [Fact]
    public void RequestsChooseOperationsAndVariables()
    {
        using var server = new Running();
        const string Body = """{"operationName":"Second","variables":{"id":1},"query":"query First { links(limit: 0) { id } } query Second($id: bigint) { links(where: {id: {_eq: $id}}) { id } }"}""";
        Assert.Equal("""{"data":{"links":[{"id":1}]}}""", server.PostText(Body));
        // Fields of the wrong kind count as absent, and a missing query is refused.
        Assert.Contains("errors", server.PostText("""{"query":1,"operationName":2}"""), StringComparison.Ordinal);
    }

    [Fact]
    public void BadRequestsGetAnErrorAndKeepTheServer()
    {
        using var server = new Running("root");
        Assert.Contains("errors", server.PostText("{not json"), StringComparison.Ordinal);

        const string Guest = "query { guest { token } }";
        Assert.Contains("invalid-jwt", server.Post(Guest, "Authorization: Bearer a.b.c\r\n").Write(), StringComparison.Ordinal);
        Assert.NotNull(server.Post(Guest, "x-hasura-admin-secret: wrong\r\n").Get("errors"));
        Assert.NotNull(server.Post(Guest, "x-hasura-admin-secret: root\r\n").Get("data"));

        // A length that is not a number means no body.
        using (var connection = server.Connect())
        {
            Assert.Contains("errors", connection.Exchange("POST /v1/graphql HTTP/1.1\r\nContent-Length: many\r\n\r\n").Body, StringComparison.Ordinal);
        }
        using (var connection = server.Connect())
        {
            Assert.StartsWith("HTTP/1.1 413", connection.Exchange($"POST /v1/graphql HTTP/1.1\r\nContent-Length: {ulong.MaxValue / 2}\r\n\r\n").Head, StringComparison.Ordinal);
        }

        // Clients that vanish at each step of a request.
        foreach (var partial in new[]
        {
            "",
            "GET /healthz HTTP/1.1\r\n",
            "POST /v1/graphql HTTP/1.1\r\nContent-Length: 5\r\n\r\nab",
        })
        {
            using var connection = server.Connect();
            connection.Write(Ascii(partial));
            connection.EndWriting();
            Assert.Equal(0, connection.ReadToEnd());
        }
        Assert.NotNull(server.Post(Insert).Get("data"));
    }

    [Fact]
    public void EveryGraphqlPathUpgradesAndTheProtocolIsNegotiated()
    {
        using var server = new Running();
        foreach (var path in new[] { "/v1/graphql", "/gql", "/v1alpha1/graphql" })
        {
            using var socket = new Socket(server, path);
            Assert.Contains("Sec-WebSocket-Protocol: graphql-ws\r\n", socket.Head, StringComparison.Ordinal);
            Assert.Contains("s3pPLMBiTxaQ9kYGzzhZRbK+xOo=", socket.Head, StringComparison.Ordinal);
        }
        using (var socket = new Socket(server, "/gql", "Sec-WebSocket-Protocol: other, graphql-transport-ws\r\n"))
        {
            Assert.Contains("Protocol: graphql-transport-ws\r\n", socket.Head, StringComparison.Ordinal);
        }
        // A missing key still gets an answer.
        using var connection = server.Connect();
        Assert.StartsWith("HTTP/1.1 101", connection.Exchange("GET /gql HTTP/1.1\r\nUpgrade: websocket\r\n\r\n").Head, StringComparison.Ordinal);
    }

    [Fact]
    public void FramesJoinAndControlFramesInterleave()
    {
        using var server = new Running();
        using var socket = Socket.Modern(server);
        var ping = Ascii("""{"type":"ping"}""");

        // Split over a fragment, with a ping and a pong between the parts.
        socket.Write(Frame(0x01, ping[..5]));
        socket.Write(Frame(0x89, Ascii("hello")));
        Assert.Equal((10, "hello"), socket.Raw());
        socket.Write(Frame(0x8a, Ascii("late")));
        socket.Write(Frame(0x80, ping[5..]));
        Assert.Equal("pong", socket.Kind());

        // Without a mask, and with each length encoding.
        socket.Write(Frame(0x81, ping, masked: false));
        Assert.Equal("pong", socket.Kind());
        foreach (var size in new[] { 200, 70_000 })
        {
            socket.Send($$"""{"type":"ping"{{new string(' ', size)}}}""");
            Assert.Equal("pong", socket.Kind());
        }

        // What cannot be understood is passed over.
        foreach (var text in new[] { "not json", "{}", """{"type":"unknown"}""", """{"type":1}""" })
        {
            socket.Send(text);
        }
        socket.Send("""{"type":"ping"}""");
        Assert.Equal("pong", socket.Kind());

        // A close is echoed, and ends the connection.
        socket.Write(Frame(0x88, Ascii("bye")));
        Assert.Equal((8, "bye"), socket.Raw());
        socket.AssertClosed();
    }

    [Fact]
    public void AFrameOverTheLimitOrCutShortEndsTheConnection()
    {
        using var server = new Running();
        using (var socket = Socket.Modern(server))
        {
            socket.Write([0x81, 0x7f, .. Enumerable.Repeat((byte)0xff, 8)]);
            socket.AssertClosed();
        }
        // Each step of a frame can be the last one.
        foreach (byte[] partial in new byte[][]
        {
            [0x81],
            [0x81, 0xfe, 0],
            [0x81, 0x85, 1, 2],
            [0x81, 0x85, 1, 2, 3, 4, 5],
        })
        {
            using var socket = Socket.Modern(server);
            socket.Write(partial);
            socket.EndWriting();
            socket.AssertClosed();
        }
    }

    [Fact]
    public void ConnectionInitCarriesCredentials()
    {
        using var server = new Running("root");
        var token = server.GuestToken();
        foreach (var payload in new[]
        {
            $$"""{"Authorization":"Bearer {{token}}"}""",
            $$"""{"authorization":"Bearer {{token}}"}""",
            "{\"headers\":{\"Authorization\":\"Bearer " + token + "\"}}",
            """{"headers":{"x-hasura-admin-secret":"root"}}""",
            "{}",
        })
        {
            using var socket = Socket.Modern(server);
            socket.Send($$"""{"type":"connection_init","payload":{{payload}}}""");
            Assert.Equal("connection_ack", socket.Kind());
        }

        // The legacy protocol also starts keeping the connection alive.
        using (var socket = Socket.Legacy(server))
        {
            socket.Init();
            Assert.Equal("ka", socket.Kind());
        }

        using (var socket = Socket.Modern(server))
        {
            socket.Send("""{"type":"connection_init","payload":{"Authorization":"Bearer a.b.c"}}""");
            var refused = socket.Message();
            Assert.Equal("connection_error", refused.Get("type")!.AsStr());
            Assert.NotNull(refused.Get("payload")!.Get("message"));
            socket.AssertClosed();
        }

        // A bad token on the upgrade is only a guest until something better arrives.
        using (var socket = new Socket(server, "/gql", "Authorization: Bearer a.b.c\r\nSec-WebSocket-Protocol: graphql-transport-ws\r\n"))
        {
            socket.Init();
        }
        using var terminated = Socket.Modern(server);
        terminated.Send("""{"type":"connection_terminate"}""");
        terminated.AssertClosed();
    }

    [Fact]
    public void SubscriptionsReportErrorsInTheFormOfTheirProtocol()
    {
        using var server = new Running();

        using var modern = Socket.Modern(server);
        modern.Init();
        modern.Subscribe("subscribe", "1", "subscription { nothing_here { id } }");
        var refused = modern.Message();
        Assert.Equal("error", refused.Get("type")!.AsStr());
        Assert.Equal("1", refused.Get("id")!.AsStr());
        Assert.NotEmpty(refused.Get("payload")!.AsList());

        using var legacy = Socket.Legacy(server);
        legacy.Init();
        Assert.Equal("ka", legacy.Kind());
        legacy.Subscribe("start", "2", "subscription { nothing_here { id } }");
        refused = legacy.Message();
        Assert.Equal("data", refused.Get("type")!.AsStr());
        Assert.NotNull(refused.Get("payload")!.Get("errors"));

        // An id is optional; the reply then names an empty one.
        modern.Send($$"""{"type":"subscribe","payload":{{Json.Object(("query", Json.Str(Watch))).Write()}}}""");
        Assert.Equal("", modern.Message().Get("id")!.AsStr());
    }

    [Fact]
    public void StoppingEndsOnlyTheNamedSubscription()
    {
        using var server = new Running();
        using var socket = Socket.Modern(server);
        socket.Init();
        socket.Subscribe("subscribe", "kept", Watch);
        Assert.Equal("next", socket.Kind());
        socket.Subscribe("subscribe", "ended", Watch);
        Assert.Equal("next", socket.Kind());
        socket.Send("""{"type":"complete","id":"ended"}""");
        // The next start is queued after the stop, so its answer proves the stop was served.
        socket.Subscribe("subscribe", "marker", "subscription { links(where: {id: {_eq: 1}}) { id } }");
        Assert.Equal("next", socket.Kind());

        server.Post(Insert);
        Assert.Equal("kept", socket.Message().Get("id")!.AsStr());
        socket.Send("""{"type":"ping"}""");
        Assert.Equal("pong", socket.Kind());

        // Without an id every subscription of the connection ends.
        using var watcher = Socket.Modern(server);
        watcher.Init();
        watcher.Subscribe("subscribe", "a", Watch);
        Assert.Equal("next", watcher.Kind());
        watcher.Send("""{"type":"complete"}""");
        watcher.Subscribe("subscribe", "marker", "subscription { links(where: {id: {_eq: 1}}) { id } }");
        Assert.Equal("next", watcher.Kind());
        server.Post(Insert);
        Assert.Equal("kept", socket.Message().Get("id")!.AsStr());
        watcher.Send("""{"type":"ping"}""");
        Assert.Equal("pong", watcher.Kind());

        // The legacy protocol confirms a stop.
        using var legacy = Socket.Legacy(server);
        legacy.Init();
        Assert.Equal("ka", legacy.Kind());
        legacy.Subscribe("start", "old", Watch);
        Assert.Equal("data", legacy.Kind());
        legacy.Send("""{"type":"stop","id":"old"}""");
        var done = legacy.Message();
        Assert.Equal("complete", done.Get("type")!.AsStr());
        Assert.Equal("old", done.Get("id")!.AsStr());
    }

    [Fact]
    public void AWriteReachesOnlySubscriptionsWhoseAnswerChanged()
    {
        using var server = new Running();
        using var watcher = Socket.Modern(server);
        watcher.Init();
        watcher.Subscribe("subscribe", "same", "subscription { links(where: {id: {_eq: 1}}) { id } }");
        Assert.Equal("next", watcher.Kind());
        watcher.Subscribe("subscribe", "moves", Watch);
        Assert.Equal("next", watcher.Kind());

        // A mutation run through a subscription start counts as a write too.
        using var writer = Socket.Modern(server);
        writer.Init();
        writer.Subscribe("subscribe", "write", Insert);
        Assert.Equal("next", writer.Kind());

        Assert.Equal("moves", watcher.Message().Get("id")!.AsStr());
        watcher.Send("""{"type":"ping"}""");
        Assert.Equal("pong", watcher.Kind());
    }

    [Fact]
    public void LongAnswersUseTheLongerFrameLengths()
    {
        using var server = new Running();
        using var socket = Socket.Modern(server);
        socket.Init();
        foreach (var (size, id) in new[] { (200, "medium"), (70_000, "long") })
        {
            var text = new string('x', size);
            var answer = server.Post("mutation { insert_links(objects: {type_id: 1, string: {data: {value: \"" + text + "\"}}}) { returning { id } } }");
            var row = answer.Get("data")!.Get("insert_links")!.Get("returning")!.AsList()[0].Get("id")!.AsI64();
            socket.Subscribe("subscribe", id, $"subscription {{ links(where: {{id: {{_eq: {row}}}}}) {{ value }} }}");
            Assert.Contains(text, socket.Message().Write(), StringComparison.Ordinal);
        }
    }

    [Fact]
    public void ASubscriberThatVanishedIsDroppedWhenAWriteReachesIt()
    {
        using var server = new Running();
        var socket = Socket.Modern(server);
        socket.Init();
        socket.Subscribe("subscribe", "gone", Watch);
        Assert.Equal("next", socket.Kind());
        socket.Dispose();
        Assert.NotNull(server.Post(Insert).Get("data"));
        Assert.NotNull(server.Post(Insert).Get("data"));
    }
}
