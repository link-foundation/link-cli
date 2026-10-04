using System.Net.Sockets;
using System.Text;
using Foundation.Data.Doublets.Cli.Protocol;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary>LiNo substitution operations over TCP (issue #105).</summary>
public sealed class LinksServerTests
{
    private sealed class RunningServer : IDisposable
    {
        private readonly string _databaseFilename = Path.GetTempFileName();
        private readonly LinksServer _server;
        private readonly Thread _thread;
        private Exception? _failure;

        public RunningServer(LinksServerOptions? options = null)
        {
            _server = LinksServer.Bind("127.0.0.1", 0, options);
            _thread = new Thread(() =>
            {
                try
                {
                    using var links = new NamedTypesDecorator<uint>(_databaseFilename);
                    _server.Serve(links);
                }
                catch (Exception error)
                {
                    _failure = error;
                }
            }) { IsBackground = true };
            _thread.Start();
        }

        public int Port => _server.LocalEndPoint.Port;

        public LinksClient Client(ILinoProtocol protocol) => LinksClient.Connect("127.0.0.1", Port, protocol);

        public void Stop()
        {
            _server.Shutdown();
            Assert.True(_thread.Join(TimeSpan.FromSeconds(30)), "the server did not stop");
            Assert.Null(_failure);
        }

        public void Dispose()
        {
            Stop();
            File.Delete(_databaseFilename);
            File.Delete(NamedTypesDecorator<uint>.MakeNamesDatabaseFilename(_databaseFilename));
        }
    }

    private static IEnumerable<ILinoProtocol> Protocols() =>
        LinoProtocolCodecTests.AllOptions()
            .Select(options => (ILinoProtocol)new BinaryLinoProtocol(options))
            .Prepend(new TextLinoProtocol());

    private static TcpClient RawConnection(RunningServer server)
    {
        return new TcpClient("127.0.0.1", server.Port) { ReceiveTimeout = 10_000 };
    }

    [Fact]
    public void CrudWorksOverEveryProtocol()
    {
        foreach (var protocol in Protocols())
        {
            using var server = new RunningServer();
            using var client = server.Client(protocol);

            // Create.
            Assert.Equal("() ((1: 1 1))", client.QueryText("() ((1 1))"));
            Assert.Equal("() ((2: 2 2))", client.QueryText("() ((2 2))"));
            // Read.
            Assert.Equal("(1: 1 1)\n(2: 2 2)", client.QueryText(""));
            Assert.Equal("((1: 1 1)) ((1: 1 1))", client.QueryText("((1: 1 1)) ((1: 1 1))"));
            // Update.
            Assert.Equal("((1: 1 1)) ((1: 1 2))", client.QueryText("((1: 1 1)) ((1: 1 2))"));
            // Delete.
            Assert.Equal("((1: 1 2)) ()", client.QueryText("((1: 1 2)) ()"));
            Assert.Equal("(2: 2 2)", client.QueryText(""));
        }
    }

    [Fact]
    public void TextAndBinaryRepliesAreIdenticalDocuments()
    {
        using var server = new RunningServer(new LinksServerOptions { AutoCreateMissingReferences = true });
        using var text = server.Client(new TextLinoProtocol());
        using var binary = server.Client(new BinaryLinoProtocol(new BinaryLinoOptions().WithExternalReferences().WithSequences()));
        text.Query("() ((child: father mother))");
        binary.Query("() (('two words': 'it''s' \"x\"))");
        Assert.Equal(text.Query(""), binary.Query(""));
        var listing = text.QueryText("");
        Assert.Contains("(child: father mother)", listing);
        Assert.Contains("('two words': \"it's\" x)", listing);
    }

    [Fact]
    public void QueryErrorsComeBackAsRemoteErrors()
    {
        using var server = new RunningServer();
        foreach (var protocol in Protocols())
        {
            using var client = server.Client(protocol);
            var error = Assert.Throws<LinoProtocolException>(() => client.Query("((99: 1 1)) ()"));
            Assert.Equal(LinoProtocolErrorKind.Remote, error.Kind);
            Assert.False(string.IsNullOrEmpty(error.Detail));
            // The connection stays usable after an error reply.
            Assert.Equal("", client.QueryText(""));
        }
    }

    [Fact]
    public void ARawTextSessionWorksWithCrlf()
    {
        using var server = new RunningServer();
        using var connection = RawConnection(server);
        var stream = connection.GetStream();
        stream.Write("() ((1 1))\r\n.\r\n"u8);
        using var reader = new StreamReader(stream, new UTF8Encoding(false));
        Assert.Equal("() ((1: 1 1))", reader.ReadLine());
        Assert.Equal(".", reader.ReadLine());

        stream.Write(".\n"u8);
        Assert.Equal("(1: 1 1)", reader.ReadLine());
    }

    [Fact]
    public void MalformedMessagesGetAnErrorAndTheConnectionCloses()
    {
        using var server = new RunningServer();
        using var connection = RawConnection(server);
        var stream = connection.GetStream();
        // A binary header announcing more links than the default limit allows.
        stream.Write(new byte[] { 0x10, 0xff, 0xff, 0xff, 0xff, 0x0f });
        using var reader = new StreamReader(stream, new UTF8Encoding(false));
        var reply = reader.ReadToEnd();
        Assert.StartsWith("(error: ", reply, StringComparison.Ordinal);
        Assert.EndsWith("\n.\n", reply, StringComparison.Ordinal);
    }

    [Fact]
    public void ServersCanRestrictTheAcceptedProtocol()
    {
        using var server = new RunningServer(new LinksServerOptions { Accept = AcceptedProtocols.Binary });
        using var text = server.Client(new TextLinoProtocol());
        var error = Assert.Throws<LinoProtocolException>(() => text.Query("() ((1 1))"));
        Assert.Equal(LinoProtocolErrorKind.Remote, error.Kind);
        using var binary = server.Client(new BinaryLinoProtocol());
        Assert.Equal("() ((1: 1 1))", binary.QueryText("() ((1 1))"));
    }

    [Fact]
    public async Task ConcurrentClientsShareOneStore()
    {
        using var server = new RunningServer();
        var workers = Enumerable.Range(0, 4).Select(worker => Task.Run(() =>
        {
            using var client = server.Client(worker % 2 == 0 ? new TextLinoProtocol() : new BinaryLinoProtocol());
            for (var item = 0; item < 5; item++)
            {
                // The C# query processor does not report named creations as
                // changes (unlike the Rust one), so only success is checked here.
                var name = $"w{worker}i{item}";
                client.Query($"() (({name}: {name} {name}))");
            }
        })).ToArray();
        await Task.WhenAll(workers);
        using var reader = server.Client(new TextLinoProtocol());
        var listing = reader.QueryText("").Split('\n');
        Assert.Equal(20, listing.Length);
        Assert.Contains("(w3i4: w3i4 w3i4)", listing);
    }

    [Fact]
    public void ShutdownStopsTheServer()
    {
        using var server = new RunningServer();
        using var client = server.Client(new TextLinoProtocol());
        client.Query("() ((1 1))");
        server.Stop();
        Assert.ThrowsAny<Exception>(() => client.Query(""));
    }

    [Fact]
    public void EndPointsAreParsed()
    {
        Assert.Equal(("127.0.0.1", 8080), LinksServer.ParseEndPoint("127.0.0.1:8080"));
        Assert.Equal(("::1", 0), LinksServer.ParseEndPoint("[::1]:0"));
        Assert.Throws<ArgumentException>(() => LinksServer.ParseEndPoint("localhost"));
        Assert.Throws<ArgumentException>(() => LinksServer.ParseEndPoint("localhost:70000"));
    }
}
