using System.Net.Sockets;
using System.Text;
using Foundation.Data.Doublets.Cli.Protocol;
using Link.Foundation.Links.Notation.Binary;

namespace Foundation.Data.Doublets.Cli.Tests.Protocol;

/// <summary>LiNo substitution operations over TCP (issue #105).</summary>
public sealed class LinksServerTests
{
    private static TcpClient RawConnection(RunningServer server)
    {
        return new TcpClient("127.0.0.1", server.Port) { ReceiveTimeout = 10_000 };
    }

    [Fact]
    public void CrudWorksOverEveryProtocol()
    {
        foreach (var protocol in RunningServer.Protocols())
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
        using var binary = server.Client(new BinaryLinoProtocol(new BinaryLinoOptions().WithExternalReferences().WithArity(ArityRange.AtLeast(1))));
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
        foreach (var protocol in RunningServer.Protocols())
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

    [Theory]
    [InlineData(AcceptedProtocols.Binary)]
    [InlineData(AcceptedProtocols.Text)]
    public void ServersCanRestrictTheAcceptedProtocol(AcceptedProtocols accept)
    {
        using var server = new RunningServer(new LinksServerOptions { Accept = accept });
        ILinoProtocol accepted = accept == AcceptedProtocols.Binary ? new BinaryLinoProtocol() : new TextLinoProtocol();
        ILinoProtocol refused = accept == AcceptedProtocols.Binary ? new TextLinoProtocol() : new BinaryLinoProtocol();
        using (var client = server.Client(refused))
        {
            var error = Assert.Throws<LinoProtocolException>(() => client.Query("() ((1 1))"));
            Assert.Equal(LinoProtocolErrorKind.Remote, error.Kind);
            Assert.Equal("this server does not accept this protocol", error.Detail);
        }
        using var allowed = server.Client(accepted);
        Assert.Equal("() ((1: 1 1))", allowed.QueryText("() ((1 1))"));
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
                var name = $"w{worker}i{item}";
                Assert.Equal($"() (({name}: {name} {name}))", client.QueryText($"() (({name}: {name} {name}))"));
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
    public void ShuttingDownRightAfterAConnectionArrivesIsClean()
    {
        // A connection the server has accepted but not started handling yet is
        // closed by the shutdown; handling it must not crash the process.
        for (var attempt = 0; attempt < 200; attempt++)
        {
            using var server = new RunningServer();
            using var client = server.Client(new TextLinoProtocol());
        }
    }

    [Fact]
    public void ServersBindToAnEndPointAndClientsConnectToIt()
    {
        using var server = LinksServer.Bind("127.0.0.1:0");
        var endPoint = $"127.0.0.1:{server.LocalEndPoint.Port}";
        using var client = LinksClient.Connect(endPoint, new TextLinoProtocol());

        Assert.IsType<TextLinoProtocol>(client.Protocol);
    }

    [Fact]
    public void ConnectingToAClosedPortIsAnIoError()
    {
        int port;
        using (var server = LinksServer.Bind("127.0.0.1", 0))
        {
            port = server.LocalEndPoint.Port;
        }

        var error = Assert.Throws<LinoProtocolException>(() => LinksClient.Connect("127.0.0.1", port, new TextLinoProtocol()));
        Assert.Equal(LinoProtocolErrorKind.Io, error.Kind);
    }

    [Fact]
    public void AClientThatHangsUpLeavesTheServerServing()
    {
        using var server = new RunningServer();
        using (var connection = RawConnection(server))
        {
            connection.GetStream().Write("() ((1 1))\n.\n"u8);
            using var reader = new StreamReader(connection.GetStream(), new UTF8Encoding(false));
            Assert.Equal("() ((1: 1 1))", reader.ReadLine());
        }
        using var client = server.Client(new TextLinoProtocol());
        Assert.Equal("(1: 1 1)", client.QueryText(""));
    }

    [Fact]
    public void TracingLogsRequestsRepliesAndSocketProblems()
    {
        using var log = new StringWriter();
        var standardError = Console.Error;
        Console.SetError(log);
        try
        {
            using (var server = new RunningServer(new LinksServerOptions { Trace = true }))
            {
                using (var client = server.Client(new TextLinoProtocol()))
                {
                    client.Query("() ((1 1))");
                }
                WaitForLog(log, "[server] client hung up");
            }
            using var bound = LinksServer.Bind("127.0.0.1", 0, new LinksServerOptions { Trace = true });
            // Nagle's algorithm is a TCP option: a UDP socket refuses it.
            using var datagram = new TcpClient { Client = new Socket(AddressFamily.InterNetwork, SocketType.Dgram, ProtocolType.Udp) };
            bound.TryDisableNagle(datagram);
        }
        finally
        {
            Console.SetError(standardError);
        }

        var lines = log.ToString();
        Assert.Contains("[server] request: () ((1 1))", lines, StringComparison.Ordinal);
        Assert.Contains("[server] reply: () ((1: 1 1))", lines, StringComparison.Ordinal);
        Assert.Contains("[server] could not disable Nagle's algorithm: ", lines, StringComparison.Ordinal);
    }

    /// <summary>Waits until the server, on its own thread, has logged <paramref name="line"/>.</summary>
    private static void WaitForLog(StringWriter log, string line)
    {
        var deadline = DateTime.UtcNow.AddSeconds(10);
        while (true)
        {
            // Console.SetError synchronizes writes on the writer Console.Error returns.
            lock (Console.Error)
            {
                if (log.ToString().Contains(line, StringComparison.Ordinal))
                {
                    return;
                }
            }
            Assert.True(DateTime.UtcNow < deadline, $"the server never logged '{line}'");
            Thread.Sleep(10);
        }
    }

    [Fact]
    public void AStoppedServerExecutesNothing()
    {
        using var server = LinksServer.Bind("127.0.0.1", 0);
        server.Shutdown();
        // A request read just before the shutdown must not reach the store, so the store is never touched.
        var reply = server.Execute(null!, LinoFormat.ParseDocument("() ((1 1))"));
        Assert.Equal("server is shutting down", LinksServer.ErrorMessage(reply));
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
