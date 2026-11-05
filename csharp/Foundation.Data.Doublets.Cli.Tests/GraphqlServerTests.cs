using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text;
using Foundation.Data.Doublets.Cli.Graphql;

namespace Foundation.Data.Doublets.Cli.Tests;

/// <summary>
/// The GraphQL server as a client meets it: over sockets, with tokens, across
/// a restart. <c>rust/tests/graphql_server.rs</c> has the same cases.
/// </summary>
public sealed class GraphqlServerTests
{
    private const string Insert = "mutation { insert_links(objects: {type_id: 1}) { returning { id } } }";

    private const string Watch =
        "subscription { links(where: {type_id: {_eq: 1}}, order_by: {id: desc}, limit: 1) { id } }";

    private const string Init = """{"type":"connection_init","payload":{}}""";

    private static string Docs(string name)
    {
        for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory is not null; directory = directory.Parent)
        {
            var path = Path.Combine(directory.FullName, "docs", "protocol", "graphql", name);
            if (File.Exists(path))
            {
                return path;
            }
        }
        throw new FileNotFoundException(name);
    }

    private static string Database()
    {
        var path = Path.Combine(Path.GetTempPath(), $"clink-graphql-{Guid.NewGuid():N}.links");
        return path;
    }

    private static void Remove(string database)
    {
        foreach (var file in Directory.GetFiles(Path.GetDirectoryName(database)!, Path.GetFileName(database) + "*"))
        {
            File.Delete(file);
        }
    }

    /// <summary>The markers of the permission layer and <paramref name="rule"/>, written without a policy.</summary>
    private static async Task WriteRules(string database, string rule)
    {
        var query = $"() ((Type: Type Type) (Extends: Extends Extends) (AllowRead: AllowRead AllowRead) (AllowWrite: AllowWrite AllowWrite) (Anything: Anything Anything) (Doc: Type Doc) {rule})";
        Clink.AssertSucceeded(await Clink.RunAsync("--db", database, "--auto-create-missing-references", query));
    }

    private sealed class Server : IDisposable
    {
        private readonly Process _process;

        private Server(Process process, IPEndPoint endpoint)
        {
            _process = process;
            Endpoint = endpoint;
        }

        public IPEndPoint Endpoint { get; }

        public static async Task<Server> Start(string database, params string[] extra)
        {
            string[] arguments =
            [
                "--db", database, "--serve", "127.0.0.1:0", "--graphql", "--jwt-secret", "s3",
                "--deep-package", Docs("deep-core.json"), .. extra,
            ];
            var info = Clink.StartInfo(arguments);
            info.RedirectStandardError = false;
            var process = Process.Start(info)!;
            using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(60));
            var line = await process.StandardOutput.ReadLineAsync(timeout.Token);
            if (line is null)
            {
                process.Dispose();
                throw new InvalidOperationException("The server exited before listening.");
            }
            return new Server(process, IPEndPoint.Parse(line.Trim().Split(' ')[^1]));
        }

        public TcpClient Connect()
        {
            var client = new TcpClient();
            client.Connect(Endpoint);
            client.ReceiveTimeout = 20000;
            client.SendTimeout = 20000;
            return client;
        }

        public Json Post(string query, string headers = "")
        {
            var body = Encoding.UTF8.GetBytes(Json.Object(("query", Json.Str(query))).Write());
            using var client = Connect();
            var stream = client.GetStream();
            var head = $"POST /v1/graphql HTTP/1.1\r\nHost: x\r\nConnection: close\r\nContent-Type: application/json\r\n{headers}Content-Length: {body.Length}\r\n\r\n";
            stream.Write(Encoding.ASCII.GetBytes(head));
            stream.Write(body);
            var length = 0;
            while (true)
            {
                var line = ReadLine(stream);
                if (line.Length == 0)
                {
                    break;
                }
                if (line.StartsWith("content-length:", StringComparison.OrdinalIgnoreCase))
                {
                    length = int.Parse(line["content-length:".Length..].Trim(), System.Globalization.CultureInfo.InvariantCulture);
                }
            }
            var answer = new byte[length];
            stream.ReadExactly(answer);
            return Json.Parse(Encoding.UTF8.GetString(answer));
        }

        public string Guest()
        {
            var guest = Post("query { guest { token linkId } }").Get("data")!.Get("guest")!;
            Assert.NotNull(guest.Get("linkId")!.AsI64());
            return guest.Get("token")!.AsStr()!;
        }

        public void Dispose()
        {
            _process.Kill(entireProcessTree: true);
            _process.WaitForExit();
            _process.Dispose();
        }
    }

    private static string ReadLine(NetworkStream stream)
    {
        var bytes = new List<byte>();
        while (true)
        {
            var next = stream.ReadByte();
            if (next < 0)
            {
                throw new EndOfStreamException();
            }
            if (next == '\n')
            {
                return Encoding.ASCII.GetString(bytes.ToArray()).TrimEnd('\r');
            }
            bytes.Add((byte)next);
        }
    }

    private static long[] Ids(Json answer, string table)
    {
        var rows = answer.Get("data")?.Get(table) ?? throw new InvalidOperationException($"no {table} in {answer.Write()}");
        return rows.AsList().Select(row => row.Get("id")!.AsI64()!.Value).ToArray();
    }

    private static string Bearer(string token) => $"Authorization: Bearer {token}\r\n";

    private static long Inserted(Json answer)
    {
        var returning = answer.Get("data")?.Get("insert_links")?.Get("returning")
            ?? throw new InvalidOperationException($"nothing inserted: {answer.Write()}");
        return returning.AsList()[0].Get("id")!.AsI64()!.Value;
    }

    private static string Start(string type, string query) => Json.Object(
        ("id", Json.Str("1")),
        ("type", Json.Str(type)),
        ("payload", Json.Object(("query", Json.Str(query))))).Write();

    /// <summary>A WebSocket client: frames it sends are masked, as a server demands.</summary>
    private sealed class Socket : IDisposable
    {
        private readonly TcpClient _client;
        private readonly NetworkStream _stream;

        public Socket(Server server, string protocol)
        {
            _client = server.Connect();
            _stream = _client.GetStream();
            _stream.Write(Encoding.ASCII.GetBytes(
                $"GET /v1/graphql HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\nSec-WebSocket-Protocol: {protocol}\r\n\r\n"));
            var head = new StringBuilder();
            while (true)
            {
                var line = ReadLine(_stream);
                if (line.Length == 0)
                {
                    break;
                }
                head.Append(line).Append('\n');
            }
            var text = head.ToString();
            Assert.StartsWith("HTTP/1.1 101", text, StringComparison.OrdinalIgnoreCase);
            Assert.Contains("s3pPLMBiTxaQ9kYGzzhZRbK+xOo=", text, StringComparison.Ordinal);
            Assert.Contains($"sec-websocket-protocol: {protocol}", text, StringComparison.OrdinalIgnoreCase);
        }

        public void Send(string text)
        {
            byte[] mask = [1, 2, 3, 4];
            var payload = Encoding.UTF8.GetBytes(text);
            var frame = new List<byte> { 0x81 };
            if (payload.Length < 126)
            {
                frame.Add((byte)(0x80 | payload.Length));
            }
            else
            {
                frame.Add(0x80 | 126);
                frame.Add((byte)(payload.Length >> 8));
                frame.Add((byte)payload.Length);
            }
            frame.AddRange(mask);
            frame.AddRange(payload.Select((value, index) => (byte)(value ^ mask[index % 4])));
            _stream.Write(frame.ToArray());
        }

        private Json Receive()
        {
            var head = new byte[2];
            _stream.ReadExactly(head);
            long length = head[1] & 0x7f;
            if (length == 126)
            {
                var size = new byte[2];
                _stream.ReadExactly(size);
                length = (size[0] << 8) | size[1];
            }
            else if (length == 127)
            {
                var size = new byte[8];
                _stream.ReadExactly(size);
                length = size.Aggregate(0L, (sum, value) => (sum << 8) | value);
            }
            var payload = new byte[length];
            _stream.ReadExactly(payload);
            return Json.Parse(Encoding.UTF8.GetString(payload));
        }

        /// <summary>The next message of a type, past keep-alives.</summary>
        public Json WaitFor(string kind)
        {
            while (true)
            {
                var message = Receive();
                var type = message.Get("type")?.AsStr();
                if (type == kind)
                {
                    return message;
                }
                Assert.True(type != "error", message.Write());
            }
        }

        public void Dispose() => _client.Dispose();
    }

    private static async Task Subscribes(string protocol, string start, string data)
    {
        var database = Database();
        try
        {
            using var server = await Server.Start(database);
            using var socket = new Socket(server, protocol);
            socket.Send(Init);
            socket.WaitFor("connection_ack");
            socket.Send(start);
            var first = socket.WaitFor(data);
            var id = Inserted(server.Post(Insert));
            var update = socket.WaitFor(data);
            Assert.NotEqual(first.Write(), update.Write());
            Assert.Equal([id], Ids(update.Get("payload")!, "links"));
        }
        finally
        {
            Remove(database);
        }
    }

    [Fact(Timeout = 120000)]
    public Task LegacySubscriptionSeesAWrite() => Subscribes("graphql-ws", Start("start", Watch), "data");

    [Fact(Timeout = 120000)]
    public Task GraphqlWsSubscriptionSeesAWrite() => Subscribes("graphql-transport-ws", Start("subscribe", Watch), "next");

    [Fact(Timeout = 120000)]
    public async Task TheTokenIsTheSubjectUnderPermissions()
    {
        var database = Database();
        try
        {
            // Whoever it is may create: creating needs some right to write.
            await WriteRules(database, "(AllowWrite (Anything Doc))");
            using var server = await Server.Start(database, "--permissions", "self", "--graphql-admin-secret", "root");
            const string all = "query { links(where: {id: {_lte: 2}}) { id } }";
            Assert.Empty(Ids(server.Post(all), "links"));
            var admin = server.Post(all, "x-hasura-admin-secret: root\r\n");
            Assert.True(Ids(admin, "links").Length == 2, admin.Write());
            var wrong = server.Post(all, "x-hasura-admin-secret: wrong\r\n");
            Assert.True(wrong.Get("errors") is not null, wrong.Write());

            // What a subject created is its own: visible to it, not to another.
            var token = server.Guest();
            var other = server.Guest();
            // Its own type, as a type it could not write would be refused.
            var own = Inserted(server.Post(
                "mutation { insert_links(objects: {id: 9000, type_id: 9000}) { returning { id } } }",
                Bearer(token)));
            var byId = $"query {{ links(where: {{id: {{_eq: {own}}}}}) {{ id }} }}";
            Assert.Equal([own], Ids(server.Post(byId, Bearer(token)), "links"));
            Assert.Empty(Ids(server.Post(byId, Bearer(other)), "links"));
            Assert.Empty(Ids(server.Post(byId), "links"));

            // The same over a WebSocket, whose token comes in connection_init.
            using var socket = new Socket(server, "graphql-ws");
            socket.Send("{\"type\":\"connection_init\",\"payload\":{\"headers\":{\"Authorization\":\"Bearer " + token + "\"}}}");
            socket.WaitFor("connection_ack");
            socket.Send(Start("start", byId));
            Assert.Equal([own], Ids(socket.WaitFor("data").Get("payload")!, "links"));

            // A forged token is refused.
            var forged = server.Post(all, Bearer("a.b.c"));
            Assert.True(forged.Get("errors") is not null, forged.Write());
        }
        finally
        {
            Remove(database);
        }
    }

    [Fact(Timeout = 120000)]
    public async Task CanShowsTheRulesOfThePolicy()
    {
        var database = Database();
        try
        {
            await WriteRules(database, "(AllowRead (Anything Anything))");
            using var server = await Server.Start(database, "--permissions", "self");
            Assert.Equal(
                """{"data":{"can":[{"subject_id":0,"object_id":0}]}}""",
                server.Post("query { can { subject_id object_id } }").Write());
            Assert.Equal([1L], Ids(server.Post("query { links(where: {id: {_eq: 1}}) { id } }"), "links"));
        }
        finally
        {
            Remove(database);
        }
    }

    [Fact(Timeout = 120000)]
    public async Task LinksSurviveARestart()
    {
        var database = Database();
        try
        {
            long id;
            using (var server = await Server.Start(database))
            {
                id = Inserted(server.Post(
                    """mutation { insert_links(objects: {type_id: 1, string: {data: {value: "kept"}}}) { returning { id } } }"""));
            }
            using var restarted = await Server.Start(database);
            var answer = restarted.Post($"query {{ links(where: {{id: {{_eq: {id}}}}}) {{ id value }} }}");
            Assert.Equal([id], Ids(answer, "links"));
            Assert.Contains("kept", answer.Write(), StringComparison.Ordinal);
        }
        finally
        {
            Remove(database);
        }
    }

    [Fact]
    public void TheEmbeddedSchemaIsTheRecordedOne()
    {
        var recorded = File.ReadAllBytes(Docs("introspection.json"));
        var assembly = typeof(Json).Assembly;
        var name = Assert.Single(assembly.GetManifestResourceNames(), resource => resource.EndsWith("introspection.json", StringComparison.Ordinal));
        using var embedded = assembly.GetManifestResourceStream(name)!;
        using var copy = new MemoryStream();
        embedded.CopyTo(copy);
        Assert.Equal(recorded, copy.ToArray());
    }
}
