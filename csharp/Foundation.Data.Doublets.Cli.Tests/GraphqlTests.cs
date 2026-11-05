using System.Net;
using System.Net.Sockets;
using System.Security.Cryptography;
using System.Text;
using Foundation.Data.Doublets.Cli.Graphql;

namespace Foundation.Data.Doublets.Cli.Tests;

public class GraphqlTests
{
    private static string Vectors(string name)
    {
        for (var directory = new DirectoryInfo(AppContext.BaseDirectory); directory is not null; directory = directory.Parent)
        {
            var path = Path.Combine(directory.FullName, "docs", "protocol", "graphql", name);
            if (File.Exists(path))
            {
                return File.ReadAllText(path);
            }
        }
        throw new FileNotFoundException(name);
    }

    private static uint? Mapping(Position position, long id) => id switch
    {
        404 => null,
        0 => 99,
        _ => (uint)(id + 100),
    };

    [Fact]
    public void TokensAreSignedAndVerified()
    {
        var secret = Encoding.UTF8.GetBytes("secret");
        var token = Crypto.SignToken(secret, 42, 1);
        Assert.Equal(42UL, Crypto.VerifyToken(secret, token, 2));
        Assert.Throws<FormatException>(() => Crypto.VerifyToken(Encoding.UTF8.GetBytes("other"), token, 2));
        Assert.Throws<FormatException>(() => Crypto.VerifyToken(secret, "a.b", 2));
        var deep = Crypto.SignToken(Encoding.UTF8.GetBytes("3EK6FD+o0+c7tzBNVfjpMkNDi2yARAAKzQlk8O2IKoxQu4nF7EdAh8s3TwpHwrdWT6R"), 7, 1);
        Assert.StartsWith("eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.", deep);
    }

    [Fact]
    public void WebSocketAcceptIsTheRfcExample() =>
        Assert.Equal("s3pPLMBiTxaQ9kYGzzhZRbK+xOo=", Crypto.WebSocketAccept("dGhlIHNhbXBsZSBub25jZQ=="));

    [Fact]
    public void LikeAndNullComparisons()
    {
        Assert.True(Tables.Like("Hello", "H%o", false));
        Assert.True(Tables.Like("Hello", "_ello", false));
        Assert.False(Tables.Like("Hello", "h%", false));
        Assert.True(Tables.Like("Hello", "h%", true));
        Assert.True(Tables.Passes(Json.Null, "_is_null", Json.Bool(true)));
        Assert.False(Tables.Passes(Json.Null, "_neq", Json.Int(1)));
    }

    [Fact]
    public void WhereVectorsCompileToTheRecordedPlans()
    {
        var failures = new List<string>();
        foreach (var vector in Json.Parse(Vectors("where-vectors.json")).AsList())
        {
            var compiled = Planner.Compile(vector.Get("where")!, Mapping);
            var expectedPlan = vector.Get("plan")!.AsStr();
            var expectedExact = vector.Get("exact")!.BoolValue;
            if (compiled.Plan.Render() != expectedPlan || compiled.Exact != expectedExact)
            {
                failures.Add($"{vector.Get("where")!.Write()}: {compiled.Plan.Render()} {compiled.Exact}, expected {expectedPlan} {expectedExact}");
            }
        }
        Assert.True(failures.Count == 0, string.Join('\n', failures));
    }

    [Fact]
    public void ExecuteVectorsAnswerAsRecorded()
    {
        var database = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
        try
        {
            using var links = new NamedTypesDecorator<uint>(database, false);
            var store = new DeepStore(links, null);
            var vectors = Json.Parse(Vectors("execute-vectors.json"));
            store.LoadPackage(vectors.Get("seed")!);
            var settings = new Settings(Encoding.UTF8.GetBytes("secret"), null);
            var failures = new List<string>();
            foreach (var step in vectors.Get("steps")!.AsList())
            {
                var (answer, _) = Exec.Execute(store, settings, step.Get("query")!.AsStr()!, null, step.Get("variables") ?? Json.Null, default);
                var written = answer.Write();
                var passed = step.Get("error")?.AsStr() is { } code
                    ? written.Contains($"\"code\":\"{code}\"", StringComparison.Ordinal)
                    : (answer.Get("data")?.Write() ?? "") == step.Get("data")!.Write();
                if (!passed)
                {
                    failures.Add($"{step.Get("name")!.AsStr()}: {written}");
                }
            }
            Assert.True(failures.Count == 0, string.Join('\n', failures));
        }
        finally
        {
            foreach (var file in Directory.GetFiles(Path.GetDirectoryName(database)!, Path.GetFileName(database) + "*"))
            {
                File.Delete(file);
            }
        }
    }

    [Theory]
    [InlineData("graphql-ws", "start", "data")]
    [InlineData("graphql-transport-ws", "subscribe", "next")]
    public void WebSocketSubprotocolsAnswerASubscription(string protocol, string start, string data)
    {
        var database = Path.Combine(Path.GetTempPath(), Path.GetRandomFileName());
        using var links = new NamedTypesDecorator<uint>(database, false);
        using var server = new GraphqlServer(new IPEndPoint(IPAddress.Loopback, 0));
        var store = new DeepStore(links, null);
        var serving = new Thread(() => server.Serve(store, new Settings(Encoding.UTF8.GetBytes("secret"), null))) { IsBackground = true };
        serving.Start();
        try
        {
            using var client = new TcpClient();
            client.Connect(server.LocalEndpoint);
            var stream = client.GetStream();
            stream.ReadTimeout = 10000;
            stream.Write(Encoding.ASCII.GetBytes($"GET /v1/graphql HTTP/1.1\r\nHost: x\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Protocol: {protocol}\r\n\r\n"));
            var head = new StringBuilder();
            while (!head.ToString().EndsWith("\r\n\r\n", StringComparison.Ordinal))
            {
                head.Append((char)stream.ReadByte());
            }
            Assert.Contains("101 Switching Protocols", head.ToString());
            Assert.Contains($"Sec-WebSocket-Protocol: {protocol}", head.ToString());
            Send(stream, "{\"type\":\"connection_init\"}");
            Assert.Contains("connection_ack", Receive(stream));
            if (protocol == "graphql-ws")
            {
                Assert.Contains("\"ka\"", Receive(stream));
            }
            Send(stream, "{\"type\":\"" + start + "\",\"id\":\"1\",\"payload\":{\"query\":\"subscription { links { id } }\"}}");
            Assert.Equal("{\"type\":\"" + data + "\",\"id\":\"1\",\"payload\":{\"data\":{\"links\":[]}}}", Receive(stream));
        }
        finally
        {
            server.Stop();
            serving.Join(10000);
        }
    }

    private static void Send(Stream stream, string text)
    {
        var payload = Encoding.UTF8.GetBytes(text);
        Assert.True(payload.Length < 126);
        var mask = RandomNumberGenerator.GetBytes(4);
        stream.Write([0x81, (byte)(0x80 | payload.Length)]);
        stream.Write(mask);
        stream.Write(payload.Select((value, at) => (byte)(value ^ mask[at % 4])).ToArray());
    }

    private static string Receive(Stream stream)
    {
        var head = new byte[2];
        stream.ReadExactly(head);
        Assert.True(head[1] < 126);
        var payload = new byte[head[1]];
        stream.ReadExactly(payload);
        return Encoding.UTF8.GetString(payload);
    }
}
